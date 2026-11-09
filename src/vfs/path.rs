//! The search path: reading the ini file, cutting the value and resolving its elements.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::VfsError;

/// The largest ini file we read: the real ones are a few hundred bytes.
pub(super) const MAX_INI_SIZE: u64 = 1024 * 1024;

/// Reads the ini file, which must be a regular file of at most [`MAX_INI_SIZE`] bytes: a FIFO
/// would hang the read, a device or a huge file would fill the memory.
pub(crate) fn read_ini(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut data = Vec::new();
    file.take(MAX_INI_SIZE + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_INI_SIZE {
        return Err(io::Error::other(format!("more than {MAX_INI_SIZE} bytes")));
    }
    Ok(data)
}

/// The value of `path` in the `[resource]` section of the ini file next to the executable. The
/// game reads that file through the ini reader of Windows, not through its own configuration
/// reader (`crate::config`): names ignore letter case, only `[section]` headers, `key=value`
/// lines and `;` comment lines count, the first `path` wins, and a value in a pair of single
/// or double quotes loses them.
pub(super) fn resource_path(ini: &str) -> Option<String> {
    let mut in_resource = false;
    for line in ini.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[') {
            let resource = section
                .strip_suffix(']')
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("resource"));
            // Only the first section of that name is read.
            if in_resource {
                return None;
            }
            in_resource = resource;
        } else if let Some((key, value)) = line.split_once('=')
            && in_resource
            && key.trim().eq_ignore_ascii_case("path")
        {
            let value = value.trim();
            let quoted = value.len() >= 2
                && (value.starts_with('"') && value.ends_with('"')
                    || value.starts_with('\'') && value.ends_with('\''));
            return Some(
                if quoted {
                    &value[1..value.len() - 1]
                } else {
                    value
                }
                .to_string(),
            );
        }
    }
    None
}

/// The longest value the game reads for the search path: its buffer is 1,024 bytes.
pub(super) const MAX_PATH_BYTES: usize = 1023;

/// The search path cut to [`MAX_PATH_BYTES`] bytes of the ini file, and whether it was cut. A
/// byte that is not UTF-8 is one byte of the file (see `name_from_bytes`), though several of ours.
pub(super) fn cut_path(path: String) -> (String, bool) {
    let mut bytes = 0;
    for (at, c) in path.char_indices() {
        let width = if ('\u{F700}'..='\u{F7FF}').contains(&c) {
            1
        } else {
            c.len_utf8()
        };
        if bytes + width > MAX_PATH_BYTES {
            return (path[..at].to_string(), true);
        }
        bytes += width;
    }
    (path, false)
}

/// The entry of `directory` called `name` in any letter case, of the given kind (directory or
/// file). If several differ only in case, the first in sorted order is taken.
pub(crate) fn find_ignoring_case(
    directory: &Path,
    name: &str,
    want_directory: bool,
) -> Result<Option<PathBuf>, VfsError> {
    let entries = std::fs::read_dir(directory).map_err(|e| {
        VfsError(
            format!("cannot read folder {}: {e}", directory.display()),
            false,
        )
    })?;
    let entries = entries.collect::<Result<Vec<_>, _>>().map_err(|e| {
        VfsError(
            format!("cannot read folder {}: {e}", directory.display()),
            false,
        )
    })?;
    let mut found: Vec<PathBuf> = entries
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() == want_directory)
        .collect();
    found.sort();
    Ok(found.into_iter().next())
}

/// Resolves one entry of the search path: against the install folder when it is relative, as it
/// stands when it is absolute. `None` if the directory does not exist. Like the original, an
/// entry may leave the install folder (`..`, an absolute path).
pub(super) fn resolve_directory(
    install_dir: &Path,
    entry: &str,
) -> Result<Option<PathBuf>, VfsError> {
    use std::path::Component;
    let mut directory = install_dir.to_path_buf();
    for component in Path::new(&entry.replace('\\', "/")).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => directory.push(".."),
            Component::RootDir => directory.push(component.as_os_str()),
            Component::Prefix(_) => directory = PathBuf::from(component.as_os_str()),
            Component::Normal(name) => {
                let found = match name.to_str() {
                    Some(name) => find_ignoring_case(&directory, name, true)?,
                    None => None,
                };
                // A name the listing does not show may still exist: on Windows a short name
                // (`RUNNER~1`) opens the folder but is not in its listing.
                match found.or_else(|| Some(directory.join(name)).filter(|p| p.is_dir())) {
                    Some(next) => directory = next,
                    None => return Ok(None),
                }
            }
        }
    }
    Ok(directory.is_dir().then_some(directory))
}
