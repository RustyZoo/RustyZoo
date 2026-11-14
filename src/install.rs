//! Finding the player's game folder when neither `--data` nor `ZT_DATA_DIR` gives one: the folder
//! remembered in the user settings, then the usual install places (the CD and the Complete
//! Collection, Steam and GOG, Wine and Proton prefixes, CrossOver bottles), then on Windows the
//! registry. A folder counts only if [`Vfs::open`] accepts it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::userdir;
use crate::vfs::Vfs;

/// Where the remembered folder is kept in the user's `zoo.ini`.
const SECTION: &str = "RustyZoo";
const KEY: &str = "data";

/// The folders of an install below a `Program Files` folder.
const UNDER_PROGRAM_FILES: [&str; 3] = [
    "Microsoft Games/Zoo Tycoon",
    "Steam/steamapps/common/Zoo Tycoon",
    "GOG Galaxy/Games/Zoo Tycoon",
];

/// The registry keys whose values may name the install, read with all their subkeys.
const REGISTRY_KEYS: [&str; 4] = [
    r"HKLM\SOFTWARE\Microsoft\Microsoft Games\Zoo Tycoon",
    r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Microsoft Games\Zoo Tycoon",
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\zoo.exe",
    r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games",
];

/// Whether `dir` holds an install the engine can open.
pub fn usable(dir: &Path) -> bool {
    Vfs::open(dir).is_ok()
}

/// The game folder found on this system, if any.
pub fn find() -> Option<PathBuf> {
    search(std::env::consts::OS, |name| std::env::var_os(name))
}

/// Keeps `dir` as the folder [`find`] tries first.
pub fn remember(dir: &Path) -> Result<(), String> {
    let user = userdir::dir().ok_or("the system gives no user folder")?;
    let text = dir.to_str().ok_or("the path is not valid Unicode")?;
    userdir::save(&user, SECTION, KEY, text)
}

/// [`find`] for the operating system `os` and the environment `env`.
fn search(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let remembered = userdir::dir_of(os, &env)
        .and_then(|user| userdir::setting(&user, SECTION, KEY))
        .map(PathBuf::from);
    let registry = (os == "windows").then(registry).into_iter().flatten();
    remembered
        .into_iter()
        .chain(places(os, var))
        .chain(registry)
        .find(|dir| usable(dir))
}

/// The usual install places of `os`, in the order they are tried.
fn places(os: &str, var: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if os == "windows" {
        for name in ["ProgramFiles(x86)", "ProgramFiles"] {
            out.extend(var(name).into_iter().flat_map(program_files));
        }
        if let Some(drive) = var("SystemDrive") {
            let mut folder = drive.into_os_string();
            folder.push(r"\GOG Games\Zoo Tycoon");
            out.push(folder.into());
        }
        return out;
    }
    let Some(home) = var("HOME") else {
        return out;
    };
    let steam: &[&str] = if os == "macos" {
        &["Library/Application Support/Steam"]
    } else {
        &[".local/share/Steam", ".steam/steam"]
    };
    let mut prefixes: Vec<PathBuf> = var("WINEPREFIX").into_iter().collect();
    prefixes.push(home.join(".wine"));
    for root in steam {
        prefixes
            .extend(children(&home.join(root).join("steamapps/compatdata")).map(|p| p.join("pfx")));
    }
    if os == "macos" {
        prefixes.extend(children(
            &home.join("Library/Application Support/CrossOver/Bottles"),
        ));
    }
    for prefix in prefixes {
        let drive = prefix.join("drive_c");
        for folder in ["Program Files (x86)", "Program Files"] {
            out.extend(program_files(drive.join(folder)));
        }
        out.push(drive.join("GOG Games/Zoo Tycoon"));
    }
    out.extend(
        steam
            .iter()
            .map(|root| home.join(root).join("steamapps/common/Zoo Tycoon")),
    );
    out.push(home.join("GOG Games/Zoo Tycoon"));
    out
}

fn program_files(root: PathBuf) -> impl Iterator<Item = PathBuf> {
    UNDER_PROGRAM_FILES.iter().map(move |tail| root.join(tail))
}

/// The entries of `dir` in name order; none if it cannot be read.
fn children(dir: &Path) -> impl Iterator<Item = PathBuf> {
    let mut list: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    list.sort();
    list.into_iter()
}

/// The folders named by the values of [`REGISTRY_KEYS`], read with the system's `reg` tool.
fn registry() -> Vec<PathBuf> {
    REGISTRY_KEYS
        .iter()
        .filter_map(|key| {
            std::process::Command::new("reg")
                .args(["query", key, "/s"])
                .output()
                .ok()
        })
        .flat_map(|out| registry_paths(&String::from_utf8_lossy(&out.stdout)))
        .collect()
}

/// The folders in the text of `reg query`: each text value, or the folder of a value that names
/// a program.
fn registry_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let (_, value) = line
                .split_once("    REG_SZ    ")
                .or_else(|| line.split_once("    REG_EXPAND_SZ    "))?;
            let path = PathBuf::from(value.trim().trim_matches('"'));
            let program = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
            if program {
                path.parent().map(Path::to_path_buf)
            } else {
                Some(path)
            }
        })
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::TempDir;

    fn install(tree: &TempDir, relative: &str) -> PathBuf {
        tree.write(&format!("{relative}/zoo.ini"), b"[resource]\npath=.\n");
        tree.0.join(relative)
    }

    fn found(os: &str, tree: &TempDir, extra: &[(&str, &Path)]) -> Option<PathBuf> {
        let home = tree.0.join("home");
        let user = tree.0.join("user");
        search(os, |name| match name {
            "HOME" => Some(home.clone().into()),
            "RUSTYZOO_USER_DIR" => Some(user.clone().into()),
            _ => extra
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, p)| p.as_os_str().to_owned()),
        })
    }

    #[test]
    fn the_search_tries_the_setting_then_the_usual_places_in_order() {
        let tree = TempDir::new("install-search");
        assert_eq!(found("linux", &tree, &[]), None);
        let gog = install(&tree, "home/GOG Games/Zoo Tycoon");
        assert_eq!(found("linux", &tree, &[]), Some(gog.clone()));
        let steam = install(&tree, "home/.local/share/Steam/steamapps/common/Zoo Tycoon");
        assert_eq!(found("linux", &tree, &[]), Some(steam));
        let proton = install(
            &tree,
            "home/.steam/steam/steamapps/compatdata/123/pfx/drive_c/Program Files/Microsoft Games/Zoo Tycoon",
        );
        assert_eq!(found("linux", &tree, &[]), Some(proton));
        let wine = install(
            &tree,
            "home/.wine/drive_c/Program Files (x86)/Microsoft Games/Zoo Tycoon",
        );
        assert_eq!(found("linux", &tree, &[]), Some(wine.clone()));
        let prefix = tree.0.join("prefix");
        let own = install(&tree, "prefix/drive_c/GOG Games/Zoo Tycoon");
        assert_eq!(found("linux", &tree, &[("WINEPREFIX", &prefix)]), Some(own));

        // The remembered folder comes first; one that is no install is passed over.
        let user = tree.0.join("user");
        userdir::save(&user, SECTION, KEY, gog.to_str().unwrap()).unwrap();
        assert_eq!(found("linux", &tree, &[]), Some(gog));
        userdir::save(&user, SECTION, KEY, "").unwrap();
        assert_eq!(found("linux", &tree, &[]), Some(wine.clone()));
        let long = "x".repeat(100_000);
        userdir::save(&user, SECTION, KEY, &long).unwrap();
        assert_eq!(found("linux", &tree, &[]), Some(wine.clone()));
        for value in ["0", "-1"] {
            userdir::save(&user, SECTION, KEY, value).unwrap();
            assert_eq!(found("linux", &tree, &[]), Some(wine.clone()));
        }
    }

    #[test]
    fn windows_and_macos_have_their_own_places() {
        let tree = TempDir::new("install-places");
        let x86 = tree.0.join("pf86");
        let pf = tree.0.join("pf");
        let vars = [
            ("ProgramFiles(x86)", x86.as_path()),
            ("ProgramFiles", pf.as_path()),
        ];
        let steam = install(&tree, "pf/Steam/steamapps/common/Zoo Tycoon");
        assert_eq!(found("windows", &tree, &vars), Some(steam));
        let cd = install(&tree, "pf86/Microsoft Games/Zoo Tycoon");
        assert_eq!(found("windows", &tree, &vars), Some(cd));

        let bottle = install(
            &tree,
            "home/Library/Application Support/CrossOver/Bottles/Zoo/drive_c/Program Files/Microsoft Games/Zoo Tycoon",
        );
        assert_eq!(found("macos", &tree, &[]), Some(bottle));
        // A Linux search does not look in the bottles.
        assert_eq!(found("linux", &tree, &[]), None);
    }

    #[test]
    fn a_folder_is_usable_when_the_engine_can_open_it() {
        let tree = TempDir::new("install-usable");
        assert!(!usable(&tree.0));
        assert!(!usable(&tree.0.join("missing")));
        tree.write("trial/zoodemo.ini", b"");
        assert!(usable(&tree.0.join("trial")));
        tree.write("full/ZOO.INI", b"[resource]\npath=.\n");
        assert!(usable(&tree.0.join("full")));
        tree.write("file", b"");
        assert!(!usable(&tree.0.join("file")));
    }

    #[test]
    fn the_registry_values_name_folders() {
        let text = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Microsoft Games\\Zoo Tycoon\\1.0\r\n    \
                    InstallationDirectory    REG_SZ    C:\\Games\\Zoo Tycoon\r\n    \
                    Version    REG_DWORD    0x1\r\n    \
                    (Default)    REG_SZ    \"D:\\Zoo\\zoo.exe\"\r\n    \
                    Path    REG_EXPAND_SZ    E:\\Zoo\r\n    \
                    Empty    REG_SZ    \r\n";
        assert_eq!(
            registry_paths(text),
            ["C:\\Games\\Zoo Tycoon", "D:\\Zoo", "E:\\Zoo"].map(PathBuf::from)
        );
        assert!(registry_paths("").is_empty());
    }
}
