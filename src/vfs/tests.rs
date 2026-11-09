use std::io::Write;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::*;
use crate::fixtures::TempDir;

mod mods;

impl TempDir {
    /// Writes an archive whose members are stored and deflated alternately.
    fn archive(&self, relative: &str, members: &[(&str, &str)]) {
        self.write(relative, &zip_bytes(members, ""));
    }

    /// A search path of `directories` and nothing else in the ini file.
    fn ini(&self, path: &str) {
        self.write("zoo.ini", format!("[resource]\npath={path}\n").as_bytes());
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

    // A search directory may be a link out of the install folder, like any other element.
    std::os::unix::fs::symlink(&outside.0, dir.0.join("link")).unwrap();
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), "secret.txt"), "secret");
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
    // The archives, updates and damaged files (a Spanish disc) differ between the installs.
    if !crate::fixtures::measured_on("full_game_facts", &dir) {
        return;
    }
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
