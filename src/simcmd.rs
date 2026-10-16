//! `rustyzoo sim`: runs a map for a number of fixed steps without an interface and prints the log
//! of the steps or the hash of the state (the same on every system).

use rustyzoo::animal::Class;
use rustyzoo::entity::Catalogue;
use rustyzoo::exhibit::TankConfig;
use rustyzoo::place;
use rustyzoo::render::MapRenderer;
use rustyzoo::sim::drive::{self, Run};
use rustyzoo::terrain;
use rustyzoo::vfs::Vfs;
use rustyzoo::world::World;
use rustyzoo::zoo::Zoo;

use crate::{CliError, Report};

/// The length of a step in milliseconds when `--dt` is not given.
const DEFAULT_DT: u32 = 67;

/// The seed of a run without `--seed`: the wall clock would give another hash at every run.
const DEFAULT_SEED: u32 = 1;

#[derive(Debug, PartialEq)]
struct Flags<'a> {
    ticks: u64,
    dt: u32,
    seed: u32,
    script: Option<&'a str>,
    log: bool,
}

fn flags<'a>(mut words: &[&'a str]) -> Result<Flags<'a>, CliError> {
    let mut f = Flags {
        ticks: 0,
        dt: DEFAULT_DT,
        seed: DEFAULT_SEED,
        script: None,
        log: false,
    };
    let (mut ticks, mut hash) = (false, false);
    while let [word, rest @ ..] = words {
        words = match (*word, rest) {
            ("--ticks", [n, rest @ ..]) => {
                (f.ticks, ticks) = (n.parse().map_err(|_| CliError::Usage)?, true);
                rest
            }
            ("--dt", [n, rest @ ..]) => {
                f.dt = n.parse().ok().filter(|&n| n > 0).ok_or(CliError::Usage)?;
                rest
            }
            ("--seed", [n, rest @ ..]) => {
                f.seed = n.parse().map_err(|_| CliError::Usage)?;
                rest
            }
            ("--script", [file, rest @ ..]) => {
                f.script = Some(file);
                rest
            }
            ("--hash", rest) => {
                hash = true;
                rest
            }
            ("--log", rest) => {
                f.log = true;
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    match ticks && !(hash && f.log) {
        true => Ok(f),
        false => Err(CliError::Usage),
    }
}

pub fn sim(vfs: &Vfs, member: &str, words: &[&str]) -> Result<(), CliError> {
    let flags = flags(words)?;
    let commands = match flags.script {
        Some(file) => {
            let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            drive::parse(&text)?
        }
        None => Vec::new(),
    };
    if let Some((step, _)) = commands.iter().find(|(step, _)| *step >= flags.ticks) {
        eprintln!("rustyzoo: warning: script step {step} is never reached");
    }
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let (zoo, money) = Zoo::parse_money(&vfs.read(member)?)
        .and_then(|(zoo, money)| Ok((zoo.checked(&catalogue)?, money)))
        .map_err(|e| format!("{member}: {e}"))?;
    let mut world = World::new(zoo, |e| renderer.shape(e));
    world.set_water(place::water(&terrain::types(vfs)));
    world.set_tank_config(TankConfig::load(vfs));
    let economy = vfs.read("economy.cfg").unwrap_or_else(|e| {
        eprintln!("rustyzoo: warning: economy.cfg: {e}; the built-in economy settings are used");
        Vec::new()
    });
    let mut run = Run::new(
        (vfs.ini(), &economy),
        (catalogue, Class::load(vfs)),
        world,
        money,
        flags.seed,
    );
    let mut report = Report::new();
    let mut commands = commands.into_iter().peekable();
    for step in 0..flags.ticks {
        while let Some((_, command)) = commands.next_if(|(at, _)| *at == step) {
            if !run.apply(&command) {
                eprintln!("rustyzoo: warning: step {step}: {command:?} changed nothing");
            }
        }
        let lines = run.step_with(flags.dt, Some(vfs));
        if flags.log {
            for line in lines {
                report.line(format_args!("step {step} {line}"));
            }
        }
    }
    if !flags.log {
        report.line(format_args!("{:016x}", run.hash()));
    }
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_need_ticks_and_one_output() {
        let f = flags(&["--ticks", "5"]).ok().unwrap();
        assert_eq!((f.ticks, f.dt, f.seed, f.log), (5, 67, 1, false));
        let f = flags(&["--ticks", "0", "--dt", "1", "--seed", "4294967295", "--log"])
            .ok()
            .unwrap();
        assert_eq!((f.ticks, f.dt, f.seed, f.log), (0, 1, u32::MAX, true));
        assert_eq!(flags(&["--hash", "--ticks", "1"]).ok().unwrap().ticks, 1);
        for bad in [
            &[][..],
            &["--hash"],
            &["--ticks"],
            &["--ticks", "-1"],
            &["--ticks", "x"],
            &["--ticks", "1", "--dt", "0"],
            &["--ticks", "1", "--dt", "-5"],
            &["--ticks", "1", "--dt", "4294967296"],
            &["--ticks", "1", "--seed", "-1"],
            &["--ticks", "1", "--script"],
            &["--ticks", "1", "--hash", "--log"],
            &["--ticks", "1", "--no"],
        ] {
            assert!(flags(bad).is_err(), "{bad:?}");
        }
    }
}
