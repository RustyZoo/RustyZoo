//! Community content: mod folders, search path rules and archives that tools write differently.

use super::*;

const TEST_AI: &str = "animals/test.ai";

/// An install whose ini is `path`, with a base archive that holds `C` for `animals/test.ai`.
fn install(name: &str, path: &str) -> TempDir {
    let dir = TempDir::new(name);
    dir.ini(path);
    dir.archive("base.ztd", &[(TEST_AI, "C")]);
    dir
}

#[test]
fn a_dlupdate_archive_beats_updates_and_the_base_game() {
    let dir = install("mod-dlupdate", "./dlupdate;./updates;.");
    dir.archive("updates/zzz.ztd", &[(TEST_AI, "B")]);
    dir.archive("dlupdate/MyMod.ZTD", &[(TEST_AI, "A")]);
    let vfs = Vfs::open(&dir.0).unwrap();
    assert_eq!(text(&vfs, TEST_AI), "A");
    assert!(vfs.warnings().is_empty(), "{:?}", vfs.warnings());
    std::fs::remove_file(dir.0.join("dlupdate/MyMod.ZTD")).unwrap();
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "B");
}

#[test]
fn in_a_mod_folder_the_greatest_lower_cased_name_wins() {
    let dir = install("mod-order", "./updates;.");
    dir.archive("updates/aaa.ztd", &[(TEST_AI, "1")]);
    dir.archive("updates/aaa01.ztd", &[(TEST_AI, "2")]);
    dir.archive("updates/aaa.zip", &[(TEST_AI, "3")]);
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "2");
    std::fs::remove_file(dir.0.join("updates/aaa01.ztd")).unwrap();
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "1");
}

#[test]
fn a_loose_file_of_a_mod_folder_beats_its_archives() {
    let dir = install("mod-loose", "./updates;.");
    dir.archive("updates/zzz.ztd", &[(TEST_AI, "B")]);
    dir.write("updates/Animals/Test.ai", b"L");
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "L");
}

#[test]
fn an_archive_in_a_sub_folder_is_found_only_when_the_sub_folder_is_listed() {
    let dir = install("mod-sub", "./updates;.");
    dir.archive("updates/sub/inner.ztd", &[("animals/sub.ai", "S")]);
    let vfs = Vfs::open(&dir.0).unwrap();
    assert!(vfs.read("animals/sub.ai").is_err());
    assert_eq!(vfs.archive_count(), 1, "only base.ztd");
    dir.ini("./updates;./updates/sub;.");
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), "animals/sub.ai"), "S");
}

#[test]
fn a_folder_named_like_an_archive_and_a_side_file_are_no_sources() {
    let dir = install("mod-junk", "./updates;.");
    std::fs::create_dir_all(dir.0.join("updates/dir.ztd")).unwrap();
    dir.write("updates/._junk.ztd", &[0; 4096]);
    let vfs = Vfs::open(&dir.0).unwrap();
    assert_eq!(vfs.archive_count(), 1);
    assert_eq!(text(&vfs, TEST_AI), "C");
}

#[test]
fn an_archive_with_a_comment_is_dropped_and_the_next_serves() {
    let dir = install("mod-comment", "./updates;.");
    dir.write(
        "updates/comment.ztd",
        &zip_bytes(&[(TEST_AI, "X")], "12345"),
    );
    let vfs = Vfs::open(&dir.0).unwrap();
    assert_eq!(text(&vfs, TEST_AI), "C");
    assert!(vfs.warnings()[0].contains("comment.ztd"));
}

#[test]
fn a_streamed_member_fails_the_request_and_names_the_archive_and_the_member() {
    let dir = install("mod-stream", "./updates;.");
    let mut bytes = zip_bytes(&[(TEST_AI, "X")], "");
    let local = find_all(&bytes, &LOCAL_SIGNATURE)[0];
    bytes[local + 6] |= 8; // sizes follow the data
    bytes[local + 14..local + 26].fill(0);
    dir.write("updates/stream.ztd", &bytes);
    let vfs = Vfs::open(&dir.0).unwrap();
    let why = vfs.read(TEST_AI).unwrap_err().to_string();
    assert!(
        why.contains("stream.ztd") && why.contains(TEST_AI) && why.contains("streaming"),
        "{why}"
    );
}

#[test]
fn a_missing_folder_is_skipped() {
    let dir = install("mod-missing", "./missing;.");
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "C");
}

#[test]
fn spaces_at_the_end_of_an_element_go_and_a_leading_space_skips_the_element() {
    let dir = install("mod-spaces", "./updates  ;\t; ./other;.");
    dir.archive("updates/u.ztd", &[(TEST_AI, "U")]);
    dir.archive("other/o.ztd", &[("animals/o.ai", "O")]);
    let vfs = Vfs::open(&dir.0).unwrap();
    assert_eq!(text(&vfs, TEST_AI), "U");
    assert!(vfs.read("animals/o.ai").is_err());
    assert_eq!(vfs.warnings().len(), 1, "{:?}", vfs.warnings());
    assert!(vfs.warnings()[0].contains("./other"));
}

#[test]
fn an_element_may_leave_the_install_folder() {
    let outer = TempDir::new("mod-outside");
    let install = outer.0.join("game");
    let dir = TempDir(install.clone());
    std::fs::create_dir_all(&install).unwrap();
    dir.archive("base.ztd", &[(TEST_AI, "C")]);
    outer.archive("mods/m.ztd", &[(TEST_AI, "M")]);
    outer.archive("abs/a.ztd", &[("animals/abs.ai", "Z")]);
    dir.ini(&format!("../Mods;{};.", outer.0.join("abs").display()));
    let vfs = Vfs::open(&install).unwrap();
    assert_eq!(text(&vfs, TEST_AI), "M");
    assert_eq!(text(&vfs, "animals/abs.ai"), "Z");
    assert!(vfs.warnings().is_empty(), "{:?}", vfs.warnings());
    std::mem::forget(dir); // `outer` removes the whole tree
}

#[test]
fn an_element_that_names_nothing_is_not_an_error() {
    let dir = install("mod-nothing", "../nowhere-at-all;C:/mods;/nowhere/at/all;.");
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "C");
}

#[test]
fn a_path_over_1023_bytes_is_cut_with_a_warning() {
    let dir = install("mod-long", "./updates");
    dir.archive("updates/u.ztd", &[(TEST_AI, "U")]);
    // The base folder is the last element, past the cut.
    let padding = ";".repeat(1023 - "./updates".len() - ";.".len());
    dir.ini(&format!("./updates{padding};."));
    let vfs = Vfs::open(&dir.0).unwrap();
    assert!(vfs.warnings().is_empty(), "exactly 1023 bytes are kept");
    assert_eq!(vfs.directory_count(), 2);
    dir.ini(&format!("./updates{padding};;."));
    let vfs = Vfs::open(&dir.0).unwrap();
    assert_eq!(vfs.directory_count(), 1, "the end is cut off");
    assert!(vfs.warnings()[0].contains("1023"));
    // Bytes that are not UTF-8 count one each, as in the file.
    let raw = name_from_bytes(&[0xFF; 1100]);
    assert_eq!(cut_path(raw).0.chars().count(), 1023);
    assert_eq!(cut_path(String::new()), (String::new(), false));
    assert_eq!(cut_path("\u{e9}".repeat(600)).0.chars().count(), 511);
}

#[test]
fn an_empty_path_value_lists_no_folder_and_a_missing_key_means_the_install_folder() {
    let dir = install("mod-empty-path", "");
    assert!(message(Vfs::open(&dir.0)).contains("lists no directory"));
    dir.write("zoo.ini", b"[resource]\n");
    assert_eq!(text(&Vfs::open(&dir.0).unwrap(), TEST_AI), "C");
}

#[test]
fn only_the_first_resource_section_is_read() {
    assert_eq!(
        resource_path("[resource]\nkey=1\n[Resource]\npath=./x\n"),
        None
    );
    assert_eq!(
        resource_path("[a]\n[resource]\npath=./x\n[resource]\npath=./y\n").as_deref(),
        Some("./x")
    );
}

#[test]
fn archives_are_not_limited_in_number() {
    for (count, warned) in [(509, false), (510, true)] {
        let dir = TempDir::new("mod-many");
        dir.ini("./updates");
        for i in 0..count {
            dir.archive(
                &format!("updates/a{i:04}.ztd"),
                &[(&format!("n{i}.txt"), "x")],
            );
        }
        let vfs = Vfs::open(&dir.0).unwrap();
        assert_eq!(vfs.archive_count(), count);
        assert_eq!(text(&vfs, "n0.txt"), "x", "the last archive of the list");
        assert_eq!(vfs.warnings().len(), usize::from(warned), "{count}");
    }
}
