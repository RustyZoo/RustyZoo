//! `rustyzoo find-path`: the path the finder gives a unit between two tiles of a map, and its cost.

use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::path::{Dest, Finder, Kind, Mover, Settings, path_cost};
use rustyzoo::place;
use rustyzoo::render::MapRenderer;
use rustyzoo::terrain;
use rustyzoo::timing::ini_whole;
use rustyzoo::vfs::Vfs;
use rustyzoo::world::World;
use rustyzoo::zoo::Zoo;

use crate::{CliError, Report};

/// `x,y` as a tile.
fn tile(word: &str) -> Result<Tile, CliError> {
    let (x, y) = word.split_once(',').ok_or(CliError::Usage)?;
    Ok((
        x.parse().map_err(|_| CliError::Usage)?,
        y.parse().map_err(|_| CliError::Usage)?,
    ))
}

type Tile = (i32, i32);

/// The tiles and the unit of the command line: `<x,y> <x,y> --unit <keeper|guest|animal>`.
fn args<'a>(words: &[&'a str]) -> Result<(Tile, Tile, &'a str), CliError> {
    match words {
        [from, to, "--unit", unit @ ("keeper" | "guest" | "animal")] => {
            Ok((tile(from)?, tile(to)?, unit))
        }
        _ => Err(CliError::Usage),
    }
}

/// The unit the finder asks for: the kind, with the values of its type file that the costs read.
fn mover(vfs: &Vfs, unit: &str, at: usize) -> Result<Mover, CliError> {
    Ok(match unit {
        "keeper" => Mover::new(Kind::Keeper, Some(at)),
        "animal" => Mover::new(Kind::Animal, Some(at)),
        "guest" => {
            let class = vfs
                .read("ztunit.ai")
                .ok()
                .and_then(|b| Config::parse(&b).ok());
            let whole = |key| {
                class
                    .as_ref()
                    .and_then(|c| ini_whole(c, "Characteristics/Integers", key))
            };
            Mover {
                non_path_cost: whole("cNonPathCost").map_or(7, |v| v.max(0) as u32),
                emergency_cost: whole("cNonPathCostEmergency").map_or(1, |v| v.max(0) as u32),
                ..Mover::new(Kind::Guest, Some(at))
            }
        }
        _ => return Err(CliError::Usage),
    })
}

pub fn find_path(vfs: &Vfs, member: &str, words: &[&str]) -> Result<(), CliError> {
    let (from, to, unit) = args(words)?;
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let zoo = Zoo::parse(&vfs.read(member)?)
        .and_then(|zoo| zoo.checked(&catalogue))
        .map_err(|e| format!("{member}: {e}"))?;
    let mut world = World::new(zoo, |e| renderer.shape(e));
    world.set_water(place::water(&terrain::types(vfs)));
    let width = world.zoo.width as usize;
    let at = |(x, y): (i32, i32)| {
        world
            .tile_index(x, y)
            .ok_or_else(|| format!("{member}: tile {x},{y} is off the map"))
    };
    let (start, goal) = (at(from)?, at(to)?);
    let unit_mover = mover(vfs, unit, start)?;
    let settings =
        Config::parse(vfs.ini()).map_or_else(|_| Settings::default(), |c| Settings::from_ini(&c));
    let reply = Finder::new(settings).request(&world, &unit_mover, goal, true);
    let path = reply.path.unwrap_or_default();
    let reached = reply.dest == Dest::Set(goal);
    let state = match (reply.ok, reached) {
        (true, true) => "found",
        (true, false) => "partial",
        _ => "no path",
    };
    let mut report = Report::new();
    report.line(format_args!(
        "{unit} {},{} to {},{}: {state}, {} tiles, cost {:.4}",
        from.0,
        from.1,
        to.0,
        to.1,
        path.len(),
        path_cost(&world, &unit_mover, &path),
    ));
    for t in path {
        report.line(format_args!("{},{}", t % width, t / width));
    }
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_two_tiles_and_a_unit() {
        fn ok<'a>(w: &[&'a str]) -> Option<(Tile, Tile, &'a str)> {
            args(w).ok()
        }
        assert_eq!(
            ok(&["1,2", "30,4", "--unit", "guest"]),
            Some(((1, 2), (30, 4), "guest"))
        );
        // Zero and negative tiles parse (the map refuses them); the rest is usage.
        assert!(ok(&["0,0", "-1,0", "--unit", "keeper"]).is_some());
        for bad in [
            &["1,2", "3,4"][..],
            &["1,2", "3,4", "--unit"],
            &["1,2", "3,4", "--unit", "lion"],
            &["1", "3,4", "--unit", "animal"],
            &["1,2,3", "3,4", "--unit", "animal"],
            &["99999999999,2", "3,4", "--unit", "animal"],
            &["1,2", "3,4", "--unit", "animal", "x"],
            &[],
        ] {
            assert!(ok(bad).is_none(), "{bad:?}");
        }
    }
}
