//! Opt-in crash reports: when a panic ends an interactive game, offer to send one Sentry envelope to
//! the project's server. Nothing is asked or sent without a server address in the build, with
//! `RUSTYZOO_NO_CRASH_REPORTS` set, in a headless run, a test or any other command, nor for a panic
//! that is caught.
//!
//! The address (DSN) is [`DSN`], set when the program is built. The answer is asked once, kept in
//! `[RustyZoo] crashReports` of the settings file (1 send, 0 do not, absent or unreadable: ask) and
//! changed by editing it. The report holds the panic message, the backtrace, the version, the
//! build, the system and the last [`LOG_LINES`] warnings, with every path left out: the known
//! folders of the player, then any other absolute path down to the name of its `.rs` file.
//!
//! The envelope goes out through the system's `curl`: the program links no HTTP or TLS client, and
//! `curl` ships with Linux, macOS and Windows 10. A missing `curl` or a failed request is a warning.
//! The start statistics of [`crate::stats`] share the question, the answer in the settings, `curl`
//! and the variable that turns everything off.

use std::collections::VecDeque;
use std::fmt;
use std::hash::{BuildHasher, RandomState};
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mcp::json::Json;
use crate::userdir;

/// The server address of this build, from `RUSTYZOO_CRASH_DSN` when the program is built.
pub const DSN: Option<&str> = option_env!("RUSTYZOO_CRASH_DSN");

/// How many of the last warnings a report holds.
const LOG_LINES: usize = 20;

const SECTION: &str = "RustyZoo";
const KEY: &str = "crashReports";

static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Prints a warning on the standard error, like `eprintln!`, and keeps it for the crash report.
#[macro_export]
macro_rules! logln {
    ($($t:tt)*) => { $crate::crash::log(format_args!($($t)*)) };
}

#[doc(hidden)]
pub fn log(line: fmt::Arguments) {
    eprintln!("{line}");
    if let Ok(mut log) = LOG.lock() {
        if log.len() == LOG_LINES {
            log.pop_front();
        }
        log.push_back(line.to_string());
    }
}

/// A parsed server address: the project key and the URL events go to.
#[derive(Debug, PartialEq)]
pub struct Dsn {
    key: String,
    url: String,
}

impl Dsn {
    /// `scheme://key@host[:port][/path]/project`, or `None` for anything else.
    pub fn parse(text: &str) -> Option<Dsn> {
        let (scheme, rest) = text.split_once("://")?;
        let (key, rest) = rest.split_once('@')?;
        let (host, path) = rest.split_once('/')?;
        let (base, project) = path
            .trim_end_matches('/')
            .rsplit_once('/')
            .unwrap_or(("", path));
        let key = key.split(':').next().unwrap_or_default();
        let bare = |s: &str| !s.is_empty() && !s.contains(char::is_whitespace);
        let ok = matches!(scheme, "http" | "https") && bare(key) && bare(host) && bare(project);
        let base = if base.is_empty() {
            String::new()
        } else {
            format!("/{base}")
        };
        ok.then(|| Dsn {
            key: key.into(),
            url: format!("{scheme}://{host}{base}/api/{project}/envelope/"),
        })
    }
}

/// `debug` or `release`: the build as the reports name it.
pub fn build() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// What a crash report says, before the paths are left out.
pub struct Report {
    pub message: String,
    pub backtrace: String,
    pub log: Vec<String>,
}

/// The answer kept in the settings: `Some` to send or not to send, `None` to ask.
fn consent(dir: &Path, key: &str) -> Option<bool> {
    let value = userdir::setting(dir, SECTION, key)?;
    crate::config::to_flag(value.as_bytes())
}

/// The folders whose names a report must not carry, the longest first.
pub fn private_roots(install: &Path) -> Vec<String> {
    let var = |name| std::env::var_os(name).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(var)
        .collect();
    roots.extend(userdir::dir());
    roots.extend(userdir::saved_games());
    roots.extend(std::env::current_dir().ok());
    roots.extend(
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    );
    roots.push(install.to_path_buf());
    let mut roots: Vec<String> = roots
        .iter()
        .map(|p| {
            p.to_string_lossy()
                .trim_end_matches(['/', '\\'])
                .to_string()
        })
        .filter(|p| p.len() > 1)
        .collect();
    roots.sort_by_key(|p| std::cmp::Reverse(p.len()));
    roots
}

/// Marks where a known folder stood, so that it is cut even in the middle of a word.
const ROOT: char = '\u{1}';

/// `text` without the `roots` and without any other absolute path: a path that ends in a `.rs`
/// file keeps that name (`<path>/app.rs:12:5`), any other becomes `<path>`. A path (a known folder,
/// `/x`, `\\x`, `C:\x`) may hold spaces, so it runs to `: `, a quote, a bracket, a comma or the end
/// of the line.
pub fn strip_paths(text: &str, roots: &[String]) -> String {
    let mut text = text.replace(ROOT, "");
    for root in roots {
        text = text.replace(root.as_str(), &ROOT.to_string());
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < chars.len() {
        let before = i.checked_sub(1).map(|j| chars[j]);
        let starts = before.is_none_or(|c| c.is_whitespace() || "\"'`(=[<,".contains(c));
        let separator = |j: usize| matches!(chars.get(j), Some('/' | '\\'));
        let drive =
            chars[i].is_ascii_alphabetic() && chars.get(i + 1) == Some(&':') && separator(i + 2);
        let rooted = separator(i) && chars.get(i + 1).is_some_and(|c| !c.is_whitespace());
        if chars[i] == ROOT || (starts && (rooted || drive)) {
            let end = (i + 1..chars.len())
                .find(|&j| {
                    let c = chars[j];
                    let colon = c == ':' && chars.get(j + 1).is_none_or(|n| n.is_whitespace());
                    colon || c == '\n' || "\"'`),;>]}".contains(c)
                })
                .unwrap_or(chars.len());
            let path: String = chars[i..end].iter().collect();
            let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
            match name.find(".rs") {
                Some(at) if at > 0 && !name.contains(' ') => {
                    out.push_str(&format!("<path>/{name}"))
                }
                _ => out.push_str("<path>"),
            }
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The envelope of `report`, with the paths of `roots` left out.
pub fn envelope(report: &Report, roots: &[String], event_id: &str, now: f64) -> String {
    let clean = |s: &str| Json::text(strip_paths(s, roots));
    let message = clean(&report.message);
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let build = build();
    let event = Json::obj([
        ("event_id", Json::text(event_id)),
        ("timestamp", Json::Num(now)),
        ("platform", Json::text("native")),
        ("level", Json::text("fatal")),
        (
            "release",
            Json::text(format!("rustyzoo@{}", env!("CARGO_PKG_VERSION"))),
        ),
        ("environment", Json::text(build)),
        ("message", message.clone()),
        (
            "exception",
            Json::obj([(
                "values",
                Json::Arr(vec![Json::obj([
                    ("type", Json::text("panic")),
                    ("value", message),
                    (
                        "mechanism",
                        Json::obj([
                            ("type", Json::text("panic")),
                            ("handled", Json::Bool(false)),
                        ]),
                    ),
                ])]),
            )]),
        ),
        (
            "tags",
            Json::obj([
                ("os", Json::text(os)),
                ("arch", Json::text(arch)),
                ("build", Json::text(build)),
            ]),
        ),
        (
            "extra",
            Json::obj([
                ("backtrace", clean(&report.backtrace)),
                (
                    "log",
                    Json::Arr(report.log.iter().map(|l| clean(l)).collect()),
                ),
            ]),
        ),
    ]);
    let header = Json::obj([("event_id", Json::text(event_id))]);
    format!(
        "{header}\n{}\n{event}\n",
        Json::obj([("type", Json::text("event"))])
    )
}

fn event_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let half = |salt: u128| RandomState::new().hash_one(now ^ salt);
    format!("{:016x}{:016x}", half(0), half(1))
}

/// Posts `body` to `url` with `curl`, with these header lines, waiting at most `seconds`.
pub fn post(url: &str, headers: &[String], seconds: u32, body: &str) -> Result<(), String> {
    let mut command = Command::new("curl");
    command
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--proto",
            "=http,https",
        ])
        .args(["--max-time", &seconds.to_string()]);
    for header in headers {
        command.args(["--header", header]);
    }
    command
        .args([
            "--data-binary",
            "@-",
            "--output",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ])
        .arg(url)
        .stdin(Stdio::piped());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
    let mut child = command.spawn().map_err(|e| format!("curl: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("curl: no input")?;
    stdin
        .write_all(body.as_bytes())
        .map_err(|e| format!("curl: {e}"))?;
    drop(stdin);
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("curl: {status}")),
        Err(e) => Err(format!("curl: {e}")),
    }
}

/// Posts a crash envelope to the server of `dsn`.
fn post_envelope(dsn: &Dsn, body: &str) -> Result<(), String> {
    let headers = [
        "Content-Type: application/x-sentry-envelope".to_string(),
        format!(
            "X-Sentry-Auth: Sentry sentry_version=7, sentry_key={}, sentry_client=rustyzoo/{}",
            dsn.key,
            env!("CARGO_PKG_VERSION")
        ),
    ];
    post(&dsn.url, &headers, 10, body)
}

/// What to do with a report: nothing without a server address, send when the setting `key` says so,
/// else ask, keep an explicit answer and send on Yes. `ask` gives `None` when it could not ask;
/// `what` names the report in the warning of a failed send.
pub fn offer<T>(
    server: Option<&T>,
    dir: Option<&Path>,
    (key, what): (&str, &str),
    ask: impl FnOnce() -> Option<bool>,
    send: impl FnOnce(&T) -> Result<(), String>,
) {
    let Some(server) = server else { return };
    let answer = match dir.and_then(|dir| consent(dir, key)) {
        Some(answer) => answer,
        None => match ask() {
            Some(answer) => {
                if let Some(dir) = dir
                    && let Err(e) = userdir::save(dir, SECTION, key, if answer { "1" } else { "0" })
                {
                    log(format_args!("rustyzoo: warning: {e}"));
                }
                answer
            }
            None => false,
        },
    };
    if answer && let Err(e) = send(server) {
        log(format_args!("rustyzoo: warning: {what} not sent: {e}"));
    }
}

/// What `install` armed: the server, the settings folder, the folders to leave out and the question.
struct Armed {
    dsn: Dsn,
    dir: Option<PathBuf>,
    roots: Vec<String>,
    ask: fn(&str) -> Option<bool>,
}

static ARMED: Mutex<Option<Armed>> = Mutex::new(None);
/// The report of the last panic: sent only when that panic ends the game (see [`guard`]).
static PENDING: Mutex<Option<Report>> = Mutex::new(None);

/// The environment variable that turns every report and statistic off, whatever it holds but nothing.
const OFF: &str = "RUSTYZOO_NO_CRASH_REPORTS";

fn is_off(value: Option<std::ffi::OsString>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// Whether the environment turns every report and statistic off.
pub fn disabled() -> bool {
    is_off(std::env::var_os(OFF))
}

/// Where the privacy policy, which the questions point to, is.
pub const POLICY: &str = "https://rustyzoo.com/privacy/";

/// Arms the panic hook of an interactive game: the hook only records the report; [`guard`] asks
/// and sends once the panic ends the game. `ask` shows the question and gives the answer (`None` if
/// it could not be asked). Without a server address in the build, or with `RUSTYZOO_NO_CRASH_REPORTS`
/// set, it does nothing and gives false.
pub fn install(
    dsn: Option<&str>,
    dir: Option<PathBuf>,
    install: &Path,
    ask: fn(&str) -> Option<bool>,
) -> bool {
    let Some(dsn) = dsn.and_then(Dsn::parse).filter(|_| !disabled()) else {
        return false;
    };
    let roots = private_roots(install);
    if let Ok(mut armed) = ARMED.lock() {
        *armed = Some(Armed {
            dsn,
            dir,
            roots,
            ask,
        });
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info: &PanicHookInfo| {
        previous(info);
        // A panic that is caught, or that ends another thread, only leaves its report behind.
        let report = Report {
            message: info.to_string(),
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
            log: LOG
                .try_lock()
                .map(|log| log.iter().cloned().collect())
                .unwrap_or_default(),
        };
        if let Ok(mut pending) = PENDING.try_lock() {
            *pending = Some(report);
        }
    }));
    true
}

/// Runs `play`; when a panic ends it, asks about the report of that panic (the window of `play` is
/// dropped by then), sends it on Yes, and goes on with the panic.
pub fn guard<T>(play: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(play)) {
        Ok(value) => value,
        Err(panic) => {
            report_pending();
            std::panic::resume_unwind(panic)
        }
    }
}

/// The question shown once, at the first crash.
fn question() -> String {
    format!(
        "RustyZoo stopped because of an error. Send a crash report? It holds the error, the \
        version, your system and the last warnings, never your files, saved games or folder \
        names. Your answer is kept in zoo.ini ([RustyZoo] {KEY}). Policy: {POLICY}"
    )
}

fn report_pending() {
    let (Ok(armed), Ok(mut pending)) = (ARMED.lock(), PENDING.lock()) else {
        return;
    };
    let (Some(armed), Some(report)) = (armed.as_ref(), pending.take()) else {
        return;
    };
    let question = question();
    offer(
        Some(&armed.dsn),
        armed.dir.as_deref(),
        (KEY, "crash report"),
        || (armed.ask)(&question),
        |dsn| post_envelope(dsn, &envelope(&report, &armed.roots, &event_id(), now())),
    );
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::fixtures::TempDir;
    use crate::mcp::json;

    fn dsn() -> Dsn {
        Dsn::parse("https://abc@errors.example.org/7").unwrap()
    }

    #[test]
    fn a_dsn_gives_the_envelope_url_and_the_key() {
        assert_eq!(dsn().key, "abc");
        assert_eq!(dsn().url, "https://errors.example.org/api/7/envelope/");
        let p = Dsn::parse("http://k:secret@localhost:8000/sub/path/12/").unwrap();
        assert_eq!(p.key, "k");
        assert_eq!(p.url, "http://localhost:8000/sub/path/api/12/envelope/");
        for bad in [
            "",
            "abc",
            "ftp://k@h/1",
            "https://@h/1",
            "https://k@h",
            "https://k@h/",
            "https://k@/1",
        ] {
            assert_eq!(Dsn::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn paths_of_the_player_are_left_out_of_the_text() {
        let roots = vec![
            "/u/bob/My Games/Zoo".to_string(),
            "C:\\Users\\Bob".to_string(),
        ];
        let text = "No such file: /u/bob/My Games/Zoo/Saved Games/a.zoo, \"/var/x/y.dat\" at \
            C:\\Users\\Bob\\AppData\\z.ini, /rustc/abc123/library/std/src/panic.rs:12:5\n(/u/bob/.cargo/src/app.rs:3) ok";
        let clean = strip_paths(text, &roots);
        for secret in [
            "bob", "Bob", "My Games", "var", "x/y", "a.zoo", "abc123", ".cargo",
        ] {
            assert!(!clean.contains(secret), "{secret} in {clean}");
        }
        assert!(clean.contains("<path>/panic.rs:12:5"), "{clean}");
        assert!(clean.contains("(<path>/app.rs:3)"), "{clean}");
        assert!(clean.ends_with(" ok"));
        assert_eq!(
            strip_paths("a/b and 1/2 and src/app.rs:3", &[]),
            "a/b and 1/2 and src/app.rs:3"
        );
        assert_eq!(strip_paths("", &[]), "");
    }

    #[test]
    fn a_path_with_spaces_is_left_out_whole() {
        let roots = vec!["/data/bob".to_string(), "C:\\Users\\Bob Smith".to_string()];
        for (text, secrets) in [
            (
                "/data/bob/Saved Games/Bob Smith Zoo.zoo: bad",
                &["Smith", "Zoo.zoo"][..],
            ),
            (
                "cannot read C:\\Games\\Ann Lee\\a b.zoo: bad",
                &["Lee", "b.zoo"],
            ),
            (
                "c:\\users\\bob smith\\Saved Games\\x.zoo: bad",
                &["smith", "x.zoo"],
            ),
            ("open \\\\nas\\Ann Lee\\z.zoo: bad", &["Lee", "z.zoo"]),
            ("/Users/ann/Library/My Zoo.zoo", &["ann", "Zoo.zoo"]),
            (
                "a \"/x/My Zoo.zoo\" b, /y/Ann Lee.zoo, c (/z/A B.zoo) d",
                &["Zoo", "Lee", "B.zoo"],
            ),
        ] {
            let clean = strip_paths(text, &roots);
            for secret in secrets {
                assert!(!clean.contains(secret), "{secret} in {clean}");
            }
        }
        assert_eq!(strip_paths("/data/bob/a b.zoo: bad", &roots), "<path>: bad");
        assert_eq!(
            strip_paths("a lone / and \\ stay", &[]),
            "a lone / and \\ stay"
        );
    }

    #[test]
    fn the_variable_turns_reports_off() {
        use std::ffi::OsString;
        assert!(!is_off(None));
        assert!(!is_off(Some(OsString::new())));
        for on in ["1", "0", "-1", "99999999999999999999", "x"] {
            assert!(is_off(Some(on.into())), "{on}");
        }
    }

    #[test]
    fn the_envelope_has_the_report_and_no_path() {
        let roots = vec!["/u/bob".to_string()];
        let report = Report {
            message: "panicked at /u/bob/RustyZoo/src/app.rs:1:2:\nbad \"zoo\" in /u/bob/Saved Games/z.zoo".into(),
            backtrace: "   0: rustyzoo::app::x\n             at /u/bob/RustyZoo/src/app.rs:1:2".into(),
            log: vec!["rustyzoo: warning: /u/bob/Saved Games/z.zoo: bad".into()],
        };
        let text = envelope(&report, &roots, "0123456789abcdef0123456789abcdef", 1.5e9);
        assert!(!text.contains("bob") && !text.contains("z.zoo"), "{text}");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            json::parse(lines[1])
                .unwrap()
                .get("type")
                .and_then(Json::as_str),
            Some("event")
        );
        let event = json::parse(lines[2]).unwrap();
        let get = |k: &str| {
            event
                .get(k)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string()
        };
        assert_eq!(get("event_id").len(), 32);
        assert!(get("message").contains("bad \"zoo\""));
        assert_eq!(
            get("release"),
            format!("rustyzoo@{}", env!("CARGO_PKG_VERSION"))
        );
        assert!(["debug", "release"].contains(&get("environment").as_str()));
        let tags = event.get("tags").unwrap();
        assert_eq!(
            tags.get("os").and_then(Json::as_str),
            Some(std::env::consts::OS)
        );
        assert_eq!(
            tags.get("arch").and_then(Json::as_str),
            Some(std::env::consts::ARCH)
        );
        let extra = event.get("extra").unwrap();
        assert!(
            extra
                .get("backtrace")
                .and_then(Json::as_str)
                .unwrap()
                .contains("rustyzoo::app::x")
        );
        assert!(extra.to_string().contains("warning: <path>: bad"));
        assert_eq!(event.get("level").and_then(Json::as_str), Some("fatal"));
    }

    #[test]
    fn the_known_folders_are_never_in_a_report() {
        let install = Path::new("/mnt/Games/Zoo Tycoon/");
        let roots = private_roots(install);
        assert!(roots.contains(&"/mnt/Games/Zoo Tycoon".to_string()));
        assert!(roots.windows(2).all(|w| w[0].len() >= w[1].len()));
        assert!(roots.iter().all(|r| r.len() > 1));
    }

    /// What `offer` did: whether it asked, and whether it sent.
    fn run_offer(dsn: Option<&Dsn>, dir: Option<&Path>, answer: Option<bool>) -> (bool, bool) {
        let (asked, sent) = (Cell::new(false), Cell::new(false));
        offer(
            dsn,
            dir,
            (KEY, "crash report"),
            || {
                asked.set(true);
                answer
            },
            |_| {
                sent.set(true);
                Ok(())
            },
        );
        (asked.get(), sent.get())
    }

    #[test]
    fn without_a_server_address_nothing_is_asked_or_sent() {
        let dir = TempDir::new("crash-none");
        assert_eq!(run_offer(None, Some(&dir.0), Some(true)), (false, false));
        assert!(!dir.0.join("zoo.ini").exists());
        // The panic hook is not armed either: no address in the build, or one that is no address.
        let ask: fn(&str) -> Option<bool> = |_| Some(true);
        assert!(!install(None, None, Path::new("/x"), ask));
        assert!(!install(Some("nonsense"), None, Path::new("/x"), ask));
        assert!(!install(Some(""), None, Path::new("/x"), ask));
    }

    #[test]
    fn the_first_crash_asks_and_keeps_the_answer() {
        let dsn = dsn();
        let yes = TempDir::new("crash-yes");
        assert_eq!(
            run_offer(Some(&dsn), Some(&yes.0), Some(true)),
            (true, true)
        );
        assert_eq!(consent(&yes.0, KEY), Some(true));
        assert_eq!(
            run_offer(Some(&dsn), Some(&yes.0), Some(false)),
            (false, true)
        );
        let no = TempDir::new("crash-no");
        assert_eq!(
            run_offer(Some(&dsn), Some(&no.0), Some(false)),
            (true, false)
        );
        assert_eq!(consent(&no.0, KEY), Some(false));
        assert_eq!(
            run_offer(Some(&dsn), Some(&no.0), Some(true)),
            (false, false)
        );
        // A box that could not be shown or was closed keeps nothing and sends nothing.
        let closed = TempDir::new("crash-closed");
        assert_eq!(run_offer(Some(&dsn), Some(&closed.0), None), (true, false));
        assert_eq!(consent(&closed.0, KEY), None);
        assert_eq!(run_offer(Some(&dsn), None, Some(true)), (true, true));
    }

    #[test]
    fn the_setting_is_read_like_a_flag() {
        let dsn = dsn();
        for (value, expect) in [
            ("1", Some(true)),
            ("2", Some(true)),
            ("-1", Some(true)),
            ("0", Some(false)),
            ("yes", Some(false)),
            ("", None),
            ("0x", None),
            ("-5", Some(true)),
        ] {
            let dir = TempDir::new("crash-flag");
            userdir::save(&dir.0, SECTION, KEY, value).unwrap();
            assert_eq!(consent(&dir.0, KEY), expect, "{value:?}");
            let (asked, sent) = run_offer(Some(&dsn), Some(&dir.0), Some(true));
            assert_eq!(
                (asked, sent),
                (expect.is_none(), expect.is_none() || expect == Some(true)),
                "{value:?}"
            );
        }
    }

    #[test]
    fn the_question_names_the_setting_and_the_policy() {
        let q = question();
        assert!(q.contains("crashReports") && q.contains("https://rustyzoo.com/privacy/"));
    }

    #[test]
    fn the_log_keeps_the_last_lines() {
        for n in 0..LOG_LINES + 5 {
            log(format_args!("crash-test line {n}"));
        }
        let log = LOG.lock().unwrap();
        assert!(log.len() <= LOG_LINES);
        assert!(log.iter().all(|l| !l.is_empty()));
    }
}
