//! The commands that read and check maps, movies and the ratings of exhibits.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;

use super::{CliError, Report, open_game};
use rustyzoo::entity::Catalogue;
use rustyzoo::exhibit::{rating, viewing};
use rustyzoo::movie::Movie;
use rustyzoo::render::MapRenderer;
use rustyzoo::sprite::write_png;
use rustyzoo::strings;
use rustyzoo::vfs::Vfs;
use rustyzoo::world::World;
use rustyzoo::zoo::{Zoo, ZooError};

pub fn load_map(vfs: &Vfs, member: &str) -> Result<Zoo, CliError> {
    let bytes = vfs.read(member)?;
    Ok(Zoo::parse(&bytes).map_err(|e| format!("{member}: {e}"))?)
}

/// Reads a movie from a file, or else from the game, decodes it, and prints what it holds.
pub fn movie_info(
    data_dir: Option<OsString>,
    source: &str,
    dump: Option<(usize, &Path)>,
) -> Result<(), CliError> {
    let movie = Movie::parse(read_source(data_dir, source)?)?;
    let mut report = Report::new();
    report.line(format_args!("movie: {source}"));
    report.line(format_args!("size: {}x{}", movie.size().0, movie.size().1));
    report.line(format_args!("frames: {}", movie.frame_count()));
    let (rate, scale) = movie.frame_rate();
    report.line(format_args!(
        "frame rate: {rate}/{scale} ({:.2} per second)",
        f64::from(rate) / f64::from(scale)
    ));
    report.line(format_args!("duration: {} ms", movie.duration_ms()));
    match movie.audio_format() {
        Some(a) => report.line(format_args!(
            "audio: {} Hz, {} channels, {} bits, {} bytes",
            a.rate,
            a.channels,
            a.bits,
            movie.audio().len()
        )),
        None => report.line(format_args!("audio: none")),
    }
    if let Some((_, dir)) = dump {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let (mut decoded, mut dumped) = (0, 0);
    for (number, frame) in movie.frames().enumerate() {
        let frame = frame.map_err(|e| format!("frame {number}: {e}"))?;
        decoded += 1;
        if let Some((_, dir)) = dump.filter(|(k, _)| number % k == 0) {
            let path = dir.join(format!("frame-{number:06}.png"));
            let file = File::create(&path)
                .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
            write_png(&frame, io::BufWriter::new(file))
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            dumped += 1;
        }
    }
    report.line(format_args!("decoded: {decoded} frames"));
    if dump.is_some() {
        report.line(format_args!("dumped: {dumped} pictures"));
    }
    report.finish()
}

/// The bytes of a file that exists at `source`, else of the game member of that name.
fn read_source(data_dir: Option<OsString>, source: &str) -> Result<Vec<u8>, CliError> {
    if Path::new(source).is_file() {
        std::fs::read(source).map_err(|e| format!("cannot read {source}: {e}").into())
    } else {
        Ok(open_game(data_dir)?.read(source)?)
    }
}

/// Reads every `.zoo` member. A file the original refuses by its header (not a saved game, a
/// version below 38 or above 106) is a refusal, as expected from leftovers; anything else that
/// fails, including a version below 70 with an entity of an undefined type, is a corrupted
/// file, which makes the command fail.
pub fn check_maps(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let mut report = Report::new();
    let (mut files, mut ok, mut refused, mut corrupted) = (0, 0, 0, 0);
    let (mut tiles, mut entities) = (0, 0);
    for member in vfs.list(None) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        files += 1;
        let name = member.escape_debug();
        let result = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        match result {
            Ok(zoo) => {
                ok += 1;
                tiles += zoo.tiles.len();
                entities += zoo.entities.len();
                report.line(format_args!(
                    "ok {name}: version {}, {}x{}, {} tiles, {} entities",
                    zoo.version,
                    zoo.width,
                    zoo.height,
                    zoo.tiles.len(),
                    zoo.entities.len()
                ));
            }
            Err(err) => {
                match err {
                    ZooError::Corrupted(_) => corrupted += 1,
                    _ => refused += 1,
                }
                // The text comes from the file system: keep it on one line.
                report.line(format_args!(
                    "FAILED {name}: {}",
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} map files, {ok} ok, {refused} refused by version or magic, \
         {corrupted} corrupted; {tiles} tiles, {entities} entities in the ok files"
    ));
    report.finish()?;
    match corrupted {
        0 => Ok(()),
        _ => Err(format!("{corrupted} of {files} map files are corrupted").into()),
    }
}

/// Finds the regions of the maps that match `pattern` (a member name, or a pattern with `*` as
/// for `ls`) from their fences and compares the number of exhibits with the habitat count the file
/// stores, and checks that each stored first tile lies in a different exhibit; the same for the
/// tanks. A mismatch is listed and fails the command.
pub fn check_exhibits(vfs: &Vfs, pattern: &str) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let language = strings::Language::of(vfs.root());
    let mut report = Report::new();
    let (mut maps, mut agree, mut mismatch, mut skipped) = (0, 0, 0, 0);
    for member in vfs.list(Some(pattern)) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        maps += 1;
        let name = member.escape_debug();
        let zoo = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        let zoo = match zoo {
            Ok(zoo) => zoo,
            Err(err) => {
                skipped += 1;
                report.line(format_args!(
                    "skipped {name}: {}",
                    err.to_string().escape_debug()
                ));
                continue;
            }
        };
        let stored = zoo.habitat_tiles.len();
        let tiles = zoo.habitat_tiles.clone();
        let stored_tanks = zoo.tanks.clone();
        let world = World::with_language(zoo, |e| renderer.shape(e), language.clone());
        let list = world.exhibits().list();
        let distinct = world.exhibits().distinct(&world, &tiles);
        let sizes: Vec<String> = list.iter().map(|e| e.tiles.to_string()).collect();
        // The tanks found from the walls are the stored ones: as many, and each stored tank's
        // first tile lies in one.
        let tanks = list.iter().filter(|e| e.tank.is_some()).count();
        let tank_tiles = stored_tanks
            .iter()
            .filter(|t| {
                world
                    .tile_index(t.tile.0, t.tile.1)
                    .is_some_and(|i| world.tank_at(i).is_some())
            })
            .count();
        let stored_tanks = stored_tanks.len();
        let verdict = if list.len() == stored
            && distinct == stored
            && tanks == stored_tanks
            && tank_tiles == stored_tanks
        {
            agree += 1;
            "ok"
        } else {
            mismatch += 1;
            "MISMATCH"
        };
        report.line(format_args!(
            "{verdict} {name}: {} exhibits, {stored} stored, {distinct} of {stored} stored first \
             tiles in distinct exhibits; {tanks} tanks, {stored_tanks} stored, {tank_tiles} of \
             {stored_tanks} stored tank tiles in a tank; sizes [{}]",
            list.len(),
            sizes.join(" ")
        ));
    }
    report.line(format_args!(
        "{maps} maps, {agree} agree, {mismatch} differ, {skipped} skipped"
    ));
    report.finish()?;
    match (maps, mismatch) {
        (0, _) => Err(format!("check-exhibits: no map matches {}", pattern.escape_debug()).into()),
        (_, 0) => Ok(()),
        _ => {
            Err(format!("{mismatch} of {maps} maps differ from their stored habitat count").into())
        }
    }
}

/// Rates every exhibit of the maps matching `pattern` for each species in it (habitat rating).
pub fn check_rating(vfs: &Vfs, pattern: &str) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let language = strings::Language::of(vfs.root());
    let mut report = Report::new();
    let (mut maps, mut rated, mut bad) = (0, 0, 0);
    for member in vfs.list(Some(pattern)) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        let zoo = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        let Ok(zoo) = zoo else {
            report.line(format_args!("skipped {}", member.escape_debug()));
            continue;
        };
        maps += 1;
        let world = World::with_language(zoo, |e| renderer.shape(e), language.clone());
        let ground = viewing::Ground::of(&world, viewing::bodies(&catalogue));
        let wear = viewing::wear(&world, viewing::decayed(&catalogue));
        for exhibit in world.exhibits().list() {
            let input = rating::input(&world, &catalogue, exhibit.id);
            let mut state = rating::State::new();
            state.compute(&input);
            report.line(format_args!(
                "{} exhibit {}{}: {} tiles, {} animals, attractiveness {}",
                member.escape_debug(),
                exhibit.id,
                if exhibit.tank.is_some() {
                    " (tank)"
                } else {
                    ""
                },
                exhibit.tiles,
                input.animals.len(),
                state.attractiveness,
            ));
            let plan = viewing::Plan::of(&world, exhibit.id);
            let mut areas = exhibit.areas.clone();
            report.line(format_args!(
                "  {} viewing areas, deterioration {:?}",
                areas.len(),
                wear.get(&exhibit.id).copied().unwrap_or_default(),
            ));
            for (n, area) in areas.iter_mut().enumerate() {
                area.observe(&plan, &ground);
                let (xs, ys): (Vec<_>, Vec<_>) = (area.tiles.iter())
                    .map(|&t| (t % plan.width, t / plan.width))
                    .unzip();
                let span =
                    |v: &[usize]| v.iter().max().unwrap_or(&0) - v.iter().min().unwrap_or(&0) + 1;
                // An area covers at most 4 columns and 4 rows.
                bad += usize::from(span(&xs) > 4 || span(&ys) > 4);
                report.line(format_args!(
                    "    area {n}: {} tiles from ({}, {}), {} columns x {} rows, sees {} tiles",
                    area.tiles.len(),
                    xs[0],
                    ys[0],
                    span(&xs),
                    span(&ys),
                    area.seen.len(),
                ));
            }
            for r in &state.records {
                rated += 1;
                let bits: Vec<String> = (0..64)
                    .filter(|&b| r.has(b))
                    .map(|b| format!("{b}{}", if r.important >> b & 1 == 1 { "!" } else { "" }))
                    .collect();
                if r.shown() > 100 {
                    bad += 1;
                }
                report.line(format_args!(
                    "  {} x{}: shown {} (rating {:.2}) terrain {} objects {} foliage {} rocks {} \
                     elevation {} shelters {} toys {} depth {} water {}; bits [{}]",
                    r.key.escape_debug(),
                    r.animals,
                    r.shown(),
                    r.rating(),
                    r.terrain,
                    r.objects,
                    r.foliage,
                    r.rocks,
                    r.elevation,
                    r.shelters,
                    r.toys,
                    r.depth,
                    r.water,
                    bits.join(" "),
                ));
            }
        }
    }
    report.line(format_args!(
        "{maps} maps, {rated} species rated, {bad} out of range"
    ));
    report.finish()?;
    match (maps, bad) {
        (0, _) => Err(format!("check-rating: no map matches {}", pattern.escape_debug()).into()),
        (_, 0) => Ok(()),
        _ => Err(
            format!("{bad} ratings are not 0 to 100 or areas cover more than 4 x 4 tiles").into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::read_source;

    #[test]
    fn a_path_that_exists_is_read_as_a_file() {
        let path = std::env::temp_dir().join(format!("rustyzoo-source-{}", std::process::id()));
        std::fs::write(&path, b"zoo").unwrap();
        let read = read_source(None, path.to_str().unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(read.ok(), Some(b"zoo".to_vec()));
    }
}
