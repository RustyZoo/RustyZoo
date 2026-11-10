//! The colour tab of a building panel through the pass (guest buildings 20): the tab, the colour
//! button, the palette and the choice, on the map of the bulldozer tests with made-up layouts.

use super::bulldozer_tests::{ROOT, Session, catalogue, ty, with_game_of};
use crate::panel::building::{COLOUR_BUTTONS, COLOURS, PALETTE};

const PANEL: &str = "[LayoutInfo]\nid=20\nlayer=7\nx=0\ny=0\ndx=60\ndy=60\n\
    [a]\ntype=UIEditableText\nid=4602\ndx=8\ndy=8\n\
    [b]\ntype=UIButton\nid=4675\ndx=8\ndy=8\nstate=8\nstate=4096\ntransparent=0\n\
    [c]\ntype=UIButton\nid=4677\nx=10\ndx=8\ndy=8\nstate=4096\naction=3\ntarget=302\ntransparent=0\n\
    [d]\ntype=UIRadioSet\nid=4674\nbutton=4675\nbutton=4677\n\
    [e]\ntype=UILayout\nlayout=ui/colours.lyt\nstate=1\n";
const COLOURS_LAYOUT: &str = "[LayoutInfo]\nid=302\nlayer=8\nx=0\ny=20\ndx=60\ndy=40\n\
    [a]\ntype=UIButton\nid=2461\ndx=8\ndy=8\ncolorasicon=1\ntransparent=0\n\
    [b]\ntype=UIButton\nid=2462\nx=10\ndx=8\ndy=8\ncolorasicon=1\ntransparent=0\n\
    [c]\ntype=UIText\nid=2451\ndx=8\ndy=8\n[d]\ntype=UIText\nid=2452\ndx=8\ndy=8\n";
const AI: &str = "[cr_a]\nncolors=4\npal=p/a.pal\nui_info=ra\npal=p/b.pal\nui_info=rb\n\
    pal=p/c.pal\nui_info=rc\n[ra]\nforecolor=200\nforecolor=0\nforecolor=0\n\
    [rb]\nforecolor=10\nforecolor=50\nforecolor=0\n[rc]\nforecolor=10\nforecolor=20\nforecolor=99\n";
/// The building of the type with the colour group and the one of a type with none.
const PAINTED: &str = "[Characteristics/Integers]\ncPurchaseCost=100\ncIsColorReplaced=1\n\
    [Satisfies]\nbuilding\n[colorrep]\ncolor=cr_color\nreplace=cr_a\ntitle=2300\ndefaultpal=p/b.pal\n";

fn with_paint(name: &str, body: impl FnOnce(&mut Session)) {
    let swatches: String = (2401..=2424)
        .map(|i| {
            format!("[s{i}]\ntype=UIButton\nid={i}\ndx=8\ndy=8\ncolorasicon=1\ntransparent=0\n")
        })
        .collect();
    let palette = format!("[LayoutInfo]\nid=37\nlayer=9\nx=0\ny=0\ndx=60\ndy=60\n{swatches}");
    let root = format!(
        "{ROOT}[k]\ntype=UILayout\nlayout=ui/panel.lyt\nstate=1\n[l]\ntype=UILayout\nlayout=ui/color.lyt\nstate=1\n"
    );
    let files = [
        ("ui/panel.lyt", PANEL.as_bytes().to_vec()),
        ("ui/colours.lyt", COLOURS_LAYOUT.as_bytes().to_vec()),
        ("ui/color.lyt", palette.into_bytes()),
        ("building.ai", AI.as_bytes().to_vec()),
    ];
    let mut cat = catalogue();
    cat.push(ty("building", b"paint", b"", PAINTED, [2, 2, 1]));
    with_game_of(name, "", (cat, &root, &files), body);
}

fn press(s: &mut Session, id: i32) -> Vec<String> {
    assert!(s.app.ui.as_mut().unwrap().press_button(id), "{id}");
    (0..4).flat_map(|_| s.log(&[])).collect()
}

fn shown(s: &Session, layout: i32) -> bool {
    s.app.ui.as_ref().unwrap().shown(layout)
}

#[test]
fn the_colour_buttons_open_the_palette_and_a_swatch_recolours_the_building() {
    with_paint("colour-tab", |s| {
        let id = s.add(20, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[]);
        s.click(s.at(20, 20, 32, 32));
        (0..4).for_each(|_| _ = s.pass(&[]));
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(id));
        let ui = s.app.ui.as_ref().unwrap();
        assert!(ui.shown(20) && !ui.shown(COLOURS) && !ui.shown(PALETTE));
        // The colour tab shows the colour page, and its button the default colour.
        press(s, 4677);
        assert!(shown(s, COLOURS));
        assert_eq!(
            s.app.ui.as_ref().unwrap().fore_color(COLOUR_BUTTONS[0]),
            Some([10, 50, 0])
        );
        // A press on the colour button opens the palette: the third swatch is the red one.
        press(s, COLOUR_BUTTONS[0]);
        assert!(shown(s, PALETTE));
        assert_eq!(
            s.app.ui.as_ref().unwrap().fore_color(2403),
            Some([200, 0, 0])
        );
        // The first swatch (blue, the third variant) is the building's choice at once.
        let lines = press(s, 2401);
        assert!(
            lines.contains(&format!("panel building colour {id} 0 2")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "world stage"),
            "the picture is made again"
        );
        assert!(!shown(s, PALETTE));
        let world = &s.app.game.as_ref().unwrap().world;
        let colors = &world.zoo.entities[world.find(id).unwrap()]
            .common
            .as_ref()
            .unwrap()
            .colors;
        assert_eq!(colors, &[2]);
        assert_eq!(
            s.app.ui.as_ref().unwrap().fore_color(COLOUR_BUTTONS[0]),
            Some([10, 20, 99])
        );
        // Leaving the colour page hides the palette with it.
        press(s, COLOUR_BUTTONS[0]);
        assert!(shown(s, PALETTE));
        press(s, 4675);
        assert!(!shown(s, COLOURS) && !shown(s, PALETTE));
    });
}
