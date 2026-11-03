//! The commands that list, read and check the files of the game.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::{CliError, Report, ignore_closed_pipe};
use rustyzoo::behaviour::{self, Survey};
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::pe;
use rustyzoo::strings::StringTables;
use rustyzoo::vfs::{self, Vfs};

pub fn which(vfs: Vfs, member: &str) -> Result<(), CliError> {
    let copies = vfs.which(member)?;
    if copies.is_empty() {
        return Err(format!("no such member: {member}").into());
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = copies
        .iter()
        .enumerate()
        .try_for_each(|(i, copy)| {
            let label = if i == 0 { "supplied by" } else { "shadowed" };
            let kind = if copy.in_archive {
                "archive"
            } else {
                "loose file"
            };
            let empty = if copy.size == 0 && !copy.in_archive {
                ", empty: reading fails"
            } else {
                ""
            };
            writeln!(
                out,
                "{label:<11} {kind} {} ({} bytes{empty})",
                escape(&copy.path.to_string_lossy()),
                copy.size
            )
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// Prints the string table and the `TEXT` entries of `dll`, a file of the install folder
/// whose name is matched ignoring letter case, or only the bytes of the `TEXT` entry `text`.
pub fn dump_strings(dir: PathBuf, dll: &str, text: Option<&str>) -> Result<(), CliError> {
    let path = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read folder {}: {e}", dir.display()))?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(dll))
                && entry.path().is_file()
        })
        .map(|entry| entry.path())
        .ok_or_else(|| format!("no file {dll} in {}", dir.display()))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if size > vfs::MAX_MEMBER_SIZE {
        return Err(format!("{} is too big for a resource DLL", path.display()).into());
    }
    let file = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let resources = pe::resources(&file).map_err(|e| format!("{dll}: {e}"))?;
    let texts = pe::texts(&resources);

    if let Some(name) = text {
        let (_, data) = texts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{dll} has no TEXT entry named {name}"))?;
        return ignore_closed_pipe(io::stdout().lock().write_all(data));
    }
    let strings = pe::strings(&resources).map_err(|e| format!("{dll}: {e}"))?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = strings
        .iter()
        .try_for_each(|(id, text)| writeln!(out, "{id}\t{}", escape(text)))
        .and_then(|()| {
            texts.iter().try_for_each(|(name, data)| {
                writeln!(out, "TEXT\t{}\t{}", escape(name), data.len())
            })
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// Prints the merged view of the `lang*.dll` libraries of the install folder.
pub fn merged_strings(dir: &Path) -> Result<(), CliError> {
    let tables = StringTables::open(dir)?;
    if tables.mixed_case() {
        let _ = writeln!(
            io::stderr(),
            "rustyzoo: warning: the lang*.dll names differ in letter case; they are ordered by their bytes"
        );
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = tables
        .libraries()
        .try_for_each(|name| writeln!(out, "LIB\t{name}"))
        .and_then(|()| {
            tables
                .strings()
                .iter()
                .try_for_each(|(id, (lib, text))| writeln!(out, "{id}\t{lib}\t{}", escape(text)))
        })
        .and_then(|()| {
            tables.texts().iter().try_for_each(|(name, (lib, data))| {
                writeln!(out, "TEXT\t{}\t{lib}\t{}", escape(name), data.len())
            })
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The text on one line: backslash, newline, carriage return and tab become `\\`, `\n`, `\r`
/// and `\t`, and any other control character becomes `\u{XXXX}`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn list(vfs: Vfs, pattern: Option<&str>) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = vfs
        .list(pattern)
        .iter()
        .try_for_each(|path| writeln!(out, "{}", escape(path)))
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The members written in the INI-like dialect (see `rustyzoo::config`).
const CONFIG_EXTENSIONS: [&str; 9] = [
    ".ai", ".cfg", ".ani", ".scn", ".lyt", ".uca", ".ucb", ".ucs", ".ini",
];

/// Parses every configuration member and reports what the original would ignore. Nothing in a
/// file is an error; only a member that cannot be read or is too large makes the command fail.
pub fn check_config(vfs: &Vfs) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failed = 0;
    let written = report_config(vfs, &mut out, &mut failed).and_then(|()| out.flush());
    ignore_closed_pipe(written)?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} configuration files could not be read").into()),
    }
}

fn report_config(vfs: &Vfs, out: &mut impl Write, failed: &mut usize) -> io::Result<()> {
    let (mut files, mut sections, mut keys, mut values) = (0, 0, 0, 0);
    let (mut empty, mut before, mut after) = (0, 0, 0);
    for path in vfs.list(None) {
        if !CONFIG_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        files += 1;
        let config = vfs
            .read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Config::parse(&bytes).map_err(|e| e.to_string()));
        let config = match config {
            Ok(config) => config,
            Err(message) => {
                *failed += 1;
                writeln!(out, "{path}: {message}")?;
                continue;
            }
        };
        for (_, section) in config.sections() {
            sections += 1;
            for key in section.keys() {
                keys += 1;
                values += section.get_all(key).len();
            }
        }
        if config.is_empty() {
            empty += 1;
            writeln!(out, "{path}: no section")?;
        }
        let (b, a) = (
            config.ignored_before_first_header,
            config.ignored_after_broken_header,
        );
        if b + a > 0 {
            writeln!(
                out,
                "{path}: ignored lines: {b} before the first header, {a} after a broken header"
            )?;
        }
        before += b;
        after += a;
    }
    writeln!(
        out,
        "{files} files, {sections} sections, {keys} keys, {values} values; \
         {empty} files without a section; ignored lines: {before} before the first header, \
         {after} after a broken header; {failed} unreadable",
        failed = *failed
    )
}

/// Prints one line per entity type, then the counts per class. Warnings go to standard error.
pub fn types(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    for warning in &catalogue.warnings {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
    }
    let name = |bytes: &[u8]| escape(&String::from_utf8_lossy(bytes));
    let mut report = Report::new();
    for ty in &catalogue.types {
        let [x, y, z] = ty.footprint;
        let member = ty.base_animation().and_then(|animation| animation.member);
        let exists = member.as_deref().is_some_and(|member| vfs.contains(member));
        let member = member.as_deref().unwrap_or("-");
        report.line(format_args!(
            "{}\t{}\t{}\t{}\t{x}x{y}x{z}\t{}\t{}",
            ty.class,
            name(&ty.type_name),
            name(&ty.subtype),
            ty.name_id,
            escape(member),
            if exists { "yes" } else { "no" },
        ));
    }
    let counts: Vec<String> = catalogue
        .counts()
        .iter()
        .map(|(class, count)| format!("{class} {count}"))
        .collect();
    report.line(format_args!(
        "{} types: {}",
        catalogue.types.len(),
        counts.join(", ")
    ));
    report.finish()
}

pub fn check_behaviours(vfs: &Vfs, type_name: Option<&str>) -> Result<(), CliError> {
    let mut report = Report::new();
    if let Some(wanted) = type_name {
        let catalogue = Catalogue::load(vfs)?;
        for warning in &catalogue.warnings {
            let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
        }
        let ty = catalogue
            .types
            .iter()
            .find(|ty| ty.type_name == wanted.as_bytes())
            .ok_or_else(|| format!("check-behaviours: no type named {}", wanted.escape_debug()))?;
        let Some(found) = catalogue.behaviours(ty) else {
            report.line(format_args!("no behaviour set or ambient table"));
            return report.finish();
        };
        for warning in &found.warnings {
            let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
        }
        for (table, name) in found.tables.iter().zip(behaviour::TABLES) {
            for row in table {
                report.line(format_args!("{name}\t{}\t{}\t{}", row.lo, row.hi, row.set));
            }
        }
        for (section, calls) in found.sets() {
            for (i, call) in calls.iter().enumerate() {
                report.line(format_args!(
                    "{}\t{}\t{}\t{:?}",
                    escape(&String::from_utf8_lossy(section)),
                    i + 1,
                    call.name,
                    call.op
                ));
            }
        }
        return report.finish();
    }
    let survey = behaviour::survey(vfs)?;
    for warning in &survey.warnings {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
    }
    let mut counts: Vec<_> = survey.counts.iter().collect();
    counts.sort_by_key(|&(name, count)| (std::cmp::Reverse(count), name));
    for (name, count) in counts {
        report.line(format_args!("{name}\t{count}"));
    }
    let Survey {
        files,
        sets,
        calls,
        tables,
        unknown,
        defects,
        ..
    } = survey;
    report.line(format_args!(
        "{files} files, {sets} sets, {calls} calls, {tables} tables; {unknown} calls of unknown names, {defects} calls with defective arguments"
    ));
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escape_keeps_a_text_on_one_line() {
        assert_eq!(escape("a\\b\n\tc\rd"), "a\\\\b\\n\\tc\\rd");
        assert_eq!(
            escape("\0\u{1b}\u{7f}\u{85}é"),
            "\\u{0000}\\u{001b}\\u{007f}\\u{0085}é"
        );
    }
}
