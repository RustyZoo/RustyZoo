//! A bounded reader for the resources of 32-bit PE files (Windows `.dll` and `.exe`).
//!
//! The game keeps its texts in resource DLLs: the string table (type `RT_STRING`) and a custom
//! resource type named `TEXT`. A PE file stores its resources as a tree three levels deep: the
//! type, the name or number of the resource, and its language. Nothing else of the PE format is
//! read. Every offset, size and count comes from an untrusted file and is checked against the
//! file; the work is bounded by the file size, so a malformed file gives an error.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// Why a file could not be read as a PE file with resources. The message is a single line.
#[derive(Debug)]
pub struct PeError(String);

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for PeError {}

fn error<T>(message: &str) -> Result<T, PeError> {
    Err(PeError(message.to_string()))
}

/// The type, the name or the language of a resource: a number, or a text for named ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResKey {
    Id(u32),
    Name(String),
}

impl ResKey {
    fn is_name(&self, wanted: &str) -> bool {
        matches!(self, ResKey::Name(name) if name.eq_ignore_ascii_case(wanted))
    }
}

/// One leaf of the resource tree: its data lies inside the file that was read.
#[derive(Debug, Clone)]
pub struct Resource<'a> {
    pub kind: ResKey,
    pub name: ResKey,
    /// The language, as a Windows language identifier (0x409 is US English, 0 is neutral).
    pub language: u32,
    pub data: &'a [u8],
}

/// The resource type number of string tables (`RT_STRING`).
pub const RT_STRING: u32 = 6;
/// The resource type numbers of bitmaps (`RT_BITMAP`) and version information (`RT_VERSION`).
pub const RT_BITMAP: u32 = 2;
pub const RT_VERSION: u32 = 16;
const MAX_NAME_CHARS: usize = 64;
/// The Windows loader refuses more; the game's libraries have five or six.
const MAX_SECTIONS: usize = 96;
const MAX_STRING_BLOCK: u32 = 4096;

/// The section of the file that holds the data at one range of virtual addresses.
struct Section {
    rva: u32,
    raw_size: u32,
    raw_offset: u32,
}

fn bytes(file: &[u8], offset: usize, len: usize) -> Result<&[u8], PeError> {
    offset
        .checked_add(len)
        .and_then(|end| file.get(offset..end))
        .ok_or_else(|| PeError("file is truncated or has an offset outside the file".to_string()))
}

fn u16_at(file: &[u8], offset: usize) -> Result<u16, PeError> {
    Ok(u16::from_le_bytes(
        bytes(file, offset, 2)?.try_into().unwrap(),
    ))
}

fn u32_at(file: &[u8], offset: usize) -> Result<u32, PeError> {
    Ok(u32::from_le_bytes(
        bytes(file, offset, 4)?.try_into().unwrap(),
    ))
}

fn utf16(units: &[u8]) -> String {
    let (pairs, _) = units.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).collect();
    String::from_utf16_lossy(&units)
}

/// The offset in the file of the data at the virtual address `rva`, if `len` bytes of it are
/// inside one section.
fn offset_of(sections: &[Section], rva: u32, len: u32) -> Result<usize, PeError> {
    let section = sections
        .iter()
        .find(|s| rva >= s.rva && rva - s.rva < s.raw_size);
    match section.filter(|s| len <= s.raw_size - (rva - s.rva)) {
        Some(s) => Ok(s.raw_offset as usize + (rva - s.rva) as usize),
        None => error("resource data lies outside the sections of the file"),
    }
}

/// Reads all resources of a 32-bit PE file.
///
/// Fails if the file is not a PE32 file, has no resources, or if its resource tree is
/// malformed: a wrong depth, an offset or size outside the file, more entries than fit, or
/// resources that share their data.
pub fn resources(file: &[u8]) -> Result<Vec<Resource<'_>>, PeError> {
    if bytes(file, 0, 2)? != b"MZ" {
        return error("not a PE file: no MZ header");
    }
    let pe = u32_at(file, 0x3C)? as usize;
    if bytes(file, pe, 4)? != b"PE\0\0" {
        return error("not a PE file: no PE signature");
    }
    let section_count = u16_at(file, pe + 6)? as usize;
    if section_count > MAX_SECTIONS {
        return error("the PE file has too many sections");
    }
    let optional_size = u16_at(file, pe + 20)? as usize;
    let optional = pe + 24;
    if u16_at(file, optional)? != 0x10B {
        return error("not a 32-bit PE file");
    }
    // The data directory entry 2 holds the address and the size of the resource tree.
    if optional_size < 120 || u32_at(file, optional + 92)? < 3 {
        return error("the PE file has no resource directory");
    }
    let tree_rva = u32_at(file, optional + 112)?;
    if tree_rva == 0 {
        return error("the PE file has no resources");
    }
    let table = optional + optional_size;
    let mut sections = Vec::new();
    for i in 0..section_count {
        let header = table + i * 40;
        sections.push(Section {
            rva: u32_at(file, header + 12)?,
            raw_size: u32_at(file, header + 16)?,
            raw_offset: u32_at(file, header + 20)?,
        });
    }

    // All offsets inside the tree are relative to its start.
    let base = offset_of(&sections, tree_rva, 16)?;
    bytes(file, base, 16)?;
    // Every entry takes eight bytes of the file, so a tree that is read entry by entry, and
    // that shares nothing, has at most this many. A tree that points several entries at the
    // same directory runs out of budget instead of growing.
    let mut budget = (file.len() - base) / 8;

    // Real files never share the data of two resources, so the data of all resources adds up
    // to less than the file. This bounds the work and memory of everything built on top.
    let mut data_total = 0usize;
    let mut found = Vec::new();
    for (kind, kind_dir) in entries(file, base, 0, &mut budget)? {
        for (name, name_dir) in entries(file, base, subdirectory(kind_dir)?, &mut budget)? {
            for (language, leaf) in entries(file, base, subdirectory(name_dir)?, &mut budget)? {
                let ResKey::Id(language) = language else {
                    return error("a resource language is a name");
                };
                if leaf & DIRECTORY != 0 {
                    return error("the resource tree is deeper than three levels");
                }
                let entry = base + leaf as usize;
                let (rva, size) = (u32_at(file, entry)?, u32_at(file, entry + 4)?);
                let start = offset_of(&sections, rva, size)?;
                let data = bytes(file, start, size as usize)?;
                data_total += data.len();
                if data_total > file.len() {
                    return error("the data of the resources adds up to more than the file");
                }
                found.push(Resource {
                    kind: kind.clone(),
                    name: name.clone(),
                    language,
                    data,
                });
            }
        }
    }
    Ok(found)
}

const DIRECTORY: u32 = 0x8000_0000;

/// The offset of the subdirectory an entry points to.
fn subdirectory(offset: u32) -> Result<u32, PeError> {
    if offset & DIRECTORY == 0 {
        return error("the resource tree is less than three levels deep");
    }
    Ok(offset & !DIRECTORY)
}

/// The entries of the directory at `dir` (relative to `base`): each one's key and its raw offset
/// field, whose top bit tells a subdirectory from a data entry.
fn entries(
    file: &[u8],
    base: usize,
    dir: u32,
    budget: &mut usize,
) -> Result<Vec<(ResKey, u32)>, PeError> {
    let dir = base + dir as usize;
    let count = u16_at(file, dir + 12)? as usize + u16_at(file, dir + 14)? as usize;
    if count > *budget {
        return error("the resource tree has more entries than fit in the file");
    }
    *budget -= count;
    let mut list = Vec::with_capacity(count);
    for i in 0..count {
        let entry = dir + 16 + i * 8;
        let key = u32_at(file, entry)?;
        let key = if key & DIRECTORY == 0 {
            ResKey::Id(key)
        } else {
            let name = base + (key & !DIRECTORY) as usize;
            let chars = u16_at(file, name)? as usize;
            if chars > MAX_NAME_CHARS {
                return error("a resource name is too long");
            }
            ResKey::Name(utf16(bytes(file, name + 2, chars * 2)?))
        };
        list.push((key, u32_at(file, entry + 4)?));
    }
    Ok(list)
}

/// The string table: string ID to text. A block of the table (resource type 6) holds 16
/// strings, the first one with the ID `(block - 1) * 16`; empty strings are not in the map.
/// If several languages define the same ID, the one found first wins. The work is bounded by
/// the size of the file, because `resources` bounds the data of all blocks together.
pub fn strings(resources: &[Resource]) -> Result<BTreeMap<u32, String>, PeError> {
    let mut table = BTreeMap::new();
    for block in resources.iter().filter(|r| r.kind == ResKey::Id(RT_STRING)) {
        let ResKey::Id(number @ 1..=MAX_STRING_BLOCK) = block.name else {
            return error("a string block has an invalid number");
        };
        let mut rest = block.data;
        for i in 0..16 {
            let chars = u16_at(rest, 0)? as usize;
            let text = bytes(rest, 2, chars * 2)?;
            rest = &rest[2 + chars * 2..];
            if chars > 0 {
                table
                    .entry((number - 1) * 16 + i)
                    .or_insert_with(|| utf16(text));
            }
        }
    }
    Ok(table)
}

/// The resources of the custom type `TEXT`, by name, in the order of the file. Entries that
/// have a number instead of a name are left out.
pub fn texts<'r, 'a>(resources: &'r [Resource<'a>]) -> Vec<(&'r str, &'a [u8])> {
    resources
        .iter()
        .filter(|r| r.kind.is_name("TEXT"))
        .filter_map(|r| match &r.name {
            ResKey::Name(name) => Some((name.as_str(), r.data)),
            ResKey::Id(_) => None,
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // A minimal image: headers, one section at file offset SECTION and address RVA, and a
    // resource tree with the type TEXT (one entry LION, "hello") and the type 6 (block 1).
    const SECTION: usize = 0x200;
    const RVA: u32 = 0x1000;
    // Offsets of fields inside the section, for the tests that damage the image.
    const ROOT_COUNT_ID: usize = 14;
    const STRING_TYPE_OFFSET: usize = 24 + 4;
    const BLOCK_NUMBER: usize = 56 + 16;
    const BLOCK_OFFSET: usize = BLOCK_NUMBER + 4;
    const BLOCK_LANGUAGE: usize = 104 + 16;
    const BLOCK_LEAF: usize = BLOCK_LANGUAGE + 4;
    const LION_NAME_LEN: usize = 170;
    const TEXT_LEAF: usize = 128;
    const STRING_LEAF: usize = 144;
    const DIR: u32 = 0x8000_0000;

    fn put16(v: &mut [u8], at: usize, value: u16) {
        v[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put32(v: &mut [u8], at: usize, value: u32) {
        v[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn name(v: &mut Vec<u8>, text: &str) {
        v.extend((text.len() as u16).to_le_bytes());
        v.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    }

    fn directory(v: &mut Vec<u8>, named: u16, ids: u16, entries: &[(u32, u32)]) {
        v.extend([0; 12]);
        v.extend(named.to_le_bytes());
        v.extend(ids.to_le_bytes());
        for (key, offset) in entries {
            v.extend(key.to_le_bytes());
            v.extend(offset.to_le_bytes());
        }
    }

    fn sample() -> Vec<u8> {
        let mut tree = Vec::new();
        directory(&mut tree, 1, 1, &[(DIR | 160, DIR | 32), (6, DIR | 56)]); // 0
        directory(&mut tree, 1, 0, &[(DIR | 170, DIR | 80)]); // 32: TEXT
        directory(&mut tree, 0, 1, &[(1, DIR | 104)]); // 56: type 6
        directory(&mut tree, 0, 1, &[(1033, 128)]); // 80: LION
        directory(&mut tree, 0, 1, &[(1033, 144)]); // 104: block 1
        // 128: data entries: address, size, code page, reserved
        for (offset, size) in [(184, 5), (192, 50)] {
            for field in [RVA + offset, size, 0, 0] {
                tree.extend(field.to_le_bytes());
            }
        }
        name(&mut tree, "TEXT"); // 160
        name(&mut tree, "LION"); // 170
        tree.extend([0; 4]); // 180
        tree.extend(b"hello\0\0\0"); // 184
        // 192: 16 strings: "Hi", empty, "Zoo", a lone surrogate, 11 empty ones and "End".
        for text in [&[0x48, 0x69][..], &[], &[0x5A, 0x6F, 0x6F], &[0xD800]] {
            tree.extend((text.len() as u16).to_le_bytes());
            tree.extend(text.iter().flat_map(|u| u16::to_le_bytes(*u)));
        }
        tree.extend([0; 22]); // 11 empty strings
        name(&mut tree, "End"); // index 15, the last one of the block
        image(tree)
    }

    /// A PE image whose resources are the given strings (ID and text) and `TEXT` entries
    /// (name and bytes), in one language each.
    pub(crate) fn library(strings: &[(u32, &str)], texts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut blocks: BTreeMap<u32, [&str; 16]> = BTreeMap::new();
        for (id, text) in strings {
            blocks.entry(id / 16 + 1).or_default()[(id % 16) as usize] = text;
        }
        let mut types = Vec::new();
        if !texts.is_empty() {
            let leaves = texts
                .iter()
                .map(|(n, d)| (ResKey::Name(n.to_string()), d.to_vec()));
            types.push((ResKey::Name("TEXT".to_string()), leaves.collect::<Vec<_>>()));
        }
        if !blocks.is_empty() {
            let leaves = blocks.iter().map(|(number, block)| {
                let mut data = Vec::new();
                for text in block {
                    name(&mut data, text);
                }
                (ResKey::Id(*number), data)
            });
            types.push((ResKey::Id(RT_STRING), leaves.collect::<Vec<_>>()));
        }

        // The layout: the root, the directories of the types, one directory of languages per
        // resource, the data entries, the names, the data.
        let leaf_count: usize = types.iter().map(|t| t.1.len()).sum();
        let type_dirs: usize = types.iter().map(|t| 16 + 8 * t.1.len()).sum();
        let languages = 16 + 8 * types.len() + type_dirs;
        let entries = languages + 24 * leaf_count;
        let names_at = entries + 16 * leaf_count;
        let mut names = Vec::new();
        let mut key = |k: &ResKey| match k {
            ResKey::Id(number) => *number,
            ResKey::Name(text) => {
                let at = names_at + names.len();
                name(&mut names, text);
                DIR | at as u32
            }
        };
        let type_keys: Vec<u32> = types.iter().map(|t| key(&t.0)).collect();
        let leaf_keys: Vec<Vec<u32>> = types
            .iter()
            .map(|t| t.1.iter().map(|leaf| key(&leaf.0)).collect())
            .collect();
        let mut data_at = names_at + names.len();

        // The counts of named and numbered entries are not told apart: the reader adds them.
        let mut tree = Vec::new();
        let mut next_dir = 16 + 8 * types.len();
        let roots: Vec<(u32, u32)> = type_keys
            .iter()
            .zip(&types)
            .map(|(k, t)| {
                let at = next_dir;
                next_dir += 16 + 8 * t.1.len();
                (*k, DIR | at as u32)
            })
            .collect();
        directory(&mut tree, 0, roots.len() as u16, &roots);
        let mut leaf = 0;
        for (keys, t) in leaf_keys.iter().zip(&types) {
            let list: Vec<(u32, u32)> = keys
                .iter()
                .enumerate()
                .map(|(i, k)| (*k, DIR | (languages + 24 * (leaf + i)) as u32))
                .collect();
            directory(&mut tree, 0, list.len() as u16, &list);
            leaf += t.1.len();
        }
        for i in 0..leaf_count {
            directory(&mut tree, 0, 1, &[(1033, (entries + 16 * i) as u32)]);
        }
        let mut blobs: Vec<u8> = Vec::new();
        for (_, data) in types.iter().flat_map(|t| &t.1) {
            for field in [RVA + data_at as u32, data.len() as u32, 0, 0] {
                tree.extend(field.to_le_bytes());
            }
            data_at += data.len();
            blobs.extend(data);
        }
        tree.extend(names);
        tree.extend(blobs);
        image(tree)
    }

    /// Headers and one section around a resource tree.
    fn image(tree: Vec<u8>) -> Vec<u8> {
        let mut file = vec![0; SECTION];
        file[..2].copy_from_slice(b"MZ");
        put32(&mut file, 0x3C, 0x40);
        file[0x40..0x44].copy_from_slice(b"PE\0\0");
        put16(&mut file, 0x44 + 2, 1); // section count at COFF + 2
        put16(&mut file, 0x44 + 16, 224); // size of the optional header
        let optional = 0x44 + 20;
        put16(&mut file, optional, 0x10B);
        put32(&mut file, optional + 92, 16);
        put32(&mut file, optional + 112, RVA);
        put32(&mut file, optional + 116, tree.len() as u32);
        let section = optional + 224;
        put32(&mut file, section + 8, tree.len() as u32);
        put32(&mut file, section + 12, RVA);
        put32(&mut file, section + 16, tree.len() as u32);
        put32(&mut file, section + 20, SECTION as u32);
        file.extend(tree);
        file
    }

    fn broken(change: impl FnOnce(&mut Vec<u8>)) -> String {
        let mut file = sample();
        change(&mut file);
        resources(&file).map(|_| ()).unwrap_err().to_string()
    }

    #[test]
    fn reads_strings_and_texts() {
        let file = sample();
        let found = resources(&file).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(texts(&found), [("LION", &b"hello"[..])]);
        assert_eq!(found[1].language, 1033);
        let table = strings(&found).unwrap();
        // Block 1 holds the IDs 0 to 15; empty strings are absent.
        let expected = [(0, "Hi"), (2, "Zoo"), (3, "\u{FFFD}"), (15, "End")];
        assert_eq!(table, expected.map(|(id, s)| (id, s.to_string())).into());
    }

    #[test]
    fn string_ids_follow_the_block_number() {
        let mut file = sample();
        put32(&mut file, SECTION + BLOCK_NUMBER, 0x1F4); // block 500: IDs 7984 and up
        let table = strings(&resources(&file).unwrap()).unwrap();
        assert_eq!(
            table.keys().copied().collect::<Vec<_>>(),
            [7984, 7986, 7987, 7999]
        );
        // Block 4096 is the last one: its IDs end at 65535.
        put32(&mut file, SECTION + BLOCK_NUMBER, 4096);
        let table = strings(&resources(&file).unwrap()).unwrap();
        assert_eq!(table.keys().last(), Some(&65535));
    }

    #[test]
    fn the_first_language_wins() {
        let mut tree = Vec::new();
        directory(&mut tree, 0, 1, &[(6, DIR | 24)]); // 0
        directory(&mut tree, 0, 1, &[(1, DIR | 48)]); // 24: type 6
        directory(&mut tree, 0, 2, &[(1033, 80), (1031, 96)]); // 48: block 1
        let (first, second) = (strings_block("first"), strings_block("second"));
        for (offset, block) in [(112, &first), (112 + first.len(), &second)] {
            for field in [RVA + offset as u32, block.len() as u32, 0, 0] {
                tree.extend(field.to_le_bytes());
            }
        }
        tree.extend(first);
        tree.extend(second);
        let table = strings(&resources(&image(tree)).unwrap()).unwrap();
        assert_eq!(table, [(0, "first".to_string())].into());
    }

    /// A block of 16 strings: the given one, then 15 empty ones.
    fn strings_block(first: &str) -> Vec<u8> {
        let mut block = Vec::new();
        name(&mut block, first);
        block.extend([0; 30]);
        block
    }

    #[test]
    fn blocks_that_share_their_data_are_an_error() {
        // 4096 blocks that all use one data entry of 32 bytes: without the bound, a data entry
        // of 2 MB would decode to 65,536 strings of 65,535 characters.
        let (languages, data_entry) = (24 + 16 + 4096 * 8, 24 + 16 + 4096 * 8 + 24);
        let mut tree = Vec::new();
        directory(&mut tree, 0, 1, &[(6, DIR | 24)]);
        let blocks: Vec<(u32, u32)> = (1..=4096).map(|n| (n, DIR | languages as u32)).collect();
        directory(&mut tree, 0, 4096, &blocks);
        directory(&mut tree, 0, 1, &[(1033, data_entry as u32)]);
        for field in [RVA + data_entry as u32 + 16, 32, 0, 0] {
            tree.extend(field.to_le_bytes());
        }
        // Room enough for the entries: the bound on the entries alone must not decide.
        tree.resize(tree.len() + 40_000, 0);
        let message = resources(&image(tree)).map(|_| ()).unwrap_err().to_string();
        assert!(message.contains("adds up"), "{message}");
    }

    #[test]
    fn every_truncation_is_an_error() {
        let file = sample();
        for len in 0..file.len() {
            assert!(resources(&file[..len]).is_err(), "length {len}");
        }
    }

    #[test]
    fn damaged_bytes_never_panic() {
        let file = sample();
        for at in 0..file.len() {
            for value in [0, 0x7F, 0xFF] {
                let mut damaged = file.clone();
                damaged[at] = value;
                if let Ok(found) = resources(&damaged) {
                    let _ = strings(&found);
                    let _ = texts(&found);
                }
            }
        }
    }

    #[test]
    fn not_a_pe32_file() {
        assert!(broken(|f| f[0] = b'Z').contains("MZ"));
        assert!(broken(|f| f[0x40] = b'Q').contains("PE signature"));
        assert!(broken(|f| put16(f, 0x44 + 20, 0x20B)).contains("32-bit"));
        assert!(broken(|f| put32(f, 0x3C, 0xFFFF_FFF0)).contains("truncated"));
        assert!(broken(|f| put32(f, 0x44 + 20 + 92, 2)).contains("no resource directory"));
        assert!(broken(|f| put32(f, 0x44 + 20 + 112, 0)).contains("no resources"));
        assert!(broken(|f| put16(f, 0x44 + 2, 0xFFFF)).contains("too many sections"));
        assert!(broken(|f| put16(f, 0x44 + 2, 96)).contains("truncated"));
    }

    #[test]
    fn a_tree_that_is_not_three_levels_deep() {
        // The type 6 entry points to a data entry instead of a directory.
        assert!(
            broken(|f| put32(f, SECTION + STRING_TYPE_OFFSET, 128)).contains("less than three")
        );
        // The data entry of a block is a directory: the entry points back at the root.
        assert!(broken(|f| put32(f, SECTION + BLOCK_LEAF, DIR)).contains("deeper"));
        // A cycle: the directory of the block points back at the root.
        assert!(broken(|f| put32(f, SECTION + BLOCK_OFFSET, DIR)).contains("name"));
        // A language is a name.
        assert!(broken(|f| put32(f, SECTION + BLOCK_LANGUAGE, DIR | 160)).contains("name"));
    }

    #[test]
    fn counts_that_do_not_fit() {
        assert!(broken(|f| put16(f, SECTION + ROOT_COUNT_ID, 0xFFFF)).contains("more entries"));
        // Ten entries that all point at the same directory, which has ten entries that all
        // point at the same directory: the entries are counted every time they are visited.
        let mut tree = Vec::new();
        directory(&mut tree, 0, 10, &[(1, DIR | 96); 10]);
        directory(&mut tree, 0, 10, &[(1, DIR | 192); 10]);
        directory(&mut tree, 0, 10, &[(1033, 288); 10]);
        for field in [RVA + 288, 0, 0, 0] {
            tree.extend(u32::to_le_bytes(field));
        }
        let message = resources(&image(tree)).map(|_| ()).unwrap_err().to_string();
        assert!(message.contains("more entries"), "{message}");
    }

    #[test]
    fn data_outside_the_file() {
        let size_at = SECTION + TEXT_LEAF + 4;
        assert!(broken(|f| put32(f, size_at, 0xFFFF_FFFF)).contains("outside"));
        assert!(broken(|f| put32(f, size_at - 4, RVA + 0x10_0000)).contains("outside"));
        assert!(broken(|f| put32(f, size_at - 4, 0)).contains("outside"));
    }

    #[test]
    fn a_section_that_ends_before_its_data() {
        // The raw size of the section is shorter than the string block at its end.
        let raw_size = 0x44 + 20 + 224 + 16;
        assert!(broken(|f| put32(f, raw_size, 200)).contains("outside"));
    }

    #[test]
    fn a_resource_name_that_is_too_long() {
        assert!(broken(|f| put16(f, SECTION + LION_NAME_LEN, 65)).contains("too long"));
    }

    #[test]
    fn bad_string_blocks() {
        let mut file = sample();
        put32(&mut file, SECTION + BLOCK_NUMBER, 0);
        assert!(strings(&resources(&file).unwrap()).is_err());
        let mut file = sample();
        put32(&mut file, SECTION + BLOCK_NUMBER, MAX_STRING_BLOCK + 1);
        assert!(strings(&resources(&file).unwrap()).is_err());
        // The block ends in the middle of a string.
        let mut file = sample();
        put32(&mut file, SECTION + STRING_LEAF + 4, 7);
        let err = strings(&resources(&file).unwrap()).unwrap_err();
        assert!(err.to_string().contains("truncated"));
        // A string longer than the rest of the block.
        let mut file = sample();
        put16(&mut file, SECTION + 192, 0xFFFF);
        assert!(strings(&resources(&file).unwrap()).is_err());
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let file = std::fs::read(std::path::Path::new(&dir).join("lang0.dll")).unwrap();
        let found = resources(&file).unwrap();
        let table = strings(&found).unwrap();
        assert_eq!(table.len(), 3161);
        for (id, text) in [
            (5006, "Lion"),
            (9400, "Savannah"),
            (8016, "Aviary"),
            (3270, "Arial Bold"),
        ] {
            assert_eq!(table[&id], text);
        }
        assert_eq!(texts(&found).len(), 220);
        let lion = texts(&found)
            .into_iter()
            .find(|(n, _)| *n == "LION_GENERAL");
        assert_eq!(lion.unwrap().1.len(), 1595);
    }
}
