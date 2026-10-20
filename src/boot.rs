//! The start-up of `rustyzoo run` without a map (main menu 3): the picture and the progress bar
//! while the game data is loaded, then the main menu. The window (or nothing, headless) is
//! reached through the `present` callback.

use rustyzoo::Framebuffer;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::lists::{freeform_list, scenario_list};
use rustyzoo::menu::{self, Menu};
use rustyzoo::startup::{self, Startup};
use rustyzoo::timing::ini_whole;
use rustyzoo::ui::interface::Interface;
use rustyzoo::ui::layout::{Kind, installed_expansions};
use rustyzoo::userdir;
use rustyzoo::vfs::Vfs;

use crate::CliError;
use crate::layout::{Fill, load_interface};

/// What the start-up made: the types, the interface and the menu, and the lines that tell how it
/// went (the log opens with them).
pub struct Booted {
    pub catalogue: Catalogue,
    pub interface: Interface,
    pub menu: Menu,
    pub lines: Vec<String>,
}

/// Loads the game for a window of `size` pixels. `present` shows the start-up picture when the
/// bar moves; `music` is called where the menu music starts. The steps are counted as the original
/// counts its own (main menu 3.3 and M2): one per directory of the resource search, one per entity
/// type, one per layout of the interface and one after each stage, and the count is written to
/// `[UI] progresscalls` of the settings of the user at the end.
pub fn boot(
    vfs: &Vfs,
    ini: &Config,
    size: (u32, u32),
    show: &[i32],
    present: &mut dyn FnMut(&Framebuffer) -> Result<(), CliError>,
    music: &mut dyn FnMut(),
) -> Result<Booted, CliError> {
    let expansions = installed_expansions(vfs);
    let mut screen = Startup::new(vfs.root(), ini, expansions, size);
    let mut lines = vec![screen.log_line()];
    present(screen.frame())?;
    let mut failure = None;
    let mut step = |screen: &mut Startup| {
        if screen.step()
            && let Err(e) = present(screen.frame())
        {
            failure.get_or_insert(e);
        }
    };
    for _ in 0..vfs.directory_count() {
        step(&mut screen);
    }
    music();
    step(&mut screen);
    let catalogue = Catalogue::load_with(vfs, &mut || step(&mut screen))?;
    step(&mut screen);
    let interface = load_interface(vfs, size, show, &Fill::default())?.ok_or("no interface")?;
    let layouts = interface
        .tree
        .elements
        .iter()
        .filter(|e| matches!(e.kind, Kind::Layout))
        .count();
    for _ in 0..=layouts {
        step(&mut screen);
    }
    if let Some(e) = failure {
        return Err(e);
    }
    lines.push(format!("steps {}", screen.steps()));
    match userdir::dir() {
        Some(dir) => {
            if let Err(e) = userdir::save(&dir, "UI", "progresscalls", &screen.steps().to_string())
            {
                eprintln!("rustyzoo: warning: {e}");
            }
        }
        None => eprintln!("rustyzoo: warning: no user folder, the progress count is not kept"),
    }
    let flag = |key: &str| ini_whole(ini, "UI", key).unwrap_or(0);
    if flag("startedFirstTutorial") == 0
        || (flag("startedDinoTutorial") == 0 && expansions & 1 != 0)
        || (flag("startedAquaTutorial") == 0 && expansions & 2 != 0)
    {
        lines.push("first-run tutorial not started".to_string());
    }
    let mut menu = Menu::new(
        ini.clone(),
        vfs.root(),
        userdir::dir(),
        startup::product_version(vfs.root()),
        expansions,
    );
    let (pages, delay) = menu::credit_pages(vfs);
    menu.set_credits(pages, delay);
    menu.set_lists(scenario_list(vfs, expansions), freeform_list(vfs));
    Ok(Booted {
        catalogue,
        interface,
        menu,
        lines,
    })
}
