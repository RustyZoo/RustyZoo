//! A read-only virtual file system over the resource search path of an installation.
//!
//! The game reads `[resource] path` from `zoo.ini` (or `zoodemo.ini` in the base trial): a
//! `;`-separated list of directories, relative to the install folder or not. The directories make
//! one ordered list of sources, and a name is served by the first source that has it:
//!
//! - the directories in the order of the path, so a directory listed earlier wins;
//! - inside a directory, first its loose files (every file below it, at any depth, by its path
//!   relative to the directory), then its archives (`*.ztd` and `*.zip`, ZIP files) in descending
//!   byte order of their lower-cased file names, so `magnet02.ztd` wins over `magnet01.ztd`,
//!   which wins over `magnet.ztd`.
//!
//! Mods are only files that this search finds: archives or loose files in a folder of the path,
//! typically `dlupdate` or `updates`, listed before the folders they override. An archive in a
//! sub-folder of a listed folder is not opened (its name is a loose name), and the list is built
//! once, so a file added while the game runs is seen after a restart. There is no limit on the
//! number of archives: the original opens at most 509 and silently drops the rest.
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
    pub(crate) fn is_system(&self) -> bool {
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
    /// An archive. The first [`KEPT_OPEN`] archives stay open: reading a member seeks to it, and
    /// the lock serialises reads. The others are opened for each read, so that the number of
    /// archives is not bounded by the limit of open files of the system.
    Archive {
        path: PathBuf,
        file: Option<Mutex<File>>,
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
                let within = |f: Failure| f.within(&path.display().to_string(), name);
                let read = match file {
                    Some(file) => {
                        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        read_member(&mut file, member)
                    }
                    None => read_member(
                        &mut File::open(path).map_err(|e| within(io_failure(e)))?,
                        member,
                    ),
                };
                read.map(Some).map_err(within)
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
    /// Fails if the folder or its ini file is missing or unusable, if a directory of the search
    /// path cannot be listed, or if the path names no directory that exists. Directories that do
    /// not exist are skipped, and so are elements that cannot be resolved (with a warning).
    /// Archives that are not acceptable ZIP files are ignored and reported by
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

        let mut vfs = Vfs {
            root: install_dir.to_path_buf(),
            ini,
            sources: Vec::new(),
            warnings: Vec::new(),
        };
        let (search_path, cut) = cut_path(search_path);
        if cut {
            vfs.warnings.push(format!(
                "[resource] path is longer than {MAX_PATH_BYTES} bytes, the end is cut off"
            ));
        }
        let mut directories: Vec<PathBuf> = Vec::new();
        for entry in search_path.split(';') {
            // The original keeps an element as written, but Windows drops the spaces at the end
            // of a path and cannot open one that starts with a space.
            let entry = entry.trim_end();
            if entry.is_empty() {
                continue;
            }
            if entry.starts_with(char::is_whitespace) {
                vfs.warnings.push(format!(
                    "[resource] path entry \"{entry}\" starts with a space, skipped"
                ));
                continue;
            }
            match resolve_directory(install_dir, entry) {
                Ok(Some(directory)) if !directories.contains(&directory) => {
                    directories.push(directory);
                }
                Ok(_) => {}
                Err(e) => vfs
                    .warnings
                    .push(format!("[resource] path entry \"{entry}\": {e}")),
            }
        }
        if directories.is_empty() {
            return error(format!(
                "[resource] path in {} lists no directory",
                ini_path.display()
            ));
        }

        let mut kept = 0;
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
                    Ok(mut source) => {
                        if let Source::Archive { file, .. } = &mut source
                            && kept >= KEPT_OPEN
                        {
                            *file = None;
                        }
                        kept += 1;
                        vfs.sources.push(source);
                    }
                    Err(Failure::Damaged(why)) => vfs
                        .warnings
                        .push(format!("{} is ignored: {why}", path.display())),
                    Err(Failure::Stop(why) | Failure::System(why)) => {
                        return error(format!("cannot use {}: {why}", path.display()));
                    }
                }
            }
        }
        let archives = vfs
            .sources
            .iter()
            .filter(|s| matches!(s, Source::Archive { .. }))
            .count();
        if archives > OPEN_FILE_LIMIT {
            vfs.warnings.push(format!(
                "{archives} archives: the original game opens at most {OPEN_FILE_LIMIT} and silently drops the rest"
            ));
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
    pub(crate) fn read_copies(&self, name: &str) -> Result<Vec<Vec<u8>>, VfsError> {
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
    pub(crate) fn listed(&self, prefix: &str, suffix: &str) -> Vec<String> {
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
    pub(crate) fn overlay_ini(&mut self, over: &Config) -> Result<(), TooLarge> {
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
        let streamed = if flags & 8 != 0 {
            " (written in streaming mode: the sizes follow the data, and the original cannot read it)"
        } else {
            ""
        };
        return Err(Failure::Stop(format!(
            "its local header declares size 0{streamed}"
        )));
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
        file: Some(Mutex::new(reader.into_inner())),
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
pub(crate) fn name_from_bytes(bytes: &[u8]) -> String {
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
pub(crate) fn normalise(path: &str) -> String {
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
            let resource = section
                .strip_suffix(']')
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("resource"));
            // Only the first section of that name is read.
            if in_resource {
                return None;
            }
            in_resource = resource;
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

/// The longest value the game reads for the search path: its buffer is 1,024 bytes.
const MAX_PATH_BYTES: usize = 1023;

/// The search path cut to [`MAX_PATH_BYTES`] bytes of the ini file, and whether it was cut. A
/// byte that is not UTF-8 is one byte of the file (see `name_from_bytes`), though several of ours.
fn cut_path(path: String) -> (String, bool) {
    let mut bytes = 0;
    for (at, c) in path.char_indices() {
        let width = if ('\u{F700}'..='\u{F7FF}').contains(&c) {
            1
        } else {
            c.len_utf8()
        };
        if bytes + width > MAX_PATH_BYTES {
            return (path[..at].to_string(), true);
        }
        bytes += width;
    }
    (path, false)
}

/// How many archives stay open. The system limits the open files of a process (256 on macOS by
/// default, 1,024 on Linux), and an install may have any number of archives.
const KEPT_OPEN: usize = 128;

/// What the C library inside the original allows to be open together: 512 streams, three of them
/// the standard ones.
const OPEN_FILE_LIMIT: usize = 509;

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

/// Resolves one entry of the search path: against the install folder when it is relative, as it
/// stands when it is absolute. `None` if the directory does not exist. Like the original, an
/// entry may leave the install folder (`..`, an absolute path).
fn resolve_directory(install_dir: &Path, entry: &str) -> Result<Option<PathBuf>, VfsError> {
    use std::path::Component;
    let mut directory = install_dir.to_path_buf();
    for component in Path::new(&entry.replace('\\', "/")).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => directory.push(".."),
            Component::RootDir => directory.push(component.as_os_str()),
            Component::Prefix(_) => directory = PathBuf::from(component.as_os_str()),
            Component::Normal(name) => {
                let found = match name.to_str() {
                    Some(name) => find_ignoring_case(&directory, name, true)?,
                    None => None,
                };
                match found {
                    Some(next) => directory = next,
                    None => return Ok(None),
                }
            }
        }
    }
    Ok(directory.is_dir().then_some(directory))
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
mod counts;

#[cfg(test)]
mod tests;
