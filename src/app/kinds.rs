//! The kind each message carries to the message line (ui layout 4.13; the kinds the specs give).

/// The kind of a message whose caller the binary posts with kind 0 (and of a string no table lists).
pub(super) const GENERAL: i32 = 0;
/// Kind 3: the pause string, a refusal of the staff assign mode, a dialog text. Never dropped.
pub(super) const PLAIN: i32 = 3;
/// Kind 5: every refusal of the player (a place, a move, the gate tool, the tank panel, the
/// bulldozer). Never dropped, so a refused click repeated at once shows again.
pub(super) const REFUSAL: i32 = 5;

/// The kind of message string `id` as the simulation posts it (ui layout 4.13 point 13): 1 good
/// news, 2 a warning, 3 a dialog text, 4 an event or a donation, 5 no foundation. The callers
/// that show a refusal of the player or a dialog pass their kind themselves.
pub(super) fn of(id: i32) -> i32 {
    match id {
        10021 | 11490 => 1,
        10008 | 10009 | 10010 | 10011..=10016 | 10018..=10020 | 10022 | 10028..=10031 | 10242
        | 10251 | 10310 | 10314 | 10315 | 10528 | 11001 | 11002 | 11004 | 11005 | 15000
        | 15001 => 2,
        10007 | 10323 | 10507 | 10508 | 11493 | 23020 | 23021 => PLAIN,
        10032 | 10322 | 11491 | 11492 | 15002 => 4,
        15003 => REFUSAL,
        _ => GENERAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_the_specs_give_and_the_default_for_the_rest() {
        assert_eq!(
            [
                10021,
                10009,
                10032,
                15002,
                15003,
                10017,
                400,
                -1,
                0,
                i32::MAX
            ]
            .map(of),
            [1, 2, 4, 4, 5, 0, 0, 0, 0, 0]
        );
        assert_eq!([10028, 11004, 11491].map(of), [2, 2, 4]);
    }
}
