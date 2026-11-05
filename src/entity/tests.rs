use std::io::Write;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::fixtures::Install;
use crate::usertext;
use crate::vfs::Vfs;

use super::catalogue::{MAX_WARNINGS, number_user_names, show};
use super::*;

/// The bytes of an archive with these members.
fn archive(members: &[(&str, &str)], method: CompressionMethod) -> Vec<u8> {
    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (member, content) in members {
        zip.start_file(
            *member,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// An installation of one archive with these text members.
fn install(name: &str, members: &[(&str, &str)]) -> Install {
    let members: Vec<_> = members
        .iter()
        .map(|(member, text)| (*member, text.as_bytes().to_vec()))
        .collect();
    Install::new(name, &members)
}

impl Install {
    fn load(&self) -> Result<Catalogue, EntityError> {
        Catalogue::load(&Vfs::open(&self.0).unwrap())
    }
}

/// The catalogue of an installation with these members.
fn catalogue(name: &str, members: &[(&str, &str)]) -> Catalogue {
    install(name, members).load().unwrap()
}

#[test]
fn the_guests_non_path_cost_is_the_unit_class_files_or_none() {
    let cost = |ai: Option<&str>| {
        let members: Vec<(&str, &str)> = ai.map(|t| ("ztunit.ai", t)).into_iter().collect();
        catalogue("ztunit", &members).guest_non_path_cost
    };
    assert_eq!(
        cost(Some("[Characteristics/Integers]\ncNonPathCost=5\n")),
        Some(5)
    );
    assert_eq!(
        cost(Some("[Characteristics/Integers]\ncNonPathCost=-3\n")),
        Some(0)
    );
    assert_eq!(
        cost(Some(
            "[Characteristics/Integers]\ncNonPathCost=2147483647\n"
        )),
        Some(i32::MAX as u32)
    );
    assert_eq!(cost(Some("[Characteristics/Integers]\n")), None);
    assert_eq!(cost(None), None);
}

#[test]
fn the_item_types_come_from_the_registries_and_a_missing_file_is_a_warning() {
    let c = catalogue(
        "items",
        &[
            (
                "items.cfg",
                "[items]\nHotDog = items/hotdog.cfg\ngone = items/gone.cfg\nnone=\n",
            ),
            (
                "items/hotdog.cfg",
                "[characteristics]\nnameID=6303\nicon=items\\hotdog\\hotdog\nhungerdelta=-75\nadulthappydelta=50\n\
                 consumenow=1\ntrashwhenconsumed=2\n",
            ),
        ],
    );
    let item = &c.items["hotdog"];
    assert_eq!((item.id, item.record[0], item.record[2]), (6303, 50, -75));
    // A flag is on only for 1.
    assert!(item.now && !item.litter && !item.remove);
    assert_eq!(item.icon, "items/hotdog/hotdog");
    assert_eq!(c.items.len(), 1);
    assert!(
        c.warnings.iter().any(|w| w.contains("gone.cfg")),
        "{:?}",
        c.warnings
    );
}

/// `class/type/subtype` of every type, in creation order.
fn names(catalogue: &Catalogue) -> Vec<String> {
    catalogue
        .types
        .iter()
        .map(|t| format!("{}/{}/{}", t.class, show(&t.type_name), show(&t.subtype)))
        .collect()
}

fn get<'a>(c: &'a Catalogue, class: &str, kind: &str, subtype: &str) -> &'a EntityType {
    c.find(class, kind.as_bytes(), subtype.as_bytes())
        .unwrap_or_else(|| panic!("no type {class}/{kind}/{subtype}"))
}

#[test]
fn registries_are_found_by_prefix_and_read_in_byte_order() {
    let c = catalogue(
        "order",
        &[
            ("animals.cfg", "[animals]\nzebra = z.ai\n"),
            ("Animal02.cfg", "[animals]\nyak = y.ai\n"),
            ("animal01.cfg", "[animals]\nxerus = x.ai\n"),
            ("animalx.cfg", "no section here\n"),
            ("sub/animal03.cfg", "[animals]\nnever = n.ai\n"),
            ("x.ai", "[Global]\n"),
        ],
    );
    assert_eq!(
        names(&c),
        ["animals/xerus/", "animals/yak/", "animals/zebra/"]
    );
    // The test is on the whole name: a folder name matches the prefix too.
    let c = catalogue("prefix", &[("food/x.cfg", "[food]\napple = a.ai\n")]);
    assert_eq!(names(&c), ["food/food/apple"]);
}

#[test]
fn classes_are_processed_in_the_fixed_order() {
    let c = catalogue(
        "classes",
        &[
            ("twall.cfg", "[tankwall]\nw\n"),
            ("animals.cfg", "[animals]\na\n"),
            ("fences.cfg", "[fences]\nf\n"),
            ("food.cfg", "[food]\nb\n"),
        ],
    );
    assert_eq!(
        names(&c),
        ["food/food/b", "fences/f/f", "animals/a/", "tankwall/w/f"]
    );
    assert_eq!(
        c.counts().iter().map(|c| c.1).collect::<Vec<_>>(),
        [1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0]
    );
}

#[test]
fn entries_follow_the_order_of_the_keys_not_of_the_file() {
    let c = catalogue(
        "keys",
        &[("paths.cfg", "[paths]\nb = b.ai\nA = a.ai\na = a.ai\n")],
    );
    assert_eq!(
        names(&c),
        ["paths/paths/A", "paths/paths/a", "paths/paths/b"]
    );
}

#[test]
fn flat_registries() {
    let c = catalogue(
        "flat",
        &[
            ("food.cfg", "[food]\nbone = b.ai\n"),
            ("bldg.cfg", "[building]\nshop = s.ai\n"),
            ("ambient.cfg", "[ambient]\nbird = b.ai\n"),
            ("guests.cfg", "[guests]\nwoman = w.ai\nman = m.ai\n"),
        ],
    );
    assert_eq!(
        names(&c),
        [
            "food/food/bone",
            "building/building/shop",
            "guests/man/",
            "guests/woman/",
            "ambient/ambient/bird"
        ]
    );
}

#[test]
fn two_level_registries() {
    let c = catalogue(
        "two-level",
        &[
            (
                "scenery.cfg",
                "[objects]\nother\nfoliage = ignored\nnothing\n[foliage]\noak = o.ai\nacacia = a.ai\n[other]\nrock = r.ai\n",
            ),
            ("rubble.cfg", "[rubble]\nr1\n[r1]\nbig = b.ai\n"),
            ("filter.cfg", "[filter]\nfilter1\n[filter1]\nf = f.ai\n"),
        ],
    );
    assert_eq!(
        names(&c),
        [
            "objects/foliage/acacia",
            "objects/foliage/oak",
            "objects/other/rock",
            "rubble/r1/big",
            "filter/filter1/f"
        ]
    );
}

#[test]
fn subtypes_come_from_the_shared_or_the_own_section() {
    let c = catalogue(
        "subtypes",
        &[
            // A shared section with a key wins over the own ones.
            (
                "fences.cfg",
                "[fences]\nwall = w.ai\nrail = r.ai\n[subtypes]\ng\nf\n[wall/subtypes]\nz\n",
            ),
            // An empty shared section counts as absent.
            (
                "animals.cfg",
                "[animals]\nlion = l.ai\nyak = y.ai\n[subtypes]\n[lion/subtypes]\nm\nf\n",
            ),
            // Staff only reads the own section, and the key decides the class.
            (
                "staff.cfg",
                "[staff]\nscient = s.ai\ntour = t.ai\nmaint\nguide = g.ai\nhelicopter = h.ai\n[subtypes]\nx\n[scient/subtypes]\nm\nf\n",
            ),
            ("twall.cfg", "[tankwall]\nglass = g.ai\n"),
        ],
    );
    assert_eq!(
        names(&c),
        [
            "fences/rail/f",
            "fences/rail/g",
            "fences/wall/f",
            "fences/wall/g",
            "animals/lion/f",
            "animals/lion/m",
            "animals/yak/",
            "helicopter/helicopter/",
            "maint/maint/",
            "keeper/scient/f",
            "keeper/scient/m",
            "tour/tour/",
            "tankwall/glass/f",
        ]
    );
    assert!(c.warnings.iter().any(|w| w.contains("unknown kind: guide")));
    // Staff is one group of the counts.
    assert_eq!(c.counts()[7], ("staff", 5));
}

#[test]
fn a_type_listed_again_keeps_the_first_definition() {
    let c = catalogue(
        "again",
        &[
            ("animal01.cfg", "[animals]\nyak = Y.ai\n"),
            ("animal02.cfg", "[animals]\nyak = y.ai\n"),
            ("animal03.cfg", "[animals]\nyak = z.ai\n"),
            ("y.ai", "[Global]\n[Animations]\nidle = idle\n"),
            ("z.ai", "[Global]\n"),
        ],
    );
    assert_eq!(names(&c), ["animals/yak/"]);
    assert_eq!(get(&c, "animals", "yak", "").file.as_deref(), Some("y.ai"));
    // Another letter case of the same file is the same file; z.ai is another.
    assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
    assert!(c.warnings[0].contains("another file"));
}

#[test]
fn a_missing_or_empty_definition_file_leaves_defaults() {
    let c = catalogue(
        "missing",
        &[
            (
                "fences.cfg",
                "[fences]\nnovalue\nempty =\ngone = gone.ai\nblank = blank.ai\n",
            ),
            ("blank.ai", "nothing but text\n"),
            ("paths.cfg", "[paths]\nbare\n"),
        ],
    );
    assert_eq!(c.types.len(), 5);
    for t in &c.types {
        assert!(t.config.is_none(), "{:?}", t.type_name);
        assert_eq!((t.name_id, t.footprint, t.height), (0, [1, 1, 1], 0));
        assert!(t.animations().is_empty() && t.base_animation().is_none());
    }
    // The fences without a file are not an error and keep the default subtype `f`.
    assert_eq!(get(&c, "fences", "novalue", "f").file, None);
    assert_eq!(c.warnings.len(), 4, "{:?}", c.warnings);
    assert_eq!(
        get(&c, "fences", "gone", "f").file.as_deref(),
        Some("gone.ai")
    );
}

fn typed(class: &'static str, type_name: &[u8], subtype: &[u8], name_id: i32) -> EntityType {
    EntityType {
        class,
        type_name: type_name.to_vec(),
        subtype: subtype.to_vec(),
        file: None,
        config: None,
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

#[test]
fn user_names_get_their_own_ids_from_19000_and_a_name_keeps_its_id() {
    let mut types = vec![
        typed("animals", b"lion", b"m", 19000),
        typed("animals", b"lion", b"f", 19000),
        typed("animals", b"bear", b"f", 19000),
        typed("building", b"building", b"shop", 19000),
        typed("objects", b"other", b"rock", 19000),
        typed("objects", b"other", b"rock", 19000),
        typed("objects", b"other", b"tree", 0),
        typed("objects", b"other", b"sign", -5),
        typed("objects", b"other", b"bush", i32::MAX),
        typed("objects", b"other", b"stone", 49000),
    ];
    assert_eq!(number_user_names(&mut types, usertext::NAMES), 0);
    let ids: Vec<i32> = types.iter().map(|t| t.name_id).collect();
    assert_eq!(
        ids,
        [
            19000,
            19000,
            19001,
            19002,
            19003,
            19003,
            0,
            -5,
            i32::MAX,
            19004
        ]
    );
}

#[test]
fn a_user_name_past_the_last_id_gets_none_and_is_counted() {
    let mut types = vec![
        typed("objects", b"other", b"a", 19000),
        typed("objects", b"other", b"b", 19000),
        typed("objects", b"other", b"a", 19000),
    ];
    assert_eq!(number_user_names(&mut types, 19000..=19000), 1);
    let ids: Vec<i32> = types.iter().map(|t| t.name_id).collect();
    assert_eq!(ids, [19000, 0, 19000]);
}

#[test]
fn user_files_come_after_the_registries_of_their_class() {
    let user = "[Global]\n[Characteristics/Integers]\ncNameID = 19000\n";
    let members = [
        ("animals.cfg", "[animals]\nyak = y.ai\n"),
        ("scenery.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
        ("bldg.cfg", "[building]\nshop = s.ai\n"),
        ("scenery/other/F554B047.ucs", user),
        ("scenery/other/rock.ucs", user),
        ("x/Pony.uca", user),
        ("home.ucb", user),
        ("a/.ucb", user),
        ("y.ai", user),
    ];
    let c = catalogue("user", &members);
    assert_eq!(
        names(&c),
        [
            "building/building/shop",
            "building/building/home",
            "animals/yak/",
            "animals/pony/m",
            "animals/pony/f",
            "animals/pony/y",
            "objects/other/rock",
            "objects/other/f554b047",
        ]
    );
    // The type that a registry names keeps its file.
    assert_eq!(
        get(&c, "objects", "other", "rock").file.as_deref(),
        Some("r.ai")
    );
    assert_eq!(
        get(&c, "building", "building", "home").file.as_deref(),
        Some("home.ucb")
    );
    assert!(c.warnings.iter().any(|w| w.contains("another file")));
    // Each user name gets its own id from 19000, in creation order (the classes in their
    // order); the three subtypes of a unit share one. The yak's file names a user id too, so
    // it is numbered; the rock has no user id.
    assert_eq!(get(&c, "building", "building", "home").name_id, 19000);
    assert_eq!(get(&c, "animals", "yak", "").name_id, 19001);
    for subtype in ["m", "f", "y"] {
        assert_eq!(get(&c, "animals", "pony", subtype).name_id, 19002);
    }
    assert_eq!(get(&c, "objects", "other", "f554b047").name_id, 19003);
    assert_eq!(get(&c, "objects", "other", "rock").name_id, 0);

    // Without a loaded registry of the class, its user files are not looked at.
    let c = catalogue(
        "user-alone",
        &[("a.uca", user), ("b.ucb", user), ("c.ucs", user)],
    );
    assert!(c.types.is_empty());
    let c = catalogue(
        "user-other",
        &[("bldg.cfg", "[building]\nshop = s.ai\n"), ("a.uca", user)],
    );
    assert_eq!(names(&c), ["building/building/shop"]);
}

#[test]
fn a_registry_or_definition_file_that_cannot_be_read_is_skipped_with_a_warning() {
    let huge = format!("[food]\n{}", "; padding\n".repeat(200_000));
    let c = catalogue(
        "huge-registry",
        &[
            ("food.cfg", &huge),
            ("food2.cfg", "[food]\nbone = b.ai\n"),
            ("b.ai", "[Animations]\nidle = idle\n"),
        ],
    );
    assert_eq!(names(&c), ["food/food/bone"]);
    assert!(
        c.warnings.len() == 1
            && c.warnings[0].contains("food.cfg")
            && c.warnings[0].contains("limit"),
        "{:?}",
        c.warnings
    );
    let c = catalogue(
        "huge-definition",
        &[
            ("food.cfg", "[food]\nbone = b.ai\nmeat = m.ai\n"),
            ("b.ai", &huge),
            ("m.ai", "[Animations]\nidle = idle\n"),
        ],
    );
    assert_eq!(names(&c), ["food/food/bone", "food/food/meat"]);
    assert!(get(&c, "food", "food", "bone").config.is_none());
    assert!(get(&c, "food", "food", "meat").config.is_some());
    assert!(c.warnings.len() == 1 && c.warnings[0].contains("b.ai"));
}

#[test]
fn a_damaged_member_leaves_the_other_types_alone() {
    let mut bytes = archive(
        &[
            ("b.ai", &"[Animations]\nidle = idle\n".repeat(50)),
            ("food.cfg", "[food]\nbone = b.ai\nmeat = m.ai\n"),
            ("m.ai", "[Animations]\nidle = idle\n"),
        ],
        CompressionMethod::Deflated,
    );
    // Overwrite the start of the data of the first member (it is at offset 0).
    let header = 30 + usize::from(bytes[26]) + usize::from(bytes[28]);
    bytes[header..header + 20].fill(0xff);
    let install = install("damaged", &[]);
    std::fs::write(install.0.join("a.ztd"), bytes).unwrap();
    let c = install.load().unwrap();
    assert_eq!(names(&c), ["food/food/bone", "food/food/meat"]);
    assert!(get(&c, "food", "food", "bone").config.is_none());
    assert!(get(&c, "food", "food", "meat").config.is_some());
    assert!(c.warnings.len() == 1 && c.warnings[0].contains("b.ai"));
}

#[test]
fn an_empty_loose_file_hides_the_member_and_the_type_keeps_the_defaults() {
    let install = install(
        "empty-loose",
        &[
            ("food.cfg", "[food]\nbone = b.ai\n"),
            ("b.ai", "[Animations]\nidle = idle\n"),
        ],
    )
    .loose("b.ai", b"");
    let c = install.load().unwrap();
    assert!(get(&c, "food", "food", "bone").config.is_none());
    assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
}

/// A failure of the system, unlike a file with the wrong content, stops the catalogue.
#[cfg(unix)]
#[test]
fn a_failure_of_the_system_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let install = install("denied", &[("food.cfg", "[food]\nbone = b.ai\n")])
        .loose("b.ai", b"[Animations]\n");
    let path = install.0.join("b.ai");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
    if std::fs::File::open(&path).is_ok() {
        return; // running as root
    }
    let message = install.load().unwrap_err().to_string();
    assert!(message.contains("b.ai"), "{message}");
}

#[test]
fn a_small_registry_cannot_make_an_unbounded_catalogue() {
    // 300 fences x 300 shared subtypes: all of them are built, and quickly.
    let keys =
        |prefix: &str, n: usize| (0..n).map(|i| format!("{prefix}{i}\n")).collect::<String>();
    let registry = format!("[fences]\n{}[subtypes]\n{}", keys("w", 300), keys("s", 300));
    let c = catalogue("many", &[("fences.cfg", &registry)]);
    assert_eq!(c.types.len(), 90_000);
    assert!(c.find("fences", b"w299", b"s299").is_some());
    // 1000 x 1000 is cut at the limit, with a warning.
    let registry = format!(
        "[fences]\n{}[subtypes]\n{}",
        keys("w", 1000),
        keys("s", 1000)
    );
    let c = catalogue("too-many", &[("fences.cfg", &registry)]);
    assert_eq!(c.types.len(), MAX_ENTRIES);
    assert!(c.warnings.iter().any(|w| w.contains("ignored")));
    // Warnings are bounded too: every entry here lacks a file.
    assert!(c.warnings.len() <= MAX_WARNINGS + 2, "{}", c.warnings.len());
    assert!(c.warnings.last().unwrap().contains("more warnings"));
}

#[test]
fn types_that_name_the_same_file_share_its_content() {
    let big = "[Characteristics/Integers]\n".to_string()
        + &(0..20_000)
            .map(|i| format!("c{i:x}=1\n"))
            .collect::<String>();
    let registry = "[food]\n".to_string()
        + &(0..200)
            .map(|i| format!("f{i} = big.ai\n"))
            .collect::<String>();
    let c = catalogue("shared", &[("food.cfg", &registry), ("big.ai", &big)]);
    assert_eq!(c.types.len(), 200);
    let first = c.types[0].config.as_ref().unwrap();
    assert!(
        c.types
            .iter()
            .all(|t| Arc::ptr_eq(t.config.as_ref().unwrap(), first))
    );
    // The three subtypes of an animal share the file too, and the file is read once.
    let c = catalogue(
        "shared-animal",
        &[
            (
                "animals.cfg",
                "[animals]\nyak = y.ai\n[yak/subtypes]\nm\nf\ny\n",
            ),
            ("y.ai", "[Global]\n"),
        ],
    );
    assert!(Arc::ptr_eq(
        get(&c, "animals", "yak", "m").config.as_ref().unwrap(),
        get(&c, "animals", "yak", "y").config.as_ref().unwrap()
    ));
}

#[test]
fn behaviour_sets_are_looked_up_by_class() {
    let c = catalogue(
        "behaviours",
        &[
            (
                "animals.cfg",
                "[animals]\nyak = y.ai\n[yak/subtypes]\nm\nf\n",
            ),
            ("guests.cfg", "[guests]\nman = g.ai\n"),
            (
                "staff.cfg",
                "[staff]\nkeeper = k.ai\n[keeper/subtypes]\nf\n",
            ),
            (
                "y.ai",
                "[Global]\nDefaultSubtype = f\n[m\\BehaviorSet\\bA]\nf = fHalt()\n\
                 [f\\BehaviorSet\\bA]\nf = fDie()\n[BehaviorSet\\bA]\nf = fHatch()\n\
                 [f\\BehaviorSet\\bB]\nf = fDie()\n[AmbientAnims]\na = -100\na = 100\nb = bA\n",
            ),
            (
                "g.ai",
                "[BehaviorSet\\bWalk]\nf = fWalk(0,0)\n[man\\BehaviorSet\\bWalk]\nf = fHalt()\n\
                 [man\\BehaviorSet\\bGawk]\nf = fHalt()\n[BehaviorSet\\bGawk]\nf = fDie()\n",
            ),
            (
                "k.ai",
                "[f\\BehaviorSet\\bX]\nf = fDie()\n[BehaviorSet\\bX]\nf = fHalt()\n",
            ),
        ],
    );
    let first = |class, kind, sub, name: &str| {
        let ty = get(&c, class, kind, sub);
        c.behaviour_set(ty, name.as_bytes())
            .map(|calls| calls[0].name)
    };
    // An animal: its subtype, then the default subtype, then the plain section.
    assert_eq!(first("animals", "yak", "m", "bA"), Some("fHalt"));
    assert_eq!(first("animals", "yak", "f", "bA"), Some("fDie"));
    assert_eq!(first("animals", "yak", "m", "bB"), Some("fDie"));
    assert_eq!(first("animals", "yak", "f", "bC"), None);
    // A guest: its type name, but bWalk is the plain one. Staff: the plain one only.
    assert_eq!(first("guests", "man", "", "bWalk"), Some("fWalk"));
    assert_eq!(first("guests", "man", "", "bGawk"), Some("fHalt"));
    assert_eq!(first("keeper", "keeper", "f", "bX"), Some("fHalt"));
    let yak = get(&c, "animals", "yak", "m");
    assert_eq!(c.ambient(yak, 0).len(), 1);
    assert!(c.ambient(yak, 1).is_empty());
    assert_eq!(c.behaviours.len(), 3);
    // The file is parsed once however many types name it.
    assert!(std::ptr::eq(
        c.behaviours(yak).unwrap(),
        c.behaviours(get(&c, "animals", "yak", "f")).unwrap()
    ));
}

#[test]
fn an_unknown_operation_fails_the_load_with_the_file_and_line() {
    let install = install(
        "bad-call",
        &[
            ("animals.cfg", "[animals]\nyak = y.ai\n"),
            (
                "y.ai",
                "[Global]\n\n[BehaviorSet\\bA]\nf = fHalt()\nf = fWait(3)\n",
            ),
        ],
    );
    let error = install.load().unwrap_err().to_string();
    assert_eq!(
        error,
        "y.ai line 5: section [BehaviorSet\\bA], call 2: unknown operation in fWait(3)"
    );
    // The names of the data that fail when run, and bad arguments, load with a warning.
    let good = catalogue(
        "bad-arguments",
        &[
            ("animals.cfg", "[animals]\nyak = y.ai\n"),
            (
                "y.ai",
                "[BehaviorSet\\bA]\nf = fFollow(keeper)\nf = fPlayTime(stand)\n",
            ),
        ],
    );
    let yak = get(&good, "animals", "yak", "");
    assert_eq!(good.behaviours(yak).unwrap().warnings.len(), 1);
}

#[test]
fn a_key_without_value_does_not_resolve() {
    let c = catalogue(
        "no-value",
        &[
            (
                "animals.cfg",
                "[animals]\napato = a.ai\n[apato/subtypes]\nm\ny\n",
            ),
            (
                "a.ai",
                "[AnimPath]\nm = animals/apato/m\ny = animals/apato/y\n\
                 [m/Animations]\nidle = idle\n[y/Animations]\nFrom stand\nidle = idle\n",
            ),
        ],
    );
    // The first key in byte order is the base animation, even without a member.
    let young = get(&c, "animals", "apato", "y");
    let base = young.base_animation().unwrap();
    assert_eq!((base.name.as_str(), base.member), ("From stand", None));
    assert_eq!(young.animations().len(), 2);
    assert_eq!(
        get(&c, "animals", "apato", "m")
            .base_animation()
            .unwrap()
            .member
            .as_deref(),
        Some("animals/apato/m/idle/idle.ani")
    );
}

#[test]
fn a_unit_with_a_default_subtype_never_reads_the_plain_name_id() {
    let c = catalogue(
        "no-fallback",
        &[
            (
                "animals.cfg",
                "[animals]\nleopard = l.ai\nlynx = x.ai\n[leopard/subtypes]\nm\n[lynx/subtypes]\nm\n",
            ),
            (
                "l.ai",
                "[Global]\nDefaultSubtype = f\n[Characteristics/Integers]\ncNameID = 7\n",
            ),
            ("x.ai", "[Characteristics/Integers]\ncNameID = 8\n"),
        ],
    );
    assert_eq!(get(&c, "animals", "leopard", "m").name_id, 0);
    assert_eq!(get(&c, "animals", "lynx", "m").name_id, 8);
}

#[test]
fn nested_search_directories_name_a_loose_user_file_twice() {
    let install = install(
        "nested",
        &[
            ("scener.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
            ("r.ai", "[Global]\n"),
        ],
    )
    .loose("updates/pony.ucs", b"[Global]\n")
    .loose("a/rock2.ucs", b"[Global]\n")
    .loose("b/rock2.ucs", b"[Global]\n");
    std::fs::write(install.0.join("zoo.ini"), "[resource]\npath=.;./updates\n").unwrap();
    let c = install.load().unwrap();
    // `updates/pony.ucs` and `pony.ucs` are one file: one type, no warning.
    assert!(c.find("objects", b"other", b"pony").is_some());
    // Two files that share a stem are two files.
    assert!(
        c.warnings.iter().any(|w| w.contains("rock2")),
        "{:?}",
        c.warnings
    );
    assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
}

#[test]
fn a_user_file_without_a_stem_is_skipped_with_a_warning() {
    let c = catalogue(
        "empty-stem",
        &[
            ("scener.cfg", "[objects]\nother\n[other]\nrock = r.ai\n"),
            ("scenery/.ucs", "[Global]\n"),
        ],
    );
    assert_eq!(names(&c), ["objects/other/rock"]);
    assert!(c.warnings.iter().any(|w| w.contains("empty name")));
}

/// One definition file per reading rule of the values.
#[test]
fn values_follow_the_passes_of_their_class() {
    let rates = "cSlowRate=1\ncMediumRate=2\ncFastRate=3\n";
    let lion = format!(
        "[Global]\nDefaultSubtype = f\n[f/Characteristics/Integers]\ncNameID = 5006\ncFootprintX = 1\ncFootprintY = 2\ncHeight = 0x10\ncEggFootprintX = -3\ncEggFootprintY = 99999999999\n{rates}\
         [m/Characteristics/Integers]\ncFootprintY = 3\ncNameID =\ncBoxFootprintX = 0\n[Characteristics/Integers]\ncNameID = 999\n"
    );
    // Name id and footprint are read at the ZT unit level, which never falls back to the
    // plain sections when there is a default subtype: the plain name id is not used.
    let tiger = format!(
        "[Global]\nDefaultSubtype = f\n[f/Characteristics/Integers]\ncNameID = 1\ncFootprintZ = 4\n[Characteristics/Integers]\ncNameID = 2\ncFootprintX = 6\n{rates}"
    );
    let guests = format!(
        "[Global]\nDefaultSubtype = man\n[man/Characteristics/Integers]\ncNameID = 3\n{rates}[woman/Characteristics/Integers]\ncNameID = 4\n"
    );
    let fence = "[Characteristics/Integers]\ncNameID = 1\ncHeight = 2\ncFootprintX = 3\n\
                 [g/Characteristics/Integers]\ncNameID = 9\ncFootprintX = 8\n";
    let scenery = "[Characteristics/Integers]\ncNameID = 7\ncFootprintX = 5\ncFootprintY = 6\n\
                   [tree/Characteristics/Integers]\ncNameID = 70\n[Global]\nDefaultSubtype = tree\n";
    let c = catalogue(
        "values",
        &[
            (
                "animals.cfg",
                "[animals]\nlion = lion.ai\ntiger = tiger.ai\n[lion/subtypes]\nf\nm\ny\n[tiger/subtypes]\nm\n",
            ),
            (
                "guests.cfg",
                "[guests]\nman = guests.ai\nwoman = guests.ai\n",
            ),
            (
                "fences.cfg",
                "[fences]\nrail = fence.ai\n[rail/subtypes]\nf\ng\n",
            ),
            (
                "scenery.cfg",
                "[objects]\nfoliage\n[foliage]\ntree = tree.ai\n",
            ),
            ("lion.ai", &lion),
            ("tiger.ai", &tiger),
            ("guests.ai", &guests),
            ("fence.ai", fence),
            ("tree.ai", scenery),
        ],
    );
    let values = |class, kind, subtype| {
        let t = get(&c, class, kind, subtype);
        (t.name_id, t.footprint, t.height)
    };
    assert_eq!(values("animals", "lion", "f"), (5006, [1, 2, 1], 16));
    // The own pass is on top of the default one, key by key; a key without value is skipped.
    assert_eq!(values("animals", "lion", "m"), (5006, [1, 3, 1], 16));
    assert_eq!(values("animals", "lion", "y"), (5006, [1, 2, 1], 16));
    assert_eq!(values("animals", "tiger", "m"), (1, [1, 1, 4], 0));
    assert_eq!(values("guests", "man", ""), (3, [1, 1, 1], 0));
    assert_eq!(values("guests", "woman", ""), (4, [1, 1, 1], 0));
    // Fences: the plain sections, then name id and height again from the subtype's.
    assert_eq!(values("fences", "rail", "f"), (1, [3, 1, 1], 2));
    assert_eq!(values("fences", "rail", "g"), (9, [3, 1, 1], 2));
    // Scenery never reads a prefixed section.
    assert_eq!(values("objects", "foliage", "tree"), (7, [5, 6, 1], 0));
    assert!(get(&c, "objects", "foliage", "tree").config.is_some());
    // The egg and crate footprints are 2 by 2 unless given: negative and huge values as they
    // come, the own pass on top of the default one.
    let forms = |subtype, egg| get(&c, "animals", "lion", subtype).form_footprint(egg);
    assert_eq!(forms("f", true), [-3, 99999999999i64 as i32]);
    assert_eq!(forms("f", false), [2, 2]);
    assert_eq!(forms("m", false), [0, 2]);
    assert_eq!(
        get(&c, "animals", "tiger", "m").form_footprint(true),
        [2, 2]
    );
}

#[test]
fn animation_sections_and_roots_by_class() {
    let ani = "[Animations]\nidle = idle\nused = used\nfull = X\\Y.ani\n";
    let c = catalogue(
        "animations",
        &[
            (
                "scenery.cfg",
                "[objects]\nfoliage\n[foliage]\ntree = ani.ai\n",
            ),
            ("bldg.cfg", "[building]\nshop = ani.ai\n"),
            ("paths.cfg", "[paths]\nroad = ani.ai\n"),
            ("food.cfg", "[food]\nbone = ani.ai\n"),
            ("rubble.cfg", "[rubble]\nr1\n[r1]\nbig = ani.ai\n"),
            ("filter.cfg", "[filter]\nf1\n[f1]\nclean = ani.ai\n"),
            ("ambient.cfg", "[ambient]\nbird = ani.ai\n"),
            ("helicopter.cfg", "not a staff registry\n"),
            (
                "fences.cfg",
                "[fences]\nrail = fence.ai\n[rail/subtypes]\nf\ng\n",
            ),
            (
                "animals.cfg",
                "[animals]\nlion = lion.ai\nyak = yak.ai\n[lion/subtypes]\nm\nf\n",
            ),
            ("guests.cfg", "[guests]\nman = guest.ai\n"),
            (
                "staff.cfg",
                "[staff]\nkeeper = ani.ai\nhelicopter = ani.ai\n[keeper/subtypes]\nm\n",
            ),
            ("ani.ai", ani),
            // The subtype's own section wins; an empty one does not count.
            (
                "fence.ai",
                "[f/Animations]\nidle = idle\n[g/Animations]\n[Animations]\nbroke = broke\n",
            ),
            (
                "lion.ai",
                "[Global]\nDefaultSubtype = m\n[AnimPath]\nm = animals/lion/m\nf = Animals\\Lion\\F\n\
                 [m/Animations]\nidle = stand\n[f/Animations]\n[Animations]\nidle = plain\n",
            ),
            ("yak.ai", "[Animations]\nidle = idle\nwalk = w/w.ani\n"),
            (
                "guest.ai",
                "[Global]\nDefaultSubtype = man\n[man/Animations]\nidle = idle\n[Animations]\nwalk = walk\n",
            ),
        ],
    );
    let member = |class, kind, subtype, name: &str| {
        get(&c, class, kind, subtype)
            .animations()
            .into_iter()
            .find(|a| a.name == name)
            .and_then(|a| a.member)
    };
    for (class, kind, subtype, expected) in [
        ("objects", "foliage", "tree", "objects/tree/idle/idle.ani"),
        ("building", "building", "shop", "objects/shop/idle/idle.ani"),
        ("paths", "paths", "road", "paths/road/idle/idle.ani"),
        ("food", "food", "bone", "food/bone/idle/idle.ani"),
        ("rubble", "r1", "big", "rubble/big/idle/idle.ani"),
        ("filter", "f1", "clean", "filter/clean/idle/idle.ani"),
        (
            "ambient",
            "ambient",
            "bird",
            "ambient/ambient/bird/idle/idle.ani",
        ),
        ("fences", "rail", "f", "fences/rail/f/idle/idle.ani"),
        ("animals", "lion", "m", "animals/lion/m/stand/stand.ani"),
        ("guests", "man", "", "guests/man/idle/idle.ani"),
        ("keeper", "keeper", "m", "staff/keeper/m/idle/idle.ani"),
        (
            "helicopter",
            "helicopter",
            "",
            "helicopter/helicopter/idle/idle.ani",
        ),
    ] {
        assert_eq!(
            member(class, kind, subtype, "idle").as_deref(),
            Some(expected)
        );
    }
    // A value with `.ani` is the member itself, in its normalised form.
    assert_eq!(
        member("objects", "foliage", "tree", "full").as_deref(),
        Some("x/y.ani")
    );
    assert_eq!(
        member("animals", "yak", "", "walk").as_deref(),
        Some("w/w.ani")
    );
    // No `[AnimPath]` entry for the subtype: the root is empty.
    assert_eq!(
        member("animals", "yak", "", "idle").as_deref(),
        Some("idle/idle.ani")
    );
    // The animals of the empty subtype section and of the plain one: g has no animations of
    // its own, so the plain section is used; f's empty section falls back to the default's.
    assert_eq!(
        member("fences", "rail", "g", "broke").as_deref(),
        Some("fences/rail/g/broke/broke.ani")
    );
    assert_eq!(
        member("animals", "lion", "f", "idle").as_deref(),
        Some("animals/lion/f/stand/stand.ani")
    );
    // Names come in ascending byte order.
    let order: Vec<_> = get(&c, "objects", "foliage", "tree")
        .animations()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(order, ["full", "idle", "used"]);
}

#[test]
fn lookup_accepts_parent_classes_and_base_animation_follows_the_class() {
    let ani = "[Animations]\nempty = empty\nfull = full\nidle = idle\n";
    let c = catalogue(
        "lookup",
        &[
            (
                "staff.cfg",
                "[staff]\nkeeper = a.ai\n[keeper/subtypes]\nm\n",
            ),
            ("bldg.cfg", "[building]\nshop = a.ai\n"),
            ("food.cfg", "[food]\nbone = a.ai\nmeat = b.ai\n"),
            ("fences.cfg", "[fences]\nrail = c.ai\n"),
            ("a.ai", ani),
            ("b.ai", "[Animations]\nempty = empty\nfull = full\n"),
            ("c.ai", "[Animations]\nbroke = broke\nidle = idle\n"),
        ],
    );
    assert_eq!(c.find("staff", b"keeper", b"m").unwrap().class, "keeper");
    assert!(c.find("keeper", b"keeper", b"m").is_some());
    assert!(c.find("animals", b"keeper", b"m").is_none());
    assert_eq!(
        c.find("objects", b"building", b"shop").unwrap().class,
        "building"
    );
    assert!(c.find("building", b"building", b"").is_none());
    let base = |class, kind, subtype| {
        get(&c, class, kind, subtype)
            .base_animation()
            .map(|a| a.name)
    };
    assert_eq!(
        base("building", "building", "shop").as_deref(),
        Some("empty")
    );
    assert_eq!(base("food", "food", "bone").as_deref(), Some("idle"));
    assert_eq!(base("food", "food", "meat").as_deref(), Some("full"));
    assert_eq!(base("fences", "rail", "f").as_deref(), Some("idle"));
}

/// Two colour groups for every staff type and four for every guest type (save games 11.2).
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_colour_groups() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let c = Catalogue::load(&Vfs::open(std::path::Path::new(&dir)).unwrap()).unwrap();
    for (class, groups) in [("keeper", 2), ("tour", 2), ("maint", 2), ("guests", 4)] {
        let found: Vec<_> = c.types.iter().filter(|t| t.class == class).collect();
        assert!(!found.is_empty());
        for t in found {
            let n = t.palettes();
            assert!(
                n.len() == groups && n.iter().all(|&p| p > 0),
                "{class} {n:?}"
            );
        }
    }
}

/// The numbers of the catalogue table of the specification, for the full game.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_counts() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let c = Catalogue::load(&vfs).unwrap();
    // The other installs have other updates and expansions, so other counts.
    if crate::fixtures::measured_on("full_game_counts (totals)", &dir) {
        assert_eq!(c.types.len(), 980);
        assert_eq!(
            c.counts().iter().map(|c| c.1).collect::<Vec<_>>(),
            [14, 17, 73, 173, 360, 4, 295, 11, 9, 7, 16, 1]
        );
    }
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    // Every definition file loads; five scenery types give the name id 0 themselves.
    assert!(c.types.iter().all(|t| t.config.is_some()));
    let unnamed: Vec<_> = c.types.iter().filter(|t| t.name_id == 0).collect();
    assert_eq!(unnamed.len(), 5);
    assert!(unnamed.iter().all(|t| t.class == "objects"));
    // Every base animation exists except for two animals.
    let missing: Vec<_> = c
        .types
        .iter()
        .filter(|t| {
            !t.base_animation()
                .and_then(|a| a.member)
                .is_some_and(|member| vfs.contains(&member))
        })
        .map(|t| t.class)
        .collect();
    assert_eq!(missing, ["animals", "animals"]);
    // The young apatosaurus has a key without value that sorts first: no member.
    let base = get(&c, "animals", "apato", "y").base_animation().unwrap();
    assert_eq!(base.member, None);
    // The loaded definition files are a small part of the budget.
    assert!(
        c.loaded_bytes < MAX_DEFINITION_BYTES / 2,
        "{}",
        c.loaded_bytes
    );
    // Lookups as the map files spell them.
    assert!(c.find("fences", b"zoowall", b"f").is_some());
    assert!(c.find("animals", b"lion", b"m").is_some());
    assert!(c.find("staff", b"keeper", b"f").is_some());
    assert!(c.find("building", b"building", b"restrant").is_some());
    assert!(c.find("objects", b"foliage", b"acacia").is_some());
    assert!(c.find("paths", b"paths", b"dirtpath").is_some());
}
