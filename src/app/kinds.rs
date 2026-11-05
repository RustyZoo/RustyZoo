//! The kind each message carries to the message line (ui layout 4.13; the kinds the specs give).

/// The kind of a message whose caller the specs give no kind for.
pub(super) const GENERAL: i32 = 0;
/// The kind of the pause string (input 4.5).
pub(super) const PAUSE: i32 = 3;

/// The kind of message string `id` (zoo status 7, 10; animals needs 13; shows; staff; tank
/// fauna): 1 good news, 2 a warning, 4 a donation or an event, 5 no foundation.
pub(super) fn of(id: i32) -> i32 {
    match id {
        10021 | 11490 => 1,
        10008 | 10009 | 10011..=10016 | 10018..=10020 | 10022 | 10028 | 10029 | 10251 | 10528
        | 11001 | 11004 | 11005 => 2,
        10032 | 10322 | 11491 | 11492 | 15002 => 4,
        15003 => 5,
        _ => GENERAL,
    }
}
