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
        10008
        | 10009
        | 10010
        | 10011..=10016
        | 10018..=10020
        | 10022
        | 10028..=10031
        | 10242
        | 10251
        | 10310
        | 10314
        | 10315
        | 10528
        | 11001
        | 11002
        | 11004
        | 11005
        | 15000
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
    use crate::app::carry::refusal;
    use crate::fixtures::Install;
    use crate::ui::draw::Strings;
    use crate::ui::interface::Interface;
    use crate::ui::layout::Tree;
    use crate::vfs::Vfs;

    struct Texts;

    impl Strings for Texts {
        fn string(&self, id: i32) -> String {
            format!("m{id}")
        }

        fn text_entry(&self, _: &[u8]) -> Vec<u8> {
            vec![]
        }
    }

    /// A screen with a message line of a 100 ms count, and its interface.
    fn interface(name: &str) -> Interface {
        let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
            [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
            [b]\ntype=ZTMessageQueue\nid=7001\nx=2\ny=2\ndx=60\ndy=20\ntimed=1\n\
            timerDuration=100\nstate=1025\n";
        let install = Install::new(
            &format!("kinds-{name}"),
            &[("ui/root.lyt", root.as_bytes().to_vec())],
        );
        let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
        tree.resolve(64, 48, |_| 12);
        Interface::new(tree, Box::new(Texts))
    }

    /// Refuses `id` with `kind`, lets the count run out, refuses it again a second later: the text
    /// the line shows after the second.
    fn twice(name: &str, id: i32, kind: i32) -> Vec<u8> {
        let mut ui = interface(name);
        let mut lines = Vec::new();
        refusal(&mut ui, &mut lines, Some(id), kind);
        ui.update(0);
        ui.update(200);
        assert_eq!(ui.text(7001), Some(vec![]), "the line hid");
        ui.set_time(1000);
        refusal(&mut ui, &mut lines, Some(id), kind);
        assert_eq!(
            lines,
            vec![format!("tool message {id}"); 2],
            "both are logged"
        );
        ui.text(7001).unwrap()
    }

    #[test]
    fn a_refusal_of_the_player_is_kind_5_and_a_repeat_is_not_dropped() {
        assert_eq!(REFUSAL, 5);
        for id in [10571, 10002, 10550, 10551, 11000, 11001, 10555] {
            assert_eq!(
                twice(&format!("r{id}"), id, REFUSAL),
                format!("m{id}").as_bytes()
            );
        }
        // With a kind below 3 the same click a second later would show nothing.
        assert_eq!(twice("dropped", 10571, GENERAL), b"");
        // The gate tool's 11001 is 5, the keeper goal's is a warning of 2.
        assert_eq!(of(11001), 2);
        assert_eq!(of(15003), REFUSAL);
    }

    #[test]
    fn an_assign_refusal_and_a_dialog_text_are_kind_3() {
        assert_eq!(PLAIN, 3);
        assert_eq!(twice("assign", 10323, PLAIN), b"m10323");
        for id in [23020, 23021, 10007, 10323, 10507, 10508, 11493, 400] {
            assert_eq!(of(id), if id == 400 { GENERAL } else { PLAIN }, "{id}");
        }
    }

    #[test]
    fn every_animal_and_keeper_warning_is_kind_2() {
        for id in [
            10008, 10009, 10010, 10011, 10012, 10013, 10014, 10015, 10016, 10018, 10019, 10020,
            10022, 10028, 10029, 10030, 10031, 10242, 10251, 10310, 10314, 10315, 10528, 11001,
            11002, 11004, 11005, 15000, 15001,
        ] {
            assert_eq!(of(id), 2, "{id}");
        }
        assert_eq!([10021, 11490].map(of), [1, 1]);
        assert_eq!([10032, 10322, 11491, 11492, 15002].map(of), [4; 5]);
        assert_eq!([10017, 10549, 10027, -1, 0, i32::MAX].map(of), [GENERAL; 6]);
    }
}
