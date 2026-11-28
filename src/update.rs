//! Self-update of an official build: the release workflow sets [`PUBLIC_KEY`] (and may set the
//! feed address) when it builds; without a key no update code path runs, so a build of one's own
//! never checks and never updates. A Flatpak, a Linux build outside an AppImage and any other
//! system that cannot replace itself never update either.
//!
//! At the start of the launcher, when the setting `[RustyZoo] checkUpdates` is not 0 (default on),
//! [`pending`] fetches `feed.json` and `feed.sig` (a plain GET, a few seconds at most, up to
//! [`REDIRECTS`] HTTPS-only redirects: the signature, not the host, is the trust anchor), verifies
//! the detached ed25519 signature over the exact bytes of the feed, then reads it. Any failure is
//! silent. A version newer than this one and not the one kept in `[RustyZoo] skipVersion` is
//! offered; nothing else is downloaded before [`install`].
//!
//! The feed is the JSON file `feed.json` of the release, `feed.sig` the raw 64 bytes of the
//! signature, the public key 32 bytes written as 64 hex digits. `files` has one entry per system,
//! named `<os>-<arch>` as Rust writes them (`windows-x86_64`, `macos-aarch64`, `linux-x86_64`).
//! Each file is what replaces the install: the new `rustyzoo.exe` (Windows), a zip of the new
//! signed `.app` (macOS), the new AppImage (Linux).
//!
//! ```text
//! {"version":"1.2.3","files":{"linux-x86_64":{"url":"https://.../RustyZoo-x86_64.AppImage","sha256":"<64 hex>"}}}
//! ```
//!
//! The signature: `openssl pkeyutl -sign -inkey key.pem -rawin -in feed.json -out feed.sig`; the
//! key: `openssl pkey -in key.pem -pubout -outform DER | tail -c 32 | xxd -p -c 64`.
//!
//! [`install`] downloads the file named for this system into the folder of the install, refuses it
//! when its SHA-256 differs from the signed feed, then replaces the running install (Windows: the
//! `.exe` is renamed to `.old`, deleted by [`tidy`] at the next start; macOS: the `.app` folder;
//! Linux: the AppImage, `$APPIMAGE`) and starts the new one. A failure keeps the install as it was.
//!
//! The only thing the check sends is a GET with the user agent `RustyZoo/<version> (<os>; <arch>)`.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ring::{digest, signature};

use crate::mcp::json;
use crate::{crash, stats, userdir};

/// The feed's ed25519 public key (64 hex digits), from `RUSTYZOO_UPDATE_KEY` when the program is built.
pub const PUBLIC_KEY: Option<&str> = option_env!("RUSTYZOO_UPDATE_KEY");
/// The folder of `feed.json` and `feed.sig`, from `RUSTYZOO_UPDATE_URL` when the program is built:
/// by default the assets of the latest release.
const BASE: &str = match option_env!("RUSTYZOO_UPDATE_URL") {
    Some(url) => url,
    None => "https://github.com/RustyZoo/RustyZoo/releases/latest/download/",
};
/// The settings keys of the check (1 or 0) and of the version the player skipped.
pub const CHECK: &str = "checkUpdates";
const SKIPPED: &str = "skipVersion";
const SECTION: &str = "RustyZoo";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How long the start waits for the feed.
const CHECK_SECONDS: u32 = 3;
const DOWNLOAD_SECONDS: u32 = 3600;
/// A download that gets no byte for this long fails.
const STALL: Duration = Duration::from_secs(30);
const REDIRECTS: u32 = 5;
/// The most a feed, a signature or an archive may be.
const FEED_MAX: u64 = 1 << 16;
const FILE_MAX: u64 = 1 << 30;

/// What a newer version asks to download: its number, the file for this system and its SHA-256.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub version: String,
    pub url: String,
    pub sha256: [u8; 32],
}

/// Opens a URL for reading; a failure says why.
type Get<'a> = &'a dyn Fn(&str) -> Result<Box<dyn Read>, String>;

/// The HTTPS client of the updater: `seconds` in all (0: no limit), a few redirects, HTTPS only
/// unless `https_only` is off (tests).
fn http(seconds: u32, https_only: bool) -> impl Fn(&str) -> Result<Box<dyn Read>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global((seconds > 0).then(|| Duration::from_secs(seconds.into())))
        .max_redirects(REDIRECTS)
        .https_only(https_only)
        .user_agent(stats::user_agent(
            VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
        ))
        .build()
        .into();
    move |url| {
        let response = agent.get(url).call().map_err(|e| e.to_string())?;
        let body = response.into_body().into_reader();
        Ok(if seconds == DOWNLOAD_SECONDS {
            Box::new(Stalling::new(body, STALL))
        } else {
            Box::new(body)
        })
    }
}

/// A reader that fails when no byte comes for `stall` (the body is read by a thread that ends with
/// the transfer or the reader).
struct Stalling {
    chunks: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    stall: Duration,
    rest: Vec<u8>,
}

impl Stalling {
    fn new(mut from: impl Read + Send + 'static, stall: Duration) -> Self {
        let (tx, chunks) = mpsc::sync_channel(4);
        std::thread::spawn(move || {
            loop {
                let mut buf = vec![0u8; 1 << 16];
                let chunk = from.read(&mut buf).map(|n| buf.drain(..n).collect());
                let end = chunk.as_ref().map_or(true, |c: &Vec<u8>| c.is_empty());
                if tx.send(chunk).is_err() || end {
                    return;
                }
            }
        });
        Stalling {
            chunks,
            stall,
            rest: Vec::new(),
        }
    }
}

impl Read for Stalling {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.rest.is_empty() {
            self.rest = match self.chunks.recv_timeout(self.stall) {
                Ok(chunk) => chunk?,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(std::io::Error::other("the download stalled"));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            };
        }
        let n = out.len().min(self.rest.len());
        out[..n].copy_from_slice(&self.rest[..n]);
        self.rest.drain(..n);
        Ok(n)
    }
}

fn hex(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    text.is_ascii()
        .then(|| {
            text.as_bytes()
                .chunks(2)
                .map(|p| Some(digit(p[0])? << 4 | digit(*p.get(1)?)?))
                .collect()
        })
        .flatten()
}

/// The numbers of a version like `1.2.3` (a `-suffix` or `+build` is left out).
fn numbers(version: &str) -> Option<Vec<u64>> {
    let core = version.split(['-', '+']).next()?;
    core.split('.').map(|n| n.parse().ok()).collect()
}

/// Whether `latest` is a higher version than `current`; a version that is not numbers is never newer.
pub fn newer(latest: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (numbers(latest), numbers(current)) else {
        return false;
    };
    let at = |v: &[u64], i| v.get(i).copied().unwrap_or(0);
    (0..a.len().max(b.len()))
        .map(|i| at(&a, i).cmp(&at(&b, i)))
        .find(|o| o.is_ne())
        .is_some_and(|o| o.is_gt())
}

/// Whether `sig` is the signature of `feed` by the 32-byte `key`.
fn verified(key: &[u8], feed: &[u8], sig: &[u8]) -> bool {
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(feed, sig)
        .is_ok()
}

/// The offer of a verified feed for the system `platform`, or `None` when it is malformed.
fn parse(feed: &[u8], platform: &str) -> Option<Offer> {
    let doc = json::parse(std::str::from_utf8(feed).ok()?).ok()?;
    let file = doc.get("files")?.get(platform)?;
    let url = file.get("url")?.as_str()?;
    Some(Offer {
        version: doc.get("version")?.as_str()?.into(),
        url: url.strip_prefix("https://").map(|_| url.into())?,
        sha256: hex(file.get("sha256")?.as_str()?)?.try_into().ok()?,
    })
}

/// Reads at most `max` bytes; more is an error.
fn read_up_to(mut from: impl Read, max: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    (&mut from)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err("too long".into());
    }
    Ok(bytes)
}

/// The offer when the feed at `base` is signed by `key` (64 hex digits; `None` or any other key: no check at all),
/// has a version newer than `current` and not `skipped`, and a file for `platform`.
fn find(
    key: Option<&str>,
    get: Get,
    base: &str,
    platform: &str,
    (current, skipped): (&str, Option<&str>),
) -> Option<Offer> {
    let key = hex(key?).filter(|k| k.len() == 32)?;
    let base = base.trim_end_matches('/');
    let feed = read_up_to(get(&format!("{base}/feed.json")).ok()?, FEED_MAX).ok()?;
    let sig = read_up_to(get(&format!("{base}/feed.sig")).ok()?, FEED_MAX).ok()?;
    if !verified(&key, &feed, &sig) {
        return None;
    }
    let offer = parse(&feed, platform)?;
    (newer(&offer.version, current) && skipped != Some(&offer.version)).then_some(offer)
}

/// Where this install is replaced: the `.exe` on Windows, the `.app` on macOS, the AppImage on
/// Linux unless it is a Flatpak; `None` for anything that does not update itself.
fn target_of(
    os: &str,
    exe: Option<PathBuf>,
    appimage: Option<OsString>,
    flatpak: bool,
) -> Option<PathBuf> {
    match os {
        "windows" => exe,
        "macos" => exe?
            .ancestors()
            .find(|p| p.extension() == Some(OsStr::new("app")))
            .map(Path::to_path_buf),
        "linux" => appimage
            .filter(|a| !a.is_empty() && !flatpak)
            .map(PathBuf::from),
        _ => None,
    }
}

fn target() -> Option<PathBuf> {
    let flatpak = std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists();
    let (exe, appimage) = (std::env::current_exe().ok(), std::env::var_os("APPIMAGE"));
    target_of(std::env::consts::OS, exe, appimage, flatpak)
}

fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The build key when it is 32 bytes written as 64 hex digits; an empty or other key is none.
fn key() -> Option<&'static str> {
    PUBLIC_KEY.filter(|k| hex(k).is_some_and(|b| b.len() == 32))
}

/// Whether this build checks for updates and can replace itself: it has a key and a place to put
/// the new version. The launcher then shows the "Check for updates" box.
pub fn asked() -> bool {
    key().is_some() && target().is_some()
}

/// Whether the player keeps the check on (the default) in the settings folder `dir`.
pub fn enabled(dir: Option<&Path>) -> bool {
    dir.and_then(|d| crash::consent(d, CHECK)).unwrap_or(true)
}

pub fn set_enabled(dir: &Path, on: bool) -> Result<(), String> {
    crash::keep(dir, CHECK, on)
}

/// Keeps `version` as the one not to offer again.
pub fn skip(dir: &Path, version: &str) -> Result<(), String> {
    userdir::save(dir, SECTION, SKIPPED, version)
}

/// The newer version this start offers, if any: only an official build that can replace itself,
/// with the check on. Silent on any failure, and never longer than a few seconds.
pub fn pending(dir: Option<&Path>) -> Option<Offer> {
    key()?;
    target()?;
    if !enabled(dir) {
        return None;
    }
    let skipped = dir.and_then(|d| userdir::setting(d, SECTION, SKIPPED));
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("update".into())
        .spawn(move || {
            let get = http(CHECK_SECONDS, true);
            let found = find(
                key(),
                &get,
                BASE,
                &platform(),
                (VERSION, skipped.as_deref()),
            );
            let _ = tx.send(found);
        })
        .ok()?;
    let wait = Duration::from_secs(CHECK_SECONDS.into()) + Duration::from_millis(500);
    rx.recv_timeout(wait).ok().flatten()
}

/// `path` with `suffix` after its name.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn aside(path: &Path) -> PathBuf {
    beside(path, ".old")
}

fn remove(path: &Path) {
    let _ = fs::remove_file(path).or_else(|_| fs::remove_dir_all(path));
}

/// Deletes the old `.exe` that the last update renamed aside (Windows cannot delete a running one).
pub fn tidy() {
    if asked()
        && std::env::consts::OS == "windows"
        && let Ok(exe) = std::env::current_exe()
    {
        remove(&aside(&exe));
    }
}

/// Streams `url` to `dest` and keeps it only when its SHA-256 is `want`.
fn download(get: Get, url: &str, want: &[u8; 32], dest: &Path) -> Result<(), String> {
    let run = || -> Result<(), String> {
        let mut from = get(url)?.take(FILE_MAX + 1);
        let mut to = File::create(dest).map_err(|e| e.to_string())?;
        let mut sum = digest::Context::new(&digest::SHA256);
        let (mut buf, mut total) = (vec![0u8; 1 << 16], 0u64);
        loop {
            let n = from.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            sum.update(&buf[..n]);
            to.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            total += n as u64;
        }
        if total > FILE_MAX {
            return Err("the download is too long".into());
        }
        to.flush().map_err(|e| e.to_string())?;
        if sum.finish().as_ref() != want {
            return Err("the download does not match the signed feed".into());
        }
        Ok(())
    };
    let result = run();
    if result.is_err() {
        remove(dest);
    }
    result
}

/// Puts `new` in the place of `target` (a file or a folder on the same disk); the old one is kept
/// beside it as `.old` when `keep_old`, and put back when `new` cannot be moved.
fn swap(target: &Path, new: &Path, keep_old: bool) -> Result<(), String> {
    let old = aside(target);
    remove(&old);
    fs::rename(target, &old).map_err(|e| format!("{}: {e}", target.display()))?;
    if let Err(e) = fs::rename(new, target) {
        let _ = fs::rename(&old, target);
        return Err(format!("{}: {e}", target.display()));
    }
    if !keep_old {
        remove(&old);
    }
    Ok(())
}

/// Unzips the `.app` of the archive `zip` into the folder `dir` (macOS: `ditto` keeps the
/// signature, the links and the modes) and gives its path.
fn unpack(zip: &Path, dir: &Path) -> Result<PathBuf, String> {
    remove(dir);
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let status = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(zip)
        .arg(dir)
        .status()
        .map_err(|e| format!("ditto: {e}"))?;
    if !status.success() {
        return Err("the archive does not unzip".into());
    }
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let app = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension() == Some(OsStr::new("app")))
        .ok_or("the archive holds no application")?;
    let signed = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .status()
        .map_err(|e| format!("codesign: {e}"))?;
    if !signed.success() {
        return Err("the new application is not correctly signed".into());
    }
    Ok(app)
}

/// Why [`install`] failed.
#[derive(Debug)]
pub enum Failed {
    /// The current install is untouched.
    Untouched(String),
    /// The new version is in place but did not start.
    NotStarted(String),
}

/// Downloads, checks and puts in place the version of `offer`, then starts it. On a failure
/// before the swap the current install is untouched.
pub fn install(offer: &Offer) -> Result<(), Failed> {
    let target =
        target().ok_or_else(|| Failed::Untouched("this install does not update itself".into()))?;
    let os = std::env::consts::OS;
    let (part, unpacked) = (beside(&target, ".new"), beside(&target, ".unpacked"));
    let get = http(DOWNLOAD_SECONDS, true);
    let swapped = download(&get, &offer.url, &offer.sha256, &part)
        .and_then(|()| {
            if os == "macos" {
                unpack(&part, &unpacked)
            } else {
                make_runnable(&part).map(|()| part.clone())
            }
        })
        .and_then(|new| swap(&target, &new, os == "windows"));
    remove(&part);
    remove(&unpacked);
    swapped.map_err(Failed::Untouched)?;
    let mut start = if os == "macos" {
        let mut open = Command::new("/usr/bin/open");
        open.arg("-n").arg(&target);
        open
    } else {
        Command::new(&target)
    };
    start
        .spawn()
        .map(drop)
        .map_err(|e| Failed::NotStarted(e.to_string()))
}

#[cfg(unix)]
fn make_runnable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn make_runnable(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Cursor;

    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;
    use crate::fixtures::TempDir;

    const BASE: &str = "https://example.org/release";

    fn pair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn sha(bytes: &[u8]) -> [u8; 32] {
        digest::digest(&digest::SHA256, bytes)
            .as_ref()
            .try_into()
            .unwrap()
    }

    fn feed(version: &str, archive: &[u8]) -> String {
        format!(
            r#"{{"version":"{version}","files":{{"linux-x86_64":{{"url":"https://example.org/a.AppImage","sha256":"{}"}}}}}}"#,
            to_hex(&sha(archive))
        )
    }

    /// The answers of a server that holds `files`.
    fn server(files: Vec<(String, Vec<u8>)>) -> impl Fn(&str) -> Result<Box<dyn Read>, String> {
        let files: HashMap<_, _> = files.into_iter().collect();
        move |url| match files.get(url) {
            Some(bytes) => Ok(Box::new(Cursor::new(bytes.clone()))),
            None => Err(format!("{url}: 404")),
        }
    }

    /// A release signed by `signer` whose feed text is `feed`.
    fn release(
        signer: &Ed25519KeyPair,
        feed: &str,
    ) -> impl Fn(&str) -> Result<Box<dyn Read>, String> {
        server(vec![
            (format!("{BASE}/feed.json"), feed.as_bytes().to_vec()),
            (
                format!("{BASE}/feed.sig"),
                signer.sign(feed.as_bytes()).as_ref().to_vec(),
            ),
        ])
    }

    fn look(key: Option<&str>, get: Get, skipped: Option<&str>) -> Option<Offer> {
        find(key, get, BASE, "linux-x86_64", ("1.2.0", skipped))
    }

    #[test]
    fn a_signed_newer_feed_is_offered() {
        let pair = pair();
        let key = to_hex(pair.public_key().as_ref());
        let text = feed("1.3.0", b"archive");
        let offer = look(Some(&key), &release(&pair, &text), None).unwrap();
        assert_eq!(offer.version, "1.3.0");
        assert_eq!(offer.url, "https://example.org/a.AppImage");
        assert_eq!(offer.sha256, sha(b"archive"));
        // Another system has no file; the same version is not newer.
        assert!(
            find(
                Some(&key),
                &release(&pair, &text),
                BASE,
                "macos-aarch64",
                ("1.2.0", None)
            )
            .is_none()
        );
        assert!(
            find(
                Some(&key),
                &release(&pair, &text),
                BASE,
                "linux-x86_64",
                ("1.3.0", None)
            )
            .is_none()
        );
    }

    #[test]
    fn a_tampered_feed_or_a_bad_signature_is_refused() {
        let (pair, other) = (pair(), pair());
        let key = to_hex(pair.public_key().as_ref());
        let text = feed("1.3.0", b"archive");
        let sig = pair.sign(text.as_bytes()).as_ref().to_vec();
        let at = |feed: &str, sig: Vec<u8>| {
            server(vec![
                (format!("{BASE}/feed.json"), feed.as_bytes().to_vec()),
                (format!("{BASE}/feed.sig"), sig),
            ])
        };
        let tampered = text.replace("a.AppImage", "b.AppImage");
        assert!(look(Some(&key), &at(&tampered, sig.clone()), None).is_none());
        assert!(look(Some(&key), &at(&text, sig.clone()), None).is_some());
        // Signed by another key, cut short, empty, or too long.
        assert!(look(Some(&key), &release(&other, &text), None).is_none());
        assert!(look(Some(&key), &at(&text, sig[..63].to_vec()), None).is_none());
        assert!(look(Some(&key), &at(&text, Vec::new()), None).is_none());
        assert!(look(Some(&key), &at(&text, vec![0; 1 << 17]), None).is_none());
        // A signed feed that is not a feed, a file without a hash, a key that is not a key.
        for bad in [
            "[]",
            "{",
            r#"{"version":"2","files":{"linux-x86_64":{"url":"https://x/y","sha256":"00"}}}"#,
            r#"{"version":"2","files":{"linux-x86_64":{"url":"http://x/y","sha256":""}}}"#,
        ] {
            assert!(
                look(Some(&key), &release(&pair, bad), None).is_none(),
                "{bad}"
            );
        }
        for key in ["", "zz", "00", &key[..62]] {
            assert!(
                look(Some(key), &release(&pair, &text), None).is_none(),
                "{key}"
            );
        }
        // A server that fails.
        assert!(look(Some(&key), &server(Vec::new()), None).is_none());
    }

    #[test]
    fn a_build_without_a_key_never_asks_the_server() {
        let get = |_: &str| -> Result<Box<dyn Read>, String> { panic!("no key, no request") };
        assert!(look(None, &get, None).is_none());
    }

    #[test]
    fn an_empty_or_short_key_never_asks_the_server() {
        let get = |_: &str| -> Result<Box<dyn Read>, String> { panic!("bad key, no request") };
        for key in ["", "00", &"0".repeat(62), &"0".repeat(66)] {
            assert!(look(Some(key), &get, None).is_none());
        }
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        for (a, b) in [
            ("1.10.0", "1.9.9"),
            ("2", "1.99"),
            ("1.0.1", "1"),
            ("0.1.0", "0.0.0"),
        ] {
            assert!(newer(a, b) && !newer(b, a), "{a} {b}");
        }
        assert!(!newer("1.2.0", "1.2"));
        assert!(!newer("1.2.0-beta", "1.2.0"));
        for bad in ["", ".", "1..2", "x", "1.x", "-1", "99999999999999999999999"] {
            assert!(!newer(bad, "0.0.0") && !newer("9.9.9", bad), "{bad}");
        }
    }

    #[test]
    fn a_skipped_version_is_not_offered_again_but_a_later_one_is() {
        let pair = pair();
        let key = to_hex(pair.public_key().as_ref());
        let text = feed("1.3.0", b"a");
        let get = release(&pair, &text);
        assert!(look(Some(&key), &get, Some("1.3.0")).is_none());
        assert!(look(Some(&key), &get, Some("1.2.9")).is_some());
        let dir = TempDir::new("update-skip");
        assert!(enabled(Some(dir.0.as_path())) && enabled(None));
        set_enabled(dir.0.as_path(), false).unwrap();
        assert!(!enabled(Some(dir.0.as_path())));
        skip(dir.0.as_path(), "1.3.0").unwrap();
        assert_eq!(
            userdir::setting(dir.0.as_path(), SECTION, SKIPPED).as_deref(),
            Some("1.3.0")
        );
        set_enabled(dir.0.as_path(), true).unwrap();
        assert!(enabled(Some(dir.0.as_path())));
    }

    #[test]
    fn an_archive_with_the_wrong_hash_is_refused_and_removed() {
        let dir = TempDir::new("update-hash");
        let dest = dir.0.as_path().join("new");
        let get = server(vec![("https://x/a".into(), b"archive".to_vec())]);
        download(&get, "https://x/a", &sha(b"other"), &dest).unwrap_err();
        assert!(!dest.exists());
        download(&get, "https://x/b", &sha(b"archive"), &dest).unwrap_err();
        assert!(!dest.exists());
        download(&get, "https://x/a", &sha(b"archive"), &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"archive");
    }

    #[test]
    fn a_stalled_download_fails() {
        struct Stuck(bool);
        impl Read for Stuck {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if std::mem::replace(&mut self.0, true) {
                    std::thread::sleep(Duration::from_secs(2));
                    return Ok(0);
                }
                out[..2].copy_from_slice(b"hi");
                Ok(2)
            }
        }
        let mut from = Stalling::new(Stuck(false), Duration::from_millis(200));
        let mut two = [0u8; 2];
        from.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"hi");
        let e = from.read(&mut two).unwrap_err();
        assert_eq!(e.to_string(), "the download stalled");
        let mut all = String::new();
        Stalling::new(Cursor::new(b"abc".to_vec()), STALL)
            .read_to_string(&mut all)
            .unwrap();
        assert_eq!(all, "abc");
    }

    #[test]
    fn the_new_version_replaces_the_old_and_a_failure_keeps_it() {
        let dir = TempDir::new("update-swap");
        let (target, new) = (
            dir.0.as_path().join("rustyzoo"),
            dir.0.as_path().join("rustyzoo.new"),
        );
        fs::write(&target, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        swap(&target, &new, true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(aside(&target)).unwrap(), b"old");
        // Windows deletes the old one at the next start.
        remove(&aside(&target));
        fs::write(&new, b"newer").unwrap();
        swap(&target, &new, false).unwrap();
        assert!(!aside(&target).exists() && fs::read(&target).unwrap() == b"newer");
        // The new one is missing: the install stays as it was.
        swap(&target, &dir.0.as_path().join("missing"), false).unwrap_err();
        assert_eq!(fs::read(&target).unwrap(), b"newer");
        // A folder (the macOS bundle) is swapped the same way.
        let (app, fresh) = (dir.0.as_path().join("A.app"), dir.0.as_path().join("B.app"));
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join("x"), b"1").unwrap();
        swap(&app, &fresh, false).unwrap();
        assert!(app.join("x").exists() && !aside(&app).exists());
    }

    #[test]
    fn only_an_install_that_can_replace_itself_updates() {
        let exe = || {
            Some(PathBuf::from(
                "/Applications/RustyZoo.app/Contents/MacOS/rustyzoo",
            ))
        };
        let image = || Some(OsString::from("/home/a/RustyZoo.AppImage"));
        assert_eq!(
            target_of("windows", Some("C:/z/rustyzoo.exe".into()), None, false),
            Some("C:/z/rustyzoo.exe".into())
        );
        assert_eq!(
            target_of("macos", exe(), None, false),
            Some("/Applications/RustyZoo.app".into())
        );
        assert_eq!(
            target_of("macos", Some("/usr/bin/rustyzoo".into()), None, false),
            None
        );
        assert_eq!(
            target_of("linux", None, image(), false),
            Some("/home/a/RustyZoo.AppImage".into())
        );
        assert_eq!(target_of("linux", None, image(), true), None);
        assert_eq!(target_of("linux", None, None, false), None);
        assert_eq!(target_of("linux", None, Some("".into()), false), None);
        assert_eq!(target_of("freebsd", exe(), image(), false), None);
    }

    #[test]
    fn this_build_has_no_key_and_no_update_path() {
        if PUBLIC_KEY.is_none() {
            assert!(!asked() && pending(None).is_none());
        }
    }

    /// A plain-HTTP server that answers each connection with the next reply.
    fn serve(replies: Vec<String>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn a_few_redirects_are_followed_and_https_only_refuses_http() {
        let head = |status: &str, extra: &str| {
            format!("HTTP/1.1 {status}\r\n{extra}Connection: close\r\n\r\n")
        };
        let moved = |to: &str| {
            head(
                "302 Found",
                &format!("Location: {to}\r\nContent-Length: 0\r\n"),
            )
        };
        let ok = head("200 OK", "Content-Length: 2\r\n") + "hi";
        let end = serve(vec![ok]);
        let url = serve(vec![moved(&format!("{end}/b"))]);
        let mut text = String::new();
        http(5, false)(&url)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "hi");
        assert!(http(5, true)(&url).is_err());
        // A loop of redirects ends.
        let looping = serve((0..9).map(|_| moved("/a")).collect());
        assert!(http(5, false)(&looping).is_err());
    }
}
