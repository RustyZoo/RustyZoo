//! A read-only virtual file system over the resource search path of an installation.
//!
//! The game reads `[resource] path` from `zoo.ini` (or `zoodemo.ini` in the base trial): a
//! `;`-separated list of directories relative to the install folder. The directories make one
//! ordered list of sources, and a name is served by the first source that has it:
//!
//! - the directories in the order of the path, so a directory listed earlier wins;
//! - inside a directory, first its loose files (every file below it, at any depth, by its path
//!   relative to the directory), then its archives (`*.ztd` and `*.zip`, ZIP files) in descending
//!   byte order of their lower-cased file names, so `magnet02.ztd` wins over `magnet01.ztd`,
//!   which wins over `magnet.ztd`.
//!
//! Names are matched after lower-casing the letters A to Z and turning `\` into `/`; nothing else
//! is touched. The order never depends on the order in which the operating system lists a folder.
//!
//! Like the game, the file system is strict about archives. One is ignored (with a warning) unless
//! the ZIP end record is exactly the last 22 bytes of the file, the archive is in one piece and has
//! entries. Members of size 0 are not indexed, so a later source can supply the name. A damaged
//! central directory ends the index at the last good record.
//!
//! The CRC-32 of a member is not checked. That the original does not check it either is an
//! assumption, not something we have established; every member that supplies a name in the full
//! game and the two trials has a matching CRC.
//!
//! Only problems in the content skip a copy in favour of the next: a file that is gone, not a ZIP
//! file, too short, not inflatable. Any other error of the operating system (permissions, the
//! limit of open files, a disk error) fails with a message that names the file, because serving
//! an older copy of a name in its place would go unnoticed.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use flate2::read::DeflateDecoder;

use crate::config::{Config, MAX_INPUT_LEN, TooLarge};

/// The largest file `Vfs::read` accepts, by its size on disk or its declared uncompressed size:
/// 256 MiB. The biggest member in the game is below 2 MiB; the limit only stops absurd sizes in
/// corrupt or hostile data from being read into memory.
pub const MAX_MEMBER_SIZE: u64 = 256 * 1024 * 1024;

/// The longest name the game looks up, in characters. A longer name is never found.
const MAX_NAME_LENGTH: usize = 259;

/// Why an installation could not be opened or a member could not be read. The message is a
/// single line meant to be shown to the user.
#[derive(Debug)]
pub struct VfsError(String, bool);

impl VfsError {
    /// Whether the operating system failed (permissions, the descriptor limit, a disk error)
    /// rather than the content being wrong (no such name, an empty or damaged file, a size over
    /// the limit). Callers that can go on without a file stop on the first kind only.
    pub fn is_system(&self) -> bool {
        self.1
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for VfsError {}

fn error<T>(message: String) -> Result<T, VfsError> {
    Err(VfsError(message, false))
}

/// One copy of a name, as reported by [`Vfs::which`].
#[derive(Debug, PartialEq, Eq)]
pub struct Location {
    /// The loose file, or the archive that holds the member, relative to the install folder when
    /// it lies inside it.
    pub path: PathBuf,
    /// Whether the copy is a member of the archive `path` (else `path` is the loose file).
    pub in_archive: bool,
    /// The size of the content in bytes. For a member that is the uncompressed size in the
    /// central directory of the archive; [`Vfs::read`] goes by the local header instead, so a
    /// damaged archive can read a different number of bytes.
    pub size: u64,
}

/// Where a member is stored in its archive.
struct Member {
    /// The position of the local header of the member.
    offset: u64,
    size: u64,
}

/// Something that can supply names. The sources of a `Vfs` are asked in order.
enum Source {
    /// Normalised name to the loose file, for every file below one search directory.
    Directory(HashMap<String, PathBuf>),
    /// An archive, kept open: reading a member seeks to it. The lock serialises reads.
    Archive {
        path: PathBuf,
        file: Mutex<File>,
        members: HashMap<String, Member>,
    },
}

/// Why a source did not supply a name it has.
enum Failure {
    /// The search ends and the request fails. The original does this for undecodable members;
    /// we also do it for every error of the operating system that is not about the content.
    Stop(String),
    /// As `Stop`, because the operating system failed, not the content.
    System(String),
    /// The content of the copy is broken: the search goes on with the next source.
    Damaged(String),
}

/// Sorts an error of the operating system. Only a file that is gone or whose content is wrong
/// (too short, not inflatable) is `Damaged` and skipped. Anything else (a descriptor limit,
/// permissions, a disk error) stops the request: skipping would silently serve an older copy.
fn io_failure(e: io::Error) -> Failure {
    use io::ErrorKind::*;
    match e.kind() {
        NotFound | UnexpectedEof | InvalidData | InvalidInput => Failure::Damaged(e.to_string()),
        _ => Failure::System(e.to_string()),
    }
}

/// `Ok(false)` if the data ended or was wrong, an error for any other failure of the system.
fn complete(result: io::Result<()>) -> Result<bool, Failure> {
    match result {
        Ok(()) => Ok(true),
        Err(e) => match io_failure(e) {
            Failure::Damaged(_) => Ok(false),
            stop => Err(stop),
        },
    }
}

impl Source {
    fn names(&self) -> Vec<&String> {
        match self {
            Source::Directory(files) => files.keys().collect(),
            Source::Archive { members, .. } => members.keys().collect(),
        }
    }

    fn has(&self, name: &str) -> bool {
        match self {
            Source::Directory(files) => files.contains_key(name),
            Source::Archive { members, .. } => members.contains_key(name),
        }
    }

    /// `Ok(None)` if the source does not have the name.
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, Failure> {
        match self {
            Source::Directory(files) => {
                let Some(path) = files.get(name) else {
                    return Ok(None);
                };
                read_loose_file(path)
                    .map(Some)
                    .map_err(|f| f.within(&path.display().to_string(), name))
            }
            Source::Archive {
                path,
                file,
                members,
            } => {
                let Some(member) = members.get(name) else {
                    return Ok(None);
                };
                let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                read_member(&mut file, member)
                    .map(Some)
                    .map_err(|f| f.within(&path.display().to_string(), name))
            }
        }
    }
}

impl Failure {
    fn within(self, container: &str, name: &str) -> Failure {
        let wrap = |reason: String| format!("{container}: cannot read {name}: {reason}");
        match self {
            Failure::Stop(reason) => Failure::Stop(wrap(reason)),
            Failure::System(reason) => Failure::System(wrap(reason)),
            Failure::Damaged(reason) => Failure::Damaged(wrap(reason)),
        }
    }
}

/// The sources of every directory on the resource search path of one installation.
pub struct Vfs {
    root: PathBuf,
    ini: Vec<u8>,
    sources: Vec<Source>,
    warnings: Vec<String>,
}

impl Vfs {
    /// Opens the installation in `install_dir`: indexes the loose files of the directories of
    /// the search path and the members of their archives. Loose files are not read until
    /// requested.
    ///
    /// Fails if the folder or its ini file is missing or unusable, or if a directory of the
    /// search path cannot be listed. Directories of the search path that do not exist are
    /// skipped. Archives that are not acceptable ZIP files are ignored and reported by
    /// [`warnings`](Vfs::warnings); an archive that cannot be read for any other reason (the
    /// operating system refuses to open or read it) fails the open, naming the file.
    pub fn open(install_dir: &Path) -> Result<Vfs, VfsError> {
        let ini_path = ["zoo.ini", "zoodemo.ini"]
            .iter()
            .find_map(|name| find_ignoring_case(install_dir, name, false).transpose())
            .transpose()?
            .ok_or_else(|| {
                VfsError(
                    format!("no zoo.ini or zoodemo.ini in {}", install_dir.display()),
                    false,
                )
            })?;
        let ini = read_ini(&ini_path)
            .map_err(|e| VfsError(format!("cannot read {}: {e}", ini_path.display()), false))?;
        // Without a path the game uses the install folder itself.
        let search_path =
            resource_path(&crate::vfs::name_from_bytes(&ini)).unwrap_or(".".to_string());

        let real_root = install_dir.canonicalize().map_err(|e| {
            VfsError(
                format!("cannot read folder {}: {e}", install_dir.display()),
                false,
            )
        })?;
        let mut directories: Vec<PathBuf> = Vec::new();
        for entry in search_path
            .split(';')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            if let Some(directory) = resolve_directory(install_dir, &real_root, entry)?
                && !directories.contains(&directory)
            {
                directories.push(directory);
            }
        }
        if directories.is_empty() {
            return error(format!(
                "[resource] path in {} lists no directory",
                ini_path.display()
            ));
        }

        let mut vfs = Vfs {
            root: install_dir.to_path_buf(),
            ini,
            sources: Vec::new(),
            warnings: Vec::new(),
        };
        for directory in &directories {
            let files = index_directory(directory, &mut vfs.warnings)?;
            // The archives are the files directly in the directory, last name first.
            let mut archives: Vec<(&String, &PathBuf)> = files
                .iter()
                .filter(|(name, _)| {
                    !name.contains('/') && (name.ends_with(".ztd") || name.ends_with(".zip"))
                })
                .collect();
            archives.sort_by(|a, b| b.0.cmp(a.0));
            let archives: Vec<PathBuf> = archives.into_iter().map(|(_, p)| p.clone()).collect();
            vfs.sources.push(Source::Directory(files));
            for path in archives {
                match open_archive(&path, &mut vfs.warnings) {
                    Ok(source) => vfs.sources.push(source),
                    Err(Failure::Damaged(why)) => vfs
                        .warnings
                        .push(format!("{} is ignored: {why}", path.display())),
                    Err(Failure::Stop(why) | Failure::System(why)) => {
                        return error(format!("cannot use {}: {why}", path.display()));
                    }
                }
            }
        }
        Ok(vfs)
    }

    /// Reads the whole file or member into memory. The name is matched without regard to letter
    /// case, and `/` and `\` are the same separator; a leading `./` or `/` is not removed.
    ///
    /// Fails if no source has the name, if the copy that wins is empty (an empty loose file hides
    /// the name from all later sources), has a size over [`MAX_MEMBER_SIZE`], or is encrypted or
    /// compressed in a way other than "stored" and "deflated". A copy whose content is damaged (a
    /// file that is gone, not a ZIP file, a truncated or uninflatable member) is skipped in favour
    /// of the next source that has the name, and is reported only if there is none. Any other
    /// error of the operating system (permissions, the descriptor limit, a disk error) fails the
    /// read with a message that names the file: the next copy is never served in its place.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, VfsError> {
        let normalised = normalise(name);
        let mut damaged = None;
        if (1..=MAX_NAME_LENGTH).contains(&normalised.chars().count()) {
            for source in &self.sources {
                match source.read(&normalised) {
                    Ok(Some(data)) => return Ok(data),
                    Ok(None) => {}
                    Err(Failure::Stop(reason)) => return error(reason),
                    Err(Failure::System(reason)) => return Err(VfsError(reason, true)),
                    Err(Failure::Damaged(reason)) => damaged = damaged.or(Some(reason)),
                }
            }
        }
        error(damaged.unwrap_or_else(|| format!("no such member: {name}")))
    }

    /// The content of every copy of the name that can be read, in search order: the first one is
    /// what [`read`](Vfs::read) serves, the others are shadowed by it. For surveys of the data,
    /// not for loading it. A copy with a damaged content is left out.
    pub fn read_copies(&self, name: &str) -> Result<Vec<Vec<u8>>, VfsError> {
        let normalised = normalise(name);
        let mut copies = Vec::new();
        for source in &self.sources {
            match source.read(&normalised) {
                Ok(Some(data)) => copies.push(data),
                Ok(None) | Err(Failure::Damaged(_)) => {}
                Err(Failure::Stop(reason)) => return error(reason),
                Err(Failure::System(reason)) => return Err(VfsError(reason, true)),
            }
        }
        Ok(copies)
    }

    /// Every copy of the name, in search order: the first one is what [`read`](Vfs::read) serves
    /// (unless it fails), the others are shadowed by it. Empty if no source has the name or the
    /// name is too long or empty for `read` to look it up.
    ///
    /// Fails if the size of a loose file cannot be read.
    pub fn which(&self, name: &str) -> Result<Vec<Location>, VfsError> {
        let normalised = normalise(name);
        let mut copies = Vec::new();
        if !(1..=MAX_NAME_LENGTH).contains(&normalised.chars().count()) {
            return Ok(copies);
        }
        for source in self.sources.iter().filter(|s| s.has(&normalised)) {
            let (path, in_archive, size) = match source {
                Source::Directory(files) => {
                    let path = &files[&normalised];
                    let size = std::fs::metadata(path)
                        .map_err(|e| {
                            VfsError(format!("cannot read {}: {e}", path.display()), false)
                        })?
                        .len();
                    (path, false, size)
                }
                Source::Archive { path, members, .. } => (path, true, members[&normalised].size),
            };
            copies.push(Location {
                path: path.strip_prefix(&self.root).unwrap_or(path).to_path_buf(),
                in_archive,
                size,
            });
        }
        Ok(copies)
    }

    /// The normalised paths (lower case, `/` separators) of all loose files and members, or of
    /// those matching `pattern`, sorted. In a pattern `*` matches any run of characters, `/`
    /// included; every other character stands for itself, regardless of letter case or separator.
    // ponytail: this lists every member of every archive. The game's own listing by prefix and
    // suffix (how it discovers its configuration files) offers only the members ending in `.cfg`,
    // `.uca`, `.ucb`, `.ucs` or `.scn`; the code that discovers configuration files must apply
    // that filter (`listed`), this listing is for `ls`.
    pub fn list(&self, pattern: Option<&str>) -> Vec<String> {
        let pattern = pattern.map(normalise);
        // Filter first: building the set of all names for every call is what costs.
        let matching: HashSet<&String> = self
            .sources
            .iter()
            .flat_map(Source::names)
            .filter(|path| {
                pattern
                    .as_ref()
                    .is_none_or(|p| wildcard_matches(p.as_bytes(), path.as_bytes()))
            })
            .collect();
        let mut paths: Vec<String> = matching.into_iter().cloned().collect();
        paths.sort();
        paths
    }

    /// The normalised names that start with `prefix` and end with `suffix`, as the game lists its
    /// files by prefix and suffix: the union of all sources, each name once, sorted. A directory
    /// offers all its names; an archive offers only its members of the record types (see
    /// `first_record_wins`). The prefix and the suffix are plain strings, not patterns.
    pub fn listed(&self, prefix: &str, suffix: &str) -> Vec<String> {
        let (prefix, suffix) = (normalise(prefix), normalise(suffix));
        let mut matching = HashSet::new();
        for source in &self.sources {
            let archive = matches!(source, Source::Archive { .. });
            for name in source.names() {
                if (!archive || first_record_wins(name))
                    && name.starts_with(&prefix)
                    && name.ends_with(&suffix)
                {
                    matching.insert(name);
                }
            }
        }
        let mut paths: Vec<String> = matching.into_iter().cloned().collect();
        paths.sort();
        paths
    }

    /// Whether [`read`](Vfs::read) can find the name: the copy that wins is not empty (an empty
    /// loose file hides the name from all later sources). The content is not checked, so a
    /// damaged member still counts. The path is matched like in [`Vfs::read`].
    pub fn contains(&self, path: &str) -> bool {
        self.which(path)
            .ok()
            .and_then(|copies| copies.first().map(|copy| copy.size > 0))
            .unwrap_or(false)
    }

    fn distinct_names(&self) -> HashSet<&String> {
        self.sources.iter().flat_map(Source::names).collect()
    }

    /// What was wrong with the installation, in plain words, one line each: archives that were
    /// ignored, a damaged central directory, symbolic links that were not followed, files whose
    /// names differ only in letter case. Empty on a healthy installation.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The install folder this was opened on.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The bytes of the ini file next to the executable.
    pub fn ini(&self) -> &[u8] {
        &self.ini
    }

    /// Puts the keys of `over` on top of the ini file, as [`Config::overlay`] does; [`ini`](Vfs::ini)
    /// then returns the merged file in the dialect of [`Config`]. Fails, changing nothing, if the
    /// result would be too large to read.
    pub fn overlay_ini(&mut self, over: &Config) -> Result<(), TooLarge> {
        let mut merged = Config::parse(&self.ini)?;
        merged.overlay(over);
        let bytes = merged.to_bytes();
        if bytes.len() > MAX_INPUT_LEN {
            return Err(TooLarge { len: bytes.len() });
        }
        self.ini = bytes;
        Ok(())
    }

    /// The number of directories of the search path that exist.
    pub fn directory_count(&self) -> usize {
        self.sources
            .iter()
            .filter(|s| matches!(s, Source::Directory(_)))
            .count()
    }

    /// The number of archives (`*.ztd` and `*.zip`) accepted.
    pub fn archive_count(&self) -> usize {
        self.sources
            .iter()
            .filter(|s| matches!(s, Source::Archive { .. }))
            .count()
    }

    /// The number of loose files indexed, counted once for every search directory below which
    /// they lie (the install folder is usually one of them).
    pub fn loose_count(&self) -> usize {
        self.sources
            .iter()
            .map(|source| match source {
                Source::Directory(files) => files.len(),
                Source::Archive { .. } => 0,
            })
            .sum()
    }

    /// The number of distinct names, that is, of loose files and members that can be read.
    pub fn member_count(&self) -> usize {
        self.distinct_names().len()
    }

    /// The number of loose files and members hidden by a copy of the same name found earlier on
    /// the search path.
    pub fn shadowed_count(&self) -> usize {
        let all: usize = self.sources.iter().map(|s| s.names().len()).sum();
        all - self.member_count()
    }
}

fn check_size(size: u64) -> Result<(), Failure> {
    if size > MAX_MEMBER_SIZE {
        return Err(Failure::Stop(format!(
            "it declares {size} bytes, more than the {MAX_MEMBER_SIZE} byte limit"
        )));
    }
    Ok(())
}

fn read_loose_file(path: &Path) -> Result<Vec<u8>, Failure> {
    let mut file = File::open(path).map_err(io_failure)?;
    let size = file.metadata().map_err(io_failure)?.len();
    if size == 0 {
        return Err(Failure::Stop("the file is empty".to_string()));
    }
    check_size(size)?;
    let mut data = buffer(size)?;
    // `take` keeps a file that grows from producing more than it had.
    (&mut file)
        .take(size)
        .read_to_end(&mut data)
        .map_err(io_failure)?;
    if data.len() as u64 != size {
        return Err(Failure::Damaged("the file got shorter".to_string()));
    }
    Ok(data)
}

/// An empty buffer with room for exactly `size` bytes (at most [`MAX_MEMBER_SIZE`]), so that
/// reading does not double it on the way.
fn buffer(size: u64) -> Result<Vec<u8>, Failure> {
    let mut data = Vec::new();
    data.try_reserve_exact(size as usize)
        .map_err(|_| Failure::Stop(format!("no memory for {size} bytes")))?;
    Ok(data)
}

/// The most that deflate can expand its input: 1032 to 1, plus a little for the framing.
fn deflate_limit(compressed: u64) -> u64 {
    compressed * 1032 + 64
}

const LOCAL_SIGNATURE: [u8; 4] = *b"PK\x03\x04";
const CENTRAL_SIGNATURE: [u8; 4] = *b"PK\x01\x02";
const END_SIGNATURE: [u8; 4] = *b"PK\x05\x06";
const END_RECORD_SIZE: u64 = 22;

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Reads a member by its local header, like the game: the sizes, method and flags there decide,
/// not those of the central directory.
fn read_member(file: &mut File, member: &Member) -> Result<Vec<u8>, Failure> {
    let length = file.metadata().map_err(io_failure)?.len();
    file.seek(SeekFrom::Start(member.offset))
        .map_err(io_failure)?;
    let mut header = [0u8; 30];
    file.read_exact(&mut header).map_err(io_failure)?;
    if header[..4] != LOCAL_SIGNATURE {
        return Err(Failure::Damaged("bad local header".to_string()));
    }
    let (flags, method) = (le16(&header, 6), le16(&header, 8));
    let (compressed, size) = (le32(&header, 18) as u64, le32(&header, 22) as u64);
    let skip = le16(&header, 26) as u64 + le16(&header, 28) as u64;
    if flags & 1 != 0 {
        return Err(Failure::Stop("the member is encrypted".to_string()));
    }
    if method != 0 && method != 8 {
        return Err(Failure::Stop(format!("unsupported method {method}")));
    }
    if compressed == 0 || size == 0 {
        return Err(Failure::Stop(
            "its local header declares size 0".to_string(),
        ));
    }
    check_size(size)?;
    if member.offset + 30 + skip + compressed > length {
        return Err(Failure::Damaged(
            "its data runs past the end of the file".to_string(),
        ));
    }
    if method == 8 && size > deflate_limit(compressed) {
        return Err(Failure::Damaged(
            "it declares more bytes than its data can inflate to".to_string(),
        ));
    }
    file.seek(SeekFrom::Current(skip as i64))
        .map_err(io_failure)?;
    let raw = file.take(compressed);
    let mut data = buffer(size)?;
    // `take` keeps a lying header from producing more than the declared size.
    if method == 0 {
        raw.take(size).read_to_end(&mut data)
    } else {
        DeflateDecoder::new(raw).take(size).read_to_end(&mut data)
    }
    .map_err(io_failure)?;
    if data.len() as u64 != size {
        return Err(Failure::Damaged(
            "it is shorter than its declared size".to_string(),
        ));
    }
    Ok(data)
}

/// Accepts the ZIP file or says why not, and indexes its members.
///
/// `Damaged` if the file is not an acceptable ZIP file, `Stop` if the system failed to let us
/// read it.
fn open_archive(path: &Path, warnings: &mut Vec<String>) -> Result<Source, Failure> {
    let bad = |why: &str| Err(Failure::Damaged(why.to_string()));
    let mut file = File::open(path).map_err(io_failure)?;
    let length = file.metadata().map_err(io_failure)?.len();
    if length < END_RECORD_SIZE {
        return bad("too short for a ZIP file");
    }
    let mut end = [0u8; END_RECORD_SIZE as usize];
    file.seek(SeekFrom::End(-(END_RECORD_SIZE as i64)))
        .and_then(|_| file.read_exact(&mut end))
        .map_err(io_failure)?;
    if end[..4] != END_SIGNATURE {
        return bad(
            "it does not end with the ZIP end record (a comment or other bytes after it are not accepted)",
        );
    }
    let (disk, directory_disk) = (le16(&end, 4), le16(&end, 6));
    let (entries_here, entries) = (le16(&end, 8), le16(&end, 10));
    let directory_offset = le32(&end, 16) as u64;
    if disk != 0 || directory_disk != 0 || entries_here != entries {
        return bad("it is split over several disks");
    }
    if entries == 0 {
        return bad("it has no entries");
    }
    if directory_offset > length - END_RECORD_SIZE {
        return bad("its central directory lies beyond the end of the file");
    }

    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(directory_offset))
        .map_err(io_failure)?;
    let mut members: HashMap<String, Member> = HashMap::new();
    let mut indexed = 0;
    for _ in 0..entries {
        let mut record = [0u8; 46];
        if !complete(reader.read_exact(&mut record))? || record[..4] != CENTRAL_SIGNATURE {
            break;
        }
        let (size, offset) = (le32(&record, 24) as u64, le32(&record, 42) as u64);
        let (name_length, rest) = (
            le16(&record, 28) as usize,
            le16(&record, 30) as i64 + le16(&record, 32) as i64,
        );
        let mut name = vec![0u8; name_length];
        if !complete(reader.read_exact(&mut name))? || !complete(reader.seek_relative(rest))? {
            break;
        }
        indexed += 1;
        // Directory entries and empty files are invisible: a later source can supply the name.
        if name.is_empty() || size == 0 {
            continue;
        }
        let name = normalise(&name_from_bytes(&name));
        let member = Member { offset, size };
        match members.entry(name) {
            Entry::Vacant(slot) => {
                slot.insert(member);
            }
            // ponytail: for the names of the five kinds below the first record wins, for all
            // others the last, as in the original. No warning; no shipped archive does this.
            Entry::Occupied(mut slot) => {
                if !first_record_wins(slot.key()) {
                    slot.insert(member);
                }
            }
        }
    }
    if indexed < entries {
        warnings.push(format!(
            "{}: the central directory is damaged after {indexed} of {entries} entries",
            path.display()
        ));
    }
    Ok(Source::Archive {
        path: path.to_path_buf(),
        file: Mutex::new(reader.into_inner()),
        members,
    })
}

fn first_record_wins(name: &str) -> bool {
    [".cfg", ".uca", ".ucb", ".ucs", ".scn"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// A name from raw bytes, without losing any: valid UTF-8 stays as it is and each byte that is not
/// part of a valid sequence becomes the private-use character U+F700 plus the byte, so two names
/// that differ in such bytes stay distinct. Names are compared byte for byte after `normalise`.
pub fn name_from_bytes(bytes: &[u8]) -> String {
    let mut name = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        name.push_str(chunk.valid());
        name.extend(
            chunk
                .invalid()
                .iter()
                .filter_map(|&b| char::from_u32(0xF700 + b as u32)),
        );
    }
    name
}

/// Lower-cases ASCII letters and turns `\` into `/`.
pub fn normalise(path: &str) -> String {
    path.chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Whether `text` matches `pattern`, where `*` matches any run of bytes.
fn wildcard_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    // Where the last `*` was, and how much of the text it has swallowed so far.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

/// The largest ini file we read: the real ones are a few hundred bytes.
const MAX_INI_SIZE: u64 = 1024 * 1024;

/// Reads the ini file, which must be a regular file of at most [`MAX_INI_SIZE`] bytes: a FIFO
/// would hang the read, a device or a huge file would fill the memory.
pub(crate) fn read_ini(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut data = Vec::new();
    file.take(MAX_INI_SIZE + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_INI_SIZE {
        return Err(io::Error::other(format!("more than {MAX_INI_SIZE} bytes")));
    }
    Ok(data)
}

/// The value of `path` in the `[resource]` section of the ini file next to the executable. The
/// game reads that file through the ini reader of Windows, not through its own configuration
/// reader (`crate::config`): names ignore letter case, only `[section]` headers, `key=value`
/// lines and `;` comment lines count, the first `path` wins, and a value in a pair of single
/// or double quotes loses them.
fn resource_path(ini: &str) -> Option<String> {
    let mut in_resource = false;
    for line in ini.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[') {
            in_resource = section
                .strip_suffix(']')
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("resource"));
        } else if let Some((key, value)) = line.split_once('=')
            && in_resource
            && key.trim().eq_ignore_ascii_case("path")
        {
            let value = value.trim();
            let quoted = value.len() >= 2
                && (value.starts_with('"') && value.ends_with('"')
                    || value.starts_with('\'') && value.ends_with('\''));
            return Some(
                if quoted {
                    &value[1..value.len() - 1]
                } else {
                    value
                }
                .to_string(),
            );
        }
    }
    None
}

/// The entry of `directory` called `name` in any letter case, of the given kind (directory or
/// file). If several differ only in case, the first in sorted order is taken.
pub(crate) fn find_ignoring_case(
    directory: &Path,
    name: &str,
    want_directory: bool,
) -> Result<Option<PathBuf>, VfsError> {
    let entries = std::fs::read_dir(directory).map_err(|e| {
        VfsError(
            format!("cannot read folder {}: {e}", directory.display()),
            false,
        )
    })?;
    let entries = entries.collect::<Result<Vec<_>, _>>().map_err(|e| {
        VfsError(
            format!("cannot read folder {}: {e}", directory.display()),
            false,
        )
    })?;
    let mut found: Vec<PathBuf> = entries
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() == want_directory)
        .collect();
    found.sort();
    Ok(found.into_iter().next())
}

/// Resolves one entry of the search path against the install folder. `None` if the directory
/// does not exist. Entries that leave the install folder, by `..` or by a symbolic link, are an
/// error.
fn resolve_directory(
    install_dir: &Path,
    real_root: &Path,
    entry: &str,
) -> Result<Option<PathBuf>, VfsError> {
    let entry = entry.replace('\\', "/");
    let leaves = || {
        error(format!(
            "[resource] path entry \"{entry}\" leaves the install folder"
        ))
    };
    let mut directory = install_dir.to_path_buf();
    for component in entry.split('/').filter(|c| !c.is_empty() && *c != ".") {
        if component == ".." || component.contains(':') {
            return leaves();
        }
        match find_ignoring_case(&directory, component, true)? {
            Some(next) => directory = next,
            None => return Ok(None),
        }
    }
    match directory.canonicalize() {
        Ok(real) if real.starts_with(real_root) => Ok(Some(directory)),
        _ => leaves(),
    }
}

/// A loose file name with the bytes that are not UTF-8 kept apart (see `name_from_bytes`).
fn file_name(name: &std::ffi::OsStr) -> String {
    #[cfg(unix)]
    return name_from_bytes(std::os::unix::ffi::OsStrExt::as_bytes(name));
    #[cfg(not(unix))]
    return name.to_string_lossy().into_owned();
}

/// Every file below `root`, at any depth: normalised path relative to `root` to the file.
///
/// Symbolic links are not followed, neither to files nor to folders: a link could lead out of the
/// install folder, or round in a circle. They are skipped with a warning.
fn index_directory(
    root: &Path,
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, PathBuf>, VfsError> {
    let mut files: HashMap<String, PathBuf> = HashMap::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let cannot = |e: io::Error| {
            VfsError(
                format!("cannot read folder {}: {e}", directory.display()),
                false,
            )
        };
        // A folder or an entry that vanished meanwhile is skipped; any other error would
        // silently hide files, so it fails.
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound && directory != root => continue,
            Err(e) => return Err(cannot(e)),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(cannot(e)),
            };
            let path = entry.path();
            let name = format!("{prefix}{}", file_name(&entry.file_name()));
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return error(format!("cannot read {}: {e}", path.display())),
            };
            match kind {
                kind if kind.is_symlink() => {
                    warnings.push(format!("{} is a symbolic link, ignored", path.display()));
                }
                kind if kind.is_dir() => pending.push((path, format!("{name}/"))),
                kind if kind.is_file() => match files.entry(normalise(&name)) {
                    Entry::Vacant(slot) => {
                        slot.insert(path);
                    }
                    // ponytail: files that differ only in letter case cannot exist on Windows
                    // and the original has no rule; the first in byte order is used. Ceiling:
                    // an install copied onto a case-sensitive file system with such pairs.
                    Entry::Occupied(mut slot) => {
                        let (kept, dropped) = if path < *slot.get() {
                            (path.clone(), slot.get().clone())
                        } else {
                            (slot.get().clone(), path)
                        };
                        warnings.push(format!(
                            "{} differs from {} only in letter case, ignored",
                            dropped.display(),
                            kept.display()
                        ));
                        slot.insert(kept);
                    }
                },
                _ => {}
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    /// A fresh directory under the system temp folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let path =
                std::env::temp_dir().join(format!("rustyzoo-{}-{}-{name}", std::process::id(), {
                    static N: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                }));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        /// Writes a file, creating the folders above it.
        fn write(&self, relative: &str, content: &[u8]) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        /// Writes an archive whose members are stored and deflated alternately.
        fn archive(&self, relative: &str, members: &[(&str, &str)]) {
            self.write(relative, &zip_bytes(members, ""));
        }

        /// A search path of `directories` and nothing else in the ini file.
        fn ini(&self, path: &str) {
            self.write("zoo.ini", format!("[resource]\npath={path}\n").as_bytes());
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn zip_bytes(members: &[(&str, &str)], comment: &str) -> Vec<u8> {
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (i, (name, content)) in members.iter().enumerate() {
            let method = if i % 2 == 0 {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            zip.start_file(
                *name,
                SimpleFileOptions::default().compression_method(method),
            )
            .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        if !comment.is_empty() {
            zip.set_comment(comment).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// The positions at which `needle` starts in `haystack`.
    fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
        (0..haystack.len().saturating_sub(needle.len() - 1))
            .filter(|&i| haystack[i..].starts_with(needle))
            .collect()
    }

    fn message(result: Result<Vfs, VfsError>) -> String {
        result.err().expect("open should fail").to_string()
    }

    fn text(vfs: &Vfs, name: &str) -> String {
        String::from_utf8(vfs.read(name).unwrap()).unwrap()
    }

    /// Two search directories (one missing, one in another letter case) and overlapping members.
    fn sample(name: &str) -> TempDir {
        let dir = TempDir::new(name);
        dir.write("ZOO.INI", b"[resource]\r\npath=./Updates;./nowhere;.\r\n");
        dir.archive("UPDATES/new.ztd", &[("Animals/Lion.AI", "new")]);
        dir.archive(
            "root-b.ztd",
            &[("animals/lion.ai", "old"), ("animals\\Zebra.ai", "zebra")],
        );
        dir.archive(
            "ROOT-A.ZTD",
            &[("Sounds/roar.wav", "roar"), ("animals/zebra.ai", "x")],
        );
        dir.write("readme.txt", b"not an archive");
        dir
    }

    #[test]
    fn earlier_directory_wins_and_lookup_ignores_case_and_separator() {
        let dir = sample("precedence");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.read("animals/lion.ai").unwrap(), b"new");
        assert_eq!(vfs.read("ANIMALS\\LION.AI").unwrap(), b"new");
        assert_eq!(vfs.read("sounds\\Roar.WAV").unwrap(), b"roar");
        assert_eq!(vfs.archive_count(), 3);
        assert!(vfs.warnings().is_empty(), "{:?}", vfs.warnings());
        assert_eq!(
            vfs.list(Some("animals/*")),
            ["animals/lion.ai", "animals/zebra.ai"]
        );
    }

    #[test]
    fn inside_a_directory_the_last_lower_cased_name_wins() {
        let dir = sample("inside");
        // root-b.ztd sorts after ROOT-A.ZTD once lower-cased, so it wins.
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "animals/zebra.ai"), "zebra");
    }

    #[test]
    fn archives_are_ordered_by_bytes_of_the_lower_cased_name() {
        let dir = TempDir::new("order");
        dir.ini(".");
        let names = [
            "m.ztd",
            "M01.ZTD",
            "m02.ztd",
            "mountainlion01.ztd",
            "mt_lion.ztd",
            "x.zip",
            "x.ztd",
            "y.ztd",
            "y.zip",
        ];
        for name in names {
            dir.archive(name, &[("who", name), (&format!("{name}.only"), "-")]);
        }
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), names.len(), "{:?}", vfs.warnings());
        // '.' < digits < '_' < letters; the last name wins; .ztd beats .zip for one base name.
        let order: Vec<_> = vfs
            .which("who")
            .unwrap()
            .into_iter()
            .map(|copy| copy.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            order,
            [
                "y.ztd",
                "y.zip",
                "x.ztd",
                "x.zip",
                "mt_lion.ztd",
                "mountainlion01.ztd",
                "m02.ztd",
                "M01.ZTD",
                "m.ztd"
            ]
        );
        assert_eq!(text(&vfs, "who"), "y.ztd");
    }

    #[test]
    fn a_loose_file_beats_the_archive_of_its_directory_but_not_an_earlier_directory() {
        let dir = TempDir::new("loose");
        dir.ini("./a;./B");
        dir.archive(
            "a/pack.ztd",
            &[
                ("one.txt", "a-archive"),
                ("two.txt", "a-archive"),
                ("sub/deep.txt", "a-archive"),
            ],
        );
        dir.write("a/ONE.txt", b"a-loose");
        dir.write("a/Sub/Deep.TXT", b"a-loose");
        dir.write("b/one.txt", b"b-loose");
        dir.write("b/two.txt", b"b-loose");
        dir.write("b/three.txt", b"b-loose");
        dir.archive("b/pack.ztd", &[("three.txt", "b-archive")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "one.txt"), "a-loose");
        assert_eq!(text(&vfs, "sub\\deep.txt"), "a-loose");
        assert_eq!(
            text(&vfs, "two.txt"),
            "a-archive",
            "earlier archive beats later loose"
        );
        assert_eq!(text(&vfs, "three.txt"), "b-loose");
        let which = vfs.which("one.txt").unwrap();
        assert_eq!(which.len(), 3);
        assert!(!which[0].in_archive && which[1].in_archive && !which[2].in_archive);
        assert_eq!(which[0].path, Path::new("a/ONE.txt"));
        assert_eq!((which[0].size, which[1].size), (7, 9));
        assert_eq!(vfs.loose_count(), 3 + 4);
        // The archives themselves are loose files too, harmlessly.
        assert!(vfs.list(Some("*.ztd")).contains(&"pack.ztd".to_string()));
    }

    #[test]
    fn names_are_matched_exactly_after_normalising() {
        let dir = TempDir::new("exact");
        dir.ini(".");
        dir.write("x/y.txt", b"y");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "X\\Y.TXT"), "y");
        for name in [
            "./x/y.txt",
            "/x/y.txt",
            "x//y.txt",
            "x/../x/y.txt",
            "x/y.txt ",
            "",
        ] {
            assert!(vfs.read(name).is_err(), "{name:?}");
        }
        let long = format!("{}y.txt", "x/".repeat(130));
        assert!(
            vfs.read(&long)
                .unwrap_err()
                .to_string()
                .contains("no such member")
        );
    }

    #[test]
    fn a_member_of_size_zero_is_not_indexed() {
        let dir = TempDir::new("zero");
        dir.ini(".");
        dir.archive("b.ztd", &[("dir/", ""), ("z.txt", ""), ("e.txt", "")]);
        dir.archive("a.ztd", &[("z.txt", "from a")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        // b.ztd sorts first but has nothing visible, so a.ztd supplies the name
        assert_eq!(text(&vfs, "z.txt"), "from a");
        assert!(vfs.read("e.txt").is_err());
        assert_eq!(vfs.which("z.txt").unwrap().len(), 1);
        assert_eq!(
            vfs.archive_count(),
            2,
            "an archive without visible members is accepted"
        );
    }

    #[test]
    fn an_empty_loose_file_hides_the_name() {
        let dir = TempDir::new("empty-loose");
        dir.ini("./a;./b");
        dir.write("a/e.txt", b"");
        dir.archive("b/p.ztd", &[("e.txt", "hidden")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        let why = vfs.read("e.txt").unwrap_err().to_string();
        assert!(why.contains("the file is empty"), "{why}");
        assert_eq!(vfs.which("e.txt").unwrap().len(), 2);
    }

    #[test]
    fn names_with_bytes_that_are_not_utf8_stay_distinct() {
        let dir = TempDir::new("bytes");
        dir.ini(".");
        // Placeholders 1 and 2 become the bytes E9 and E8 in the file, as in a Latin-1 name.
        let mut zip = zip_bytes(&[("a\u{1}.txt", "one"), ("a\u{2}.txt", "two")], "");
        for (from, to) in [(b"a\x01.txt", b"a\xE9.txt"), (b"a\x02.txt", b"a\xE8.txt")] {
            for at in find_all(&zip, from) {
                zip[at..at + to.len()].copy_from_slice(to);
            }
        }
        dir.write("p.ztd", &zip);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, &name_from_bytes(b"a\xE9.txt")), "one");
        assert_eq!(text(&vfs, &name_from_bytes(b"A\xE8.TXT")), "two");
        assert!(vfs.read("a\u{FFFD}.txt").is_err());
        assert!(vfs.read("a\u{E9}.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn loose_files_with_bytes_that_are_not_utf8_stay_distinct() {
        use std::os::unix::ffi::OsStrExt;
        let dir = TempDir::new("loosebytes");
        dir.ini(".");
        for (name, content) in [(&b"a\xE9.txt"[..], "one"), (b"a\xE8.txt", "two")] {
            let path = dir.0.join(std::ffi::OsStr::from_bytes(name));
            if let Err(e) = std::fs::write(path, content) {
                // APFS and similar refuse names that are not valid UTF-8.
                eprintln!("skipped: the file system refuses the name: {e}");
                return;
            }
        }
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, &name_from_bytes(b"a\xE9.txt")), "one");
        assert_eq!(text(&vfs, &name_from_bytes(b"a\xE8.txt")), "two");
        assert_eq!(name_from_bytes("a\u{E9}".as_bytes()), "a\u{E9}");
        assert_eq!(name_from_bytes(b""), "");
    }

    #[test]
    fn archives_the_game_rejects_are_ignored_with_a_warning() {
        let dir = TempDir::new("rejected");
        dir.ini(".");
        dir.archive("a.ztd", &[("x.txt", "good")]);
        // Each of these sorts after a.ztd, so it would win if it were accepted.
        dir.write(
            "b_comment.ztd",
            &zip_bytes(&[("x.txt", "comment")], "hello"),
        );
        let mut trailing = zip_bytes(&[("x.txt", "trailing")], "");
        trailing.push(0);
        dir.write("c_trailing.ztd", &trailing);
        dir.write("d_empty.ztd", &zip_bytes(&[], ""));
        let mut split = zip_bytes(&[("x.txt", "split")], "");
        let end = split.len() - 22;
        split[end + 4] = 1; // this disk is number 1
        dir.write("e_split.ztd", &split);
        let mut uneven = zip_bytes(&[("x.txt", "uneven")], "");
        let end = uneven.len() - 22;
        uneven[end + 8] = 2; // entries on this disk != entries
        dir.write("f_uneven.ztd", &uneven);
        dir.write("g_garbage.ztd", b"this is not a zip file at all, no");
        dir.write("h_tiny.zip", b"PK");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "x.txt"), "good");
        assert_eq!(vfs.archive_count(), 1);
        assert_eq!(vfs.warnings().len(), 7, "{:#?}", vfs.warnings());
        for name in [
            "b_comment",
            "c_trailing",
            "d_empty",
            "e_split",
            "f_uneven",
            "g_garbage",
            "h_tiny",
        ] {
            assert!(
                vfs.warnings()
                    .iter()
                    .any(|w| w.contains(name) && w.contains("is ignored")),
                "{name}"
            );
        }
    }

    #[test]
    fn a_damaged_central_directory_ends_the_index_but_keeps_the_archive() {
        let dir = TempDir::new("damaged-directory");
        dir.ini(".");
        let mut bytes = zip_bytes(
            &[("one.txt", "1"), ("two.txt", "2"), ("three.txt", "3")],
            "",
        );
        let records = find_all(&bytes, &CENTRAL_SIGNATURE);
        assert_eq!(records.len(), 3);
        bytes[records[1]] = b'X';
        dir.write("a.ztd", &bytes);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), 1);
        assert_eq!(text(&vfs, "one.txt"), "1");
        assert!(vfs.read("two.txt").is_err() && vfs.read("three.txt").is_err());
        assert!(
            vfs.warnings()[0].contains("after 1 of 3"),
            "{:?}",
            vfs.warnings()
        );
    }

    #[test]
    fn the_same_name_twice_in_one_archive() {
        let dir = TempDir::new("duplicates");
        dir.ini(".");
        let kinds = ["cfg", "uca", "ucb", "ucs", "scn", "txt"];
        let members: Vec<(String, &str)> = kinds
            .iter()
            .flat_map(|kind| {
                [
                    (format!("a.{kind}"), "first"),
                    (format!("b.{kind}"), "second"),
                ]
            })
            .collect();
        let members: Vec<(&str, &str)> = members.iter().map(|(n, c)| (n.as_str(), *c)).collect();
        let mut bytes = zip_bytes(&members, "");
        // The writer refuses duplicates: rename in the local headers and the directory alike.
        for kind in kinds {
            let (from, to) = (format!("b.{kind}"), format!("a.{kind}"));
            for at in find_all(&bytes, from.as_bytes()) {
                bytes[at..at + from.len()].copy_from_slice(to.as_bytes());
            }
        }
        dir.write("a.ztd", &bytes);
        let vfs = Vfs::open(&dir.0).unwrap();
        for kind in kinds {
            let expected = if kind == "txt" { "second" } else { "first" };
            assert_eq!(
                text(&vfs, &format!("a.{kind}")),
                expected,
                "{kind}: the first record wins for configuration names, the last for others"
            );
        }
    }

    #[test]
    fn an_undecodable_member_stops_the_search_a_damaged_one_does_not() {
        let dir = TempDir::new("decode");
        dir.ini(".");
        dir.archive("a.ztd", &[("x.txt", "fallback"), ("y.txt", "fallback")]);
        // b.ztd wins. Its x.txt is stored (the first member) with a method the game cannot
        // read, its y.txt is deflated and loses its local header signature.
        let mut bytes = zip_bytes(&[("x.txt", "wins"), ("y.txt", "wins")], "");
        let locals = find_all(&bytes, &LOCAL_SIGNATURE);
        bytes[locals[0] + 8] = 99; // method
        bytes[locals[1]] = b'X';
        dir.write("b.ztd", &bytes);
        let vfs = Vfs::open(&dir.0).unwrap();
        let why = vfs.read("x.txt").unwrap_err().to_string();
        assert!(
            why.contains("b.ztd") && why.contains("unsupported method 99"),
            "{why}"
        );
        assert_eq!(text(&vfs, "y.txt"), "fallback");

        // Alone, the damaged member is an error that says what is wrong.
        std::fs::remove_file(dir.0.join("a.ztd")).unwrap();
        let why = Vfs::open(&dir.0)
            .unwrap()
            .read("y.txt")
            .unwrap_err()
            .to_string();
        assert!(
            why.contains("b.ztd") && why.contains("bad local header"),
            "{why}"
        );
    }

    #[test]
    fn encrypted_members_and_local_headers_of_size_zero_stop_the_search() {
        let dir = TempDir::new("local-header");
        dir.ini(".");
        dir.archive(
            "a.ztd",
            &[
                ("x.txt", "fallback"),
                ("y.txt", "fallback"),
                ("z.txt", "fallback"),
            ],
        );
        let mut bytes = zip_bytes(
            &[("x.txt", "wins"), ("y.txt", "wins"), ("z.txt", "wins")],
            "",
        );
        let locals = find_all(&bytes, &LOCAL_SIGNATURE);
        bytes[locals[0] + 6] |= 1; // encrypted
        bytes[locals[1] + 22..locals[1] + 26].fill(0); // uncompressed size 0
        bytes[locals[2] + 18..locals[2] + 22].fill(0); // compressed size 0
        dir.write("b.ztd", &bytes);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert!(
            vfs.read("x.txt")
                .unwrap_err()
                .to_string()
                .contains("encrypted")
        );
        for name in ["y.txt", "z.txt"] {
            let why = vfs.read(name).unwrap_err().to_string();
            assert!(why.contains("size 0"), "{name}: {why}");
        }
    }

    /// An archive `b.ztd` that sorts before `a.ztd` and wins, whose local headers `damage`
    /// changes. Its members are `x.txt` (stored) and `y.txt` (deflated), made of 2000 `w`s so that
    /// the deflated one is much smaller than it declares; `a.ztd` has `fallback` for both.
    fn damaged_winner(name: &str, damage: impl Fn(&mut [u8], &[usize])) -> Vfs {
        let dir = TempDir::new(name);
        dir.ini(".");
        dir.archive("a.ztd", &[("x.txt", "fallback"), ("y.txt", "fallback")]);
        let wins = "w".repeat(2000);
        let mut bytes = zip_bytes(&[("x.txt", &wins), ("y.txt", &wins)], "");
        let locals = find_all(&bytes, &LOCAL_SIGNATURE);
        damage(&mut bytes, &locals);
        dir.write("b.ztd", &bytes);
        Vfs::open(&dir.0).unwrap()
    }

    fn put32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn a_member_shorter_than_its_declared_size_falls_through() {
        // x.txt is stored and its compressed size says 1000 bytes, y.txt inflates to its 2000
        // bytes but declares 5000.
        let vfs = damaged_winner("short", |bytes, locals| {
            put32(bytes, locals[0] + 18, 1000);
            put32(bytes, locals[1] + 22, 5000);
        });
        assert_eq!(text(&vfs, "x.txt"), "fallback");
        assert_eq!(text(&vfs, "y.txt"), "fallback");
    }

    #[test]
    fn a_member_that_runs_past_the_end_of_the_file_falls_through() {
        let vfs = damaged_winner("past-the-end", |bytes, locals| {
            put32(bytes, locals[0] + 18, 1_000_000);
            put32(bytes, locals[1] + 18, 1_000_000);
        });
        assert_eq!(text(&vfs, "x.txt"), "fallback");
        assert_eq!(text(&vfs, "y.txt"), "fallback");
    }

    #[test]
    fn a_size_that_deflate_cannot_reach_falls_through() {
        // 20 bytes of data cannot inflate to 100000 bytes: the most is 1032 to 1.
        let vfs = damaged_winner("ratio", |bytes, locals| {
            put32(bytes, locals[1] + 18, 20);
            put32(bytes, locals[1] + 22, 100_000);
        });
        assert_eq!(text(&vfs, "y.txt"), "fallback");
        assert_eq!(deflate_limit(20), 20 * 1032 + 64);
    }

    #[test]
    fn a_size_over_the_limit_stops_the_search() {
        let vfs = damaged_winner("limit", |bytes, locals| {
            put32(bytes, locals[0] + 22, MAX_MEMBER_SIZE as u32 + 1);
            put32(bytes, locals[1] + 22, u32::MAX);
        });
        for name in ["x.txt", "y.txt"] {
            let why = vfs.read(name).unwrap_err().to_string();
            assert!(why.contains("byte limit"), "{name}: {why}");
        }
    }

    #[test]
    fn a_name_of_259_characters_is_found_one_of_260_is_not() {
        let dir = TempDir::new("long-names");
        dir.ini(".");
        let (short, long) = ("a".repeat(259), "b".repeat(260));
        dir.archive("p.ztd", &[(&short, "short"), (&long, "long")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, &short), "short");
        assert_eq!(vfs.which(&short).unwrap().len(), 1);
        assert!(vfs.read(&long).is_err());
        assert!(
            vfs.which(&long).unwrap().is_empty(),
            "which agrees with read"
        );
        assert!(vfs.which("").unwrap().is_empty());
    }

    #[test]
    fn members_with_an_empty_name_are_not_indexed() {
        let dir = TempDir::new("empty-name");
        dir.ini(".");
        let mut bytes = zip_bytes(&[("x.txt", "content")], "");
        // Empty the name in the central record and skip its bytes as extra data instead.
        let record = find_all(&bytes, &CENTRAL_SIGNATURE)[0];
        bytes[record + 28..record + 30].fill(0);
        bytes[record + 30] = 5;
        dir.write("a.ztd", &bytes);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), 1, "{:?}", vfs.warnings());
        assert!(!vfs.list(None).contains(&String::new()));
        assert!(vfs.read("").is_err());
    }

    #[test]
    fn a_directory_listed_twice_is_indexed_once() {
        let dir = TempDir::new("repeat");
        dir.ini(".;./.;");
        dir.archive("a.ztd", &[("x.txt", "1")]);
        dir.write("y.txt", b"2");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), 1);
        assert_eq!(vfs.which("x.txt").unwrap().len(), 1);
        assert_eq!(vfs.which("y.txt").unwrap().len(), 1);
        assert_eq!(vfs.shadowed_count(), 0);
    }

    #[test]
    fn listed_takes_the_union_of_the_sources_once_with_the_record_types_of_archives() {
        let dir = TempDir::new("listed");
        dir.ini(".");
        dir.write("x/more.scn", b"1");
        dir.archive(
            "a.ztd",
            &[
                ("X/More.SCN", "2"),
                ("x/more01.scn", "3"),
                ("x/more.wav", "4"),
                ("x/more02.cfg", "5"),
            ],
        );
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.listed("X/More", ".SCN"), ["x/more.scn", "x/more01.scn"]);
        assert_eq!(vfs.listed("x/more", ".wav"), Vec::<String>::new());
        assert_eq!(vfs.listed("x/more", ".txt"), Vec::<String>::new());
        assert_eq!(
            vfs.listed("x/more", ""),
            ["x/more.scn", "x/more01.scn", "x/more02.cfg"]
        );
        assert_eq!(vfs.listed("", ".scn").len(), 2);
    }

    #[test]
    fn which_lists_nothing_for_an_unknown_name() {
        let dir = sample("which");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert!(vfs.which("animals/tiger.ai").unwrap().is_empty());
        let copies = vfs.which("ANIMALS\\LION.AI").unwrap();
        assert_eq!(copies.len(), 2, "Updates/new.ztd shadows root-b.ztd");
        assert_eq!(copies[0].path, Path::new("UPDATES/new.ztd"));
        assert_eq!(copies[1].path, Path::new("root-b.ztd"));
        assert!(copies.iter().all(|copy| copy.in_archive));
    }

    #[test]
    fn read_copies_returns_every_copy_in_search_order() {
        let dir = sample("copies");
        let vfs = Vfs::open(&dir.0).unwrap();
        let copies = vfs.read_copies("ANIMALS\\LION.AI").unwrap();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0], vfs.read("animals/lion.ai").unwrap());
        assert!(vfs.read_copies("animals/tiger.ai").unwrap().is_empty());
        assert!(vfs.read_copies("").unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_not_followed() {
        let outside = TempDir::new("link-outside");
        outside.write("secret.txt", b"secret");
        outside.archive("evil.ztd", &[("secret.txt", "secret")]);
        let dir = TempDir::new("links");
        dir.ini("./a;./link");
        dir.write("a/real.txt", b"real");
        std::os::unix::fs::symlink(&outside.0, dir.0.join("a/folder")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("secret.txt"), dir.0.join("a/file.txt")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("evil.ztd"), dir.0.join("a/evil.ztd")).unwrap();
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "real.txt"), "real");
        assert!(vfs.read("folder/secret.txt").is_err());
        assert!(vfs.read("file.txt").is_err());
        assert!(vfs.read("secret.txt").is_err());
        assert_eq!(vfs.warnings().len(), 3, "{:?}", vfs.warnings());

        // A search directory that is a link out of the install folder is an error.
        std::os::unix::fs::symlink(&outside.0, dir.0.join("link")).unwrap();
        assert!(message(Vfs::open(&dir.0)).contains("leaves the install folder"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn files_that_differ_only_in_letter_case_use_the_first_in_byte_order() {
        let dir = TempDir::new("case");
        dir.ini(".");
        dir.write("Same.txt", b"upper");
        dir.write("same.txt", b"lower");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(text(&vfs, "same.txt"), "upper", "'S' sorts before 's'");
        assert_eq!(vfs.warnings().len(), 1);
    }

    #[test]
    fn list_is_sorted_and_normalised_and_filters_with_wildcards() {
        let dir = sample("list");
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(
            vfs.list(Some("animals/*")),
            ["animals/lion.ai", "animals/zebra.ai"]
        );
        let all = vfs.list(None);
        assert!(
            all.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted, no duplicates"
        );
        assert!(all.contains(&"sounds/roar.wav".to_string()));
        assert!(
            all.contains(&"readme.txt".to_string()),
            "loose files are listed"
        );
        assert_eq!(vfs.member_count(), all.len());
        assert_eq!(
            vfs.list(Some("ANIMALS\\*")),
            ["animals/lion.ai", "animals/zebra.ai"]
        );
        assert_eq!(vfs.list(Some("*.wav")), ["sounds/roar.wav"]);
        assert_eq!(vfs.list(Some("animals/lion.ai")), ["animals/lion.ai"]);
        assert!(vfs.list(Some("animals")).is_empty());
    }

    #[test]
    fn counts() {
        let dir = TempDir::new("counts");
        dir.ini("./a;.");
        dir.archive("a/old.ztd", &[("x", "1"), ("y", "1")]);
        dir.archive("a/p.ztd", &[("x", "2")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), 2);
        // Loose: old.ztd and p.ztd from a; zoo.ini, a/old.ztd and a/p.ztd from the root.
        assert_eq!(vfs.loose_count(), 5);
        // The names x and y come on top; x is in both archives.
        assert_eq!(vfs.member_count(), 5 + 2);
        assert_eq!(vfs.shadowed_count(), 1);
    }

    #[test]
    fn wildcard_cases() {
        for (pattern, text, expected) in [
            ("", "", true),
            ("*", "", true),
            ("*", "a/b", true),
            ("a*c", "abbc", true),
            ("a*c", "abbd", false),
            ("a*b*c", "aXbYbZc", true),
            ("*b", "abab", true),
            ("*b", "aba", false),
            ("a", "ab", false),
            ("ab", "a", false),
            ("**x", "x", true),
        ] {
            assert_eq!(
                wildcard_matches(pattern.as_bytes(), text.as_bytes()),
                expected,
                "{pattern:?} against {text:?}"
            );
        }
    }

    #[test]
    fn demo_ini_is_used_when_there_is_no_zoo_ini() {
        let dir = TempDir::new("demo-ini");
        dir.write("ZooDemo.ini", b"[Resource]\nPATH = .\n");
        dir.archive("a.ztd", &[("x.txt", "demo")]);
        assert_eq!(Vfs::open(&dir.0).unwrap().read("x.txt").unwrap(), b"demo");

        dir.write("zoo.ini", b"[resource]\npath=./other\n");
        dir.archive("other/b.ztd", &[("y.txt", "full")]);
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.list(Some("y.txt")), ["y.txt"], "zoo.ini comes first");
        assert!(vfs.list(Some("x.txt")).is_empty());
    }

    #[test]
    fn resource_path_reads_only_the_resource_section() {
        let ini = "\u{feff}[user]\r\npath=wrong\r\n; path=comment\r\n[resource]\r\n; for the demo\r\n  Path = ./a;.  \r\n[other]\r\npath=wrong\r\n";
        assert_eq!(resource_path(ini).as_deref(), Some("./a;."));
        assert_eq!(
            resource_path("[RESOURCE]\nPATH=\"./q;.\"\n").as_deref(),
            Some("./q;.")
        );
        assert_eq!(
            resource_path("[resource]\npath='.'\n").as_deref(),
            Some(".")
        );
        assert_eq!(resource_path("[resource]\nkey=1\n"), None);
        assert_eq!(resource_path("path=.\n"), None);
        assert_eq!(resource_path(""), None);
    }

    #[test]
    fn open_errors() {
        let missing = std::env::temp_dir().join("rustyzoo-does-not-exist");
        assert!(message(Vfs::open(&missing)).contains("cannot read folder"));

        let dir = TempDir::new("no-ini");
        assert!(message(Vfs::open(&dir.0)).contains("no zoo.ini or zoodemo.ini"));

        // Without a path the install folder itself is the search path, as in the game.
        dir.write("zoo.ini", b"[user]\nfullscreen=1\n");
        dir.archive("p.ztd", &[("x.txt", "root")]);
        assert_eq!(text(&Vfs::open(&dir.0).unwrap(), "x.txt"), "root");
        dir.write("zoo.ini", b"[resource]\nkey=1\n");
        assert_eq!(text(&Vfs::open(&dir.0).unwrap(), "x.txt"), "root");

        dir.write("zoo.ini", b"[resource]\npath=;\n");
        assert!(message(Vfs::open(&dir.0)).contains("lists no directory"));

        dir.write("zoo.ini", b"[resource]\npath=./a;../b\n");
        assert!(message(Vfs::open(&dir.0)).contains("leaves the install folder"));

        // Directories that do not exist are skipped, and a path of only those is an error.
        dir.write("zoo.ini", b"[resource]\npath=./a\n");
        assert!(message(Vfs::open(&dir.0)).contains("lists no directory"));
    }

    #[test]
    fn missing_member_and_absurd_sizes_are_errors() {
        let dir = sample("read-errors");
        let vfs = Vfs::open(&dir.0).unwrap();
        let text = vfs.read("animals/tiger.ai").unwrap_err().to_string();
        assert_eq!(text, "no such member: animals/tiger.ai");
        assert!(vfs.read("animals").is_err(), "a directory is not a member");

        assert!(check_size(MAX_MEMBER_SIZE).is_ok());
        assert!(check_size(MAX_MEMBER_SIZE + 1).is_err());
        assert!(check_size(u64::MAX).is_err());
    }

    #[test]
    fn io_errors_are_not_damaged_content() {
        use io::ErrorKind::*;
        for kind in [NotFound, UnexpectedEof, InvalidData, InvalidInput] {
            assert!(
                matches!(io_failure(kind.into()), Failure::Damaged(_)),
                "{kind}"
            );
        }
        for kind in [PermissionDenied, Other, TimedOut, Interrupted, OutOfMemory] {
            assert!(
                matches!(io_failure(kind.into()), Failure::System(_)),
                "{kind}"
            );
        }
        assert!(matches!(complete(Ok(())), Ok(true)));
        assert!(matches!(complete(Err(UnexpectedEof.into())), Ok(false)));
        assert!(complete(Err(PermissionDenied.into())).is_err());
    }

    /// Removes all permissions of `path` and says whether that stops this process from opening
    /// it (it does not if the tests run as root).
    #[cfg(unix)]
    fn lock(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        File::open(path).is_err()
    }

    #[cfg(unix)]
    fn unlock(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_the_system_will_not_open_is_an_error_not_a_fall_through() {
        let dir = TempDir::new("denied");
        dir.ini(".");
        dir.archive("a.ztd", &[("x.txt", "older copy")]);
        dir.write("x.txt", b"loose");
        let vfs = Vfs::open(&dir.0).unwrap();
        let loose = dir.0.join("x.txt");
        if lock(&loose) {
            let why = vfs.read("x.txt").unwrap_err().to_string();
            assert!(
                why.contains("x.txt") && why.contains("Permission denied"),
                "{why}"
            );
        }
        unlock(&loose);
        assert_eq!(text(&vfs, "x.txt"), "loose");

        // The same for an archive at open time, and for a sub-folder.
        let archive = dir.0.join("a.ztd");
        if lock(&archive) {
            let why = message(Vfs::open(&dir.0));
            assert!(
                why.contains("a.ztd") && why.contains("Permission denied"),
                "{why}"
            );
        }
        unlock(&archive);
        dir.write("sub/f.txt", b"f");
        let sub = dir.0.join("sub");
        if lock(&sub) {
            let why = message(Vfs::open(&dir.0));
            assert!(
                why.contains("sub") && why.contains("Permission denied"),
                "{why}"
            );
        }
        unlock(&sub);
        assert!(Vfs::open(&dir.0).is_ok());
    }

    #[test]
    fn the_ini_file_must_be_a_small_regular_file() {
        let dir = TempDir::new("ini-limits");
        dir.write("big.ini", &vec![b' '; MAX_INI_SIZE as usize + 1]);
        assert!(read_ini(&dir.0.join("big.ini")).is_err());
        dir.write("ok.ini", &vec![b' '; MAX_INI_SIZE as usize]);
        assert!(read_ini(&dir.0.join("ok.ini")).is_ok());
        assert!(read_ini(&dir.0).is_err(), "a folder is not a file");
    }

    /// The archives (relative path and size) that hold a name, the one that supplies it first.
    fn copies(vfs: &Vfs, name: &str) -> Vec<(String, u64)> {
        vfs.which(name)
            .unwrap()
            .into_iter()
            .map(|copy| {
                assert!(copy.in_archive, "{name} is loose");
                (copy.path.to_string_lossy().replace('\\', "/"), copy.size)
            })
            .collect()
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(Path::new(&dir)).unwrap();
        assert!(vfs.warnings().is_empty(), "{:#?}", vfs.warnings());
        assert_eq!(
            (
                vfs.archive_count(),
                vfs.loose_count(),
                vfs.member_count(),
                vfs.shadowed_count()
            ),
            if crate::pe::installed_language(&dir) == 1036 {
                (172, 475, 328_950, 2_468)
            } else {
                (174, 484, 328_964, 3_567)
            }
        );
        // For each name: the archives in search order with the sizes of their copies. The
        // first supplies the name; for the last two names it is not the archive whose file
        // name sorts first.
        let examples: [(&str, &[(&str, u64)]); 6] = [
            (
                "animals/lion.ai",
                &[
                    ("Updates/animalsa.ztd", 26_986),
                    ("ZUPDATE1/animals7.ztd", 26_799),
                    ("ZUPDATE/animals4.ztd", 26_582),
                    ("animals2.ztd", 26_436),
                ],
            ),
            (
                "animals/asianele.ai",
                &[
                    ("Updates/asian_elephant02.ztd", 22_736),
                    ("Updates/asian_elephant01.ztd", 22_637),
                    ("Updates/asian_elephant.ztd", 17_353),
                ],
            ),
            (
                "animals/magnet.ai",
                &[
                    ("Updates/magnet02.ztd", 23_165),
                    ("Updates/magnet01.ztd", 22_835),
                    ("Updates/magnet.ztd", 15_851),
                ],
            ),
            (
                "animals/marlin.ai",
                &[
                    ("Updates/swordfsh01.ztd", 15_298),
                    ("Updates/swordfsh.ztd", 15_199),
                ],
            ),
            (
                "animals/mntnlion.ai",
                &[
                    ("Updates/mt_lion.ztd", 19_941),
                    ("Updates/mountainlion01.ztd", 20_128),
                    ("Updates/mountainlion.ztd", 19_941),
                ],
            ),
            (
                "animals/asianele/icasiane/icasiane.pal",
                &[
                    ("Updates/asian_elephant01.ztd", 1_024),
                    ("Updates/asian_elephant.ztd", 1_024),
                ],
            ),
        ];
        for (name, expected) in examples {
            // One of the archives of this name is in the English install only.
            if name == "animals/mntnlion.ai" && crate::pe::installed_language(&dir) == 1036 {
                continue;
            }
            let found = copies(&vfs, name);
            let expected: Vec<_> = expected.iter().map(|&(n, s)| (n.to_string(), s)).collect();
            assert_eq!(found, expected, "{name}");
            assert_eq!(
                vfs.read(name).unwrap().len() as u64,
                expected[0].1,
                "{name}"
            );
        }
        assert_eq!(vfs.read("ANIMALS\\LION.AI").unwrap().len(), 26_986);
        assert!(
            vfs.list(Some("animals/lion*"))
                .contains(&"animals/lion.ai".to_string())
        );
    }

    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_maps_are_loose_files() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(Path::new(&dir)).unwrap();
        let copies = vfs.which("maps/default.zoo").unwrap();
        assert_eq!(copies.len(), 1);
        assert!(!copies[0].in_archive);
        let data = vfs.read("MAPS\\Default.zoo").unwrap();
        assert_eq!(data.len() as u64, copies[0].size);
        assert!(vfs.list(Some("maps/*.zoo")).len() > 1);
        // The members that are empty in the archives are invisible.
        assert!(vfs.read("empty.txt").is_err());
    }
}
