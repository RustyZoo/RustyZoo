//! The sections kept as read, written in the form of version 106 (save games 6 and 11.2): the
//! fields a version lacks take the value the original writes for them.

use super::Shows;
use super::write::Out;

/// The date the original puts in a date field that has none: 1 January 2000.
pub(crate) const NEVER: u64 = 125_911_584_000_000_000;

/// The shows that precede the id counter: the scripts and the two counters, which a fresh map holds
/// when the file has none. Scripts are refused below 103, where their items differ.
pub(super) fn shows(v: u32, lang: u32, shows: &Shows) -> Result<Vec<u8>, String> {
    if !shows.scripts.is_empty() && v < 103 {
        return Err(format!(
            "shows with scripts of version {v} cannot be written yet"
        ));
    }
    let mut o = Out(Vec::new(), lang);
    shows.write_section(&mut o)?;
    Ok(o.0)
}
