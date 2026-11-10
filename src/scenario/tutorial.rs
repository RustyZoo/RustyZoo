//! The seven tutorial files and the end text (scenarios 9.1).

/// The seven tutorial files (9.1), in the order of their chain.
const TUTORIALS: [&str; 7] = [
    "scn01a", "scn01", "scn02", "scn19", "scn27", "scn28", "scn28b",
];

/// The name of a tutorial among the seven (9.1), from a scenario file path, or "". The path is
/// compared byte for byte, as the end routine does (10.1 point 4).
fn tutorial(file: &str) -> &'static str {
    TUTORIALS
        .into_iter()
        .find(|t| file == format!("scenario/{t}/{t}.scn"))
        .unwrap_or("")
}

/// Whether `file` is one of the seven tutorial files (9.1), listed or not.
pub(crate) fn is_tutorial(file: &str) -> bool {
    !tutorial(file).is_empty()
}

/// The file of the tutorial that follows the tutorial `file` (9.1), given the expansion flags
/// (bit 1 the first expansion, bit 2 the second). None for any other file or without a next one.
pub(crate) fn next_tutorial(file: &str, expansions: i32) -> Option<String> {
    let next = match tutorial(file) {
        "scn01a" => "scn01",
        "scn01" => "scn02",
        "scn02" if expansions & 1 != 0 => "scn19",
        "scn02" | "scn19" if expansions & 2 != 0 => "scn27",
        "scn27" => "scn28",
        "scn28" => "scn28b",
        _ => return None,
    };
    Some(format!("scenario/{next}/{next}.scn"))
}

/// The string of the end dialog's text (9.1) for the scenario `file` and the expansion flags
/// (bit 1 the first expansion, bit 2 the second), plus 1 when lost.
pub(crate) fn end_text(file: &str, won: bool, expansions: i32) -> i32 {
    let base = match file {
        "scenario/scn01/scn01.scn" | "scenario/scn01a/scn01a.scn" => 16104,
        "scenario/scn27/scn27.scn" | "scenario/scn28/scn28.scn" => 16104,
        "scenario/scn02/scn02.scn" if expansions != 0 => 16104,
        "scenario/scn19/scn19.scn" if expansions & 2 != 0 => 16104,
        "scenario/scn02/scn02.scn" | "scenario/scn19/scn19.scn" => 16102,
        "scenario/scn28b/scn28b.scn" => 16102,
        _ => 16100,
    };
    base + i32::from(!won)
}
