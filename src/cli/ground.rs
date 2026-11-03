//! The commands that check and draw the terrain pictures.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io;
use std::path::Path;

use super::{CliError, Report};
use rustyzoo::ground::{self, Key, Renderer};
use rustyzoo::sprite::write_png;
use rustyzoo::vfs::Vfs;

/// Differences between the rendered and the cached pictures of one terrain type.
struct GroundStats {
    pictures: usize,
    /// Pictures where some pixel is covered in only one of the two.
    differing: usize,
    /// Pixels covered in only one of the two.
    uncommon: usize,
    /// Pictures whose cut differs although the covered pixels do not.
    boxes: usize,
    /// Pixels covered in both, and the sum of the absolute differences per channel.
    covered: u64,
    sums: [u64; 3],
    /// How many (pixel, channel) pairs differ by each amount.
    histogram: Vec<u64>,
}

impl GroundStats {
    fn new() -> GroundStats {
        GroundStats {
            pictures: 0,
            differing: 0,
            uncommon: 0,
            boxes: 0,
            covered: 0,
            sums: [0; 3],
            histogram: vec![0; 256],
        }
    }

    fn add(&mut self, other: &GroundStats) {
        self.pictures += other.pictures;
        self.differing += other.differing;
        self.uncommon += other.uncommon;
        self.boxes += other.boxes;
        self.covered += other.covered;
        for c in 0..3 {
            self.sums[c] += other.sums[c];
        }
        for (a, b) in self.histogram.iter_mut().zip(&other.histogram) {
            *a += b;
        }
    }

    /// The smallest difference that at least `percent` of the (pixel, channel) pairs do not exceed.
    fn percentile(&self, percent: u64) -> usize {
        let total: u64 = self.histogram.iter().sum();
        let mut seen = 0;
        for (value, count) in self.histogram.iter().enumerate() {
            seen += count;
            if seen * 100 >= total * percent {
                return value;
            }
        }
        0
    }

    /// The counts, then after a semicolon the colour differences. `extra` goes at the end of the
    /// counts.
    fn describe(&self, extra: &str) -> String {
        let mean = self.sums.map(|s| s as f64 / self.covered.max(1) as f64);
        format!(
            "{} pictures, {} covered pixels, {} pictures with differing covered pixels, \
             {} differing pixels, {} pictures with another cut{extra}; colour difference per \
             channel over covered pixels: mean R {:.2} G {:.2} B {:.2}, 95th percentile {}, max {}",
            self.pictures,
            self.covered,
            self.differing,
            self.uncommon,
            self.boxes,
            mean[0],
            mean[1],
            mean[2],
            self.percentile(95),
            self.histogram.iter().rposition(|&n| n > 0).unwrap_or(0),
        )
    }
}

/// Renders every picture of the terrain cache again and compares the pixels both cover.
pub fn check_ground(vfs: &Vfs) -> Result<(), CliError> {
    let renderer = Renderer::load(vfs)?;
    let mut report = Report::new();
    let mut per_type: BTreeMap<(u8, bool), GroundStats> = Default::default();
    let mut failed = 0;
    let members = [vfs.list(Some("ztats/*.lle")), vfs.list(Some("ztatb/*.lle"))].concat();
    let palettes: HashSet<String> = [vfs.list(Some("ztats/*.pal")), vfs.list(Some("ztatb/*.pal"))]
        .concat()
        .into_iter()
        .collect();
    let (mut plain, mut blended) = (GroundStats::new(), GroundStats::new());
    let mut unusable = 0;
    for member in members {
        // The original renders a picture whose palette is missing, so there is nothing to compare.
        if Key::parse(&member).is_ok_and(|key| !palettes.contains(&key.palette())) {
            unusable += 1;
            continue;
        }
        let compared = ground::load_cached(vfs, &member).and_then(|(key, cached)| {
            let rendered = renderer.render(&key)?;
            Ok((key, cached, rendered))
        });
        let (key, cached, rendered) = match compared {
            Ok(compared) => compared,
            Err(err) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
                continue;
            }
        };
        let mut one = GroundStats::new();
        one.pictures = 1;
        // The cached picture is drawn, so it is as the screen shows it; so is the rendered one.
        let (a, mut b) = (cached.on_canvas(), rendered.on_canvas());
        for pixel in b.pixels.as_chunks_mut::<4>().0 {
            let [r, g, bl] = rustyzoo::sprite::screen([pixel[0], pixel[1], pixel[2]]);
            pixel[..3].copy_from_slice(&[r, g, bl]);
        }
        for (a, b) in a
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.pixels.as_chunks::<4>().0)
        {
            match (a[3] != 0, b[3] != 0) {
                (true, true) => {
                    one.covered += 1;
                    for c in 0..3 {
                        let difference = a[c].abs_diff(b[c]);
                        one.sums[c] += u64::from(difference);
                        one.histogram[usize::from(difference)] += 1;
                    }
                }
                (false, false) => {}
                _ => one.uncommon += 1,
            }
        }
        if one.uncommon > 0 {
            one.differing = 1;
            report.line(format_args!("DIFFERS {member}: {} pixels", one.uncommon));
        } else if (
            cached.image.width,
            cached.image.height,
            cached.x_offset,
            cached.y_offset,
        ) != (
            rendered.image.width,
            rendered.image.height,
            rendered.x_offset,
            rendered.y_offset,
        ) {
            one.boxes = 1;
            report.line(format_args!("CUT {member}"));
        }
        per_type
            .entry((key.terrain, key.is_uniform()))
            .or_insert_with(GroundStats::new)
            .add(&one);
        if key.is_uniform() {
            &mut plain
        } else {
            &mut blended
        }
        .add(&one);
    }
    let mut total = GroundStats::new();
    for ((terrain, uniform), stats) in &per_type {
        let kind = if *uniform { "plain" } else { "blended" };
        report.line(format_args!(
            "type {terrain:02x} {kind}: {}",
            stats.describe("")
        ));
        total.add(stats);
    }
    report.line(format_args!(
        "plain (ztats, lit in the cache, rendered unlit): {}",
        plain.describe("")
    ));
    report.line(format_args!("blended (ztatb): {}", blended.describe("")));
    report.line(format_args!(
        "total: {}",
        total.describe(&format!(", {unusable} without palette, {failed} failed"))
    ));
    report.finish()?;
    if total.uncommon > 0 || total.boxes > 0 || failed > 0 {
        return Err(format!(
            "{} pictures differ in covered pixels, {} in their cut, {failed} failed",
            total.differing, total.boxes
        )
        .into());
    }
    Ok(())
}

pub fn render_ground(vfs: &Vfs, key: &str, out: &Path) -> Result<(), CliError> {
    let key = Key::parse(key)?;
    let picture = Renderer::load(vfs)?.render(&key)?;
    if picture.image.width == 0 {
        return Err(format!("{} draws nothing", key.member()).into());
    }
    let file = File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    write_png(&picture.image, io::BufWriter::new(file))
        .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!(
        "{}: {}x{} x offset {} y offset {}",
        key.member(),
        picture.image.width,
        picture.image.height,
        picture.x_offset,
        picture.y_offset
    );
    Ok(())
}
