//! Opt-in start statistics: at the start of a game in a window, once the player has said Yes, one
//! anonymous event goes to the project's Umami server. Nothing is asked or sent without a server
//! address and a site in the build, with `RUSTYZOO_NO_CRASH_REPORTS` set, in a headless run, a test
//! or any other command.
//!
//! The address and the site are [`URL`] and [`WEBSITE`], set when the program is built. The answer
//! is asked once and kept in `[RustyZoo] startStats` of the settings file (1 send, 0 do not, absent
//! or unreadable: ask) and changed by editing it. The event holds the version, the build, the
//! system and the processor: no identifier, no path, no name of the machine. The server tells a
//! visit from another by a daily hash of the address and the user agent, which it does not keep.
//!
//! The event goes out in a background thread through the HTTPS client of the program (see [`crate::crash`]),
//! with a short time limit; a failed request is a warning and never ends the game.

use std::path::Path;

use crate::crash::{self, POLICY};
use crate::mcp::json::Json;

/// The server address of this build, from `RUSTYZOO_STATS_URL` when the program is built.
pub const URL: Option<&str> = option_env!("RUSTYZOO_STATS_URL");
/// The site of this build on the server, from `RUSTYZOO_STATS_WEBSITE` when the program is built.
pub const WEBSITE: Option<&str> = option_env!("RUSTYZOO_STATS_WEBSITE");

/// The settings key of the statistics question.
pub const KEY: &str = "startStats";
/// The host name events carry: the one of the game, not of the machine.
const HOSTNAME: &str = "game.rustyzoo.com";
const SECONDS: u32 = 5;

/// The question shown once.
fn question() -> String {
    format!(
        "Share anonymous statistics? At each start, RustyZoo sends the version, the build, your \
        system and its architecture, and nothing else: no identifier, no file or folder name. \
        Your answer is kept in zoo.ini ([RustyZoo] {KEY}) and you can change it there. \
        Policy: {POLICY}"
    )
}

/// A parsed server: where `/api/send` is and the site events belong to.
#[derive(Debug, PartialEq)]
pub struct Target {
    url: String,
    website: String,
}

impl Target {
    /// An `http` or `https` address and a site without spaces or quotes, or `None`.
    pub fn parse(url: &str, website: &str) -> Option<Target> {
        let url = url.trim_end_matches('/');
        let (scheme, host) = url.split_once("://")?;
        let bare = |s: &str| !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || c == '"');
        (matches!(scheme, "http" | "https") && bare(host) && bare(website)).then(|| Target {
            url: format!("{url}/api/send"),
            website: website.into(),
        })
    }
}

/// The body of the event.
pub fn payload(website: &str, version: &str, build: &str, os: &str, arch: &str) -> String {
    let data = Json::obj([
        ("version", Json::text(version)),
        ("build", Json::text(build)),
        ("os", Json::text(os)),
        ("arch", Json::text(arch)),
    ]);
    let payload = Json::obj([
        ("website", Json::text(website)),
        ("hostname", Json::text(HOSTNAME)),
        ("url", Json::text("/start")),
        ("name", Json::text("start")),
        ("data", data),
    ]);
    Json::obj([("type", Json::text("event")), ("payload", payload)]).to_string()
}

/// The user agent: the server drops the default one of an HTTP library as a robot.
pub fn user_agent(version: &str, os: &str, arch: &str) -> String {
    format!("RustyZoo/{version} ({os}; {arch})")
}

/// Sends the event of this build in the background.
fn send(target: &Target) -> Result<(), String> {
    let (version, os, arch) = (
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    let body = payload(&target.website, version, crash::build(), os, arch);
    let headers = [
        "Content-Type: application/json".to_string(),
        format!("User-Agent: {}", user_agent(version, os, arch)),
    ];
    let url = target.url.clone();
    std::thread::Builder::new()
        .name("stats".into())
        .spawn(move || {
            if let Err(e) = crash::post(&url, &headers, SECONDS, &body) {
                crate::logln!("rustyzoo: warning: statistics not sent: {e}");
            }
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

/// What to do at the start of a game in a window: nothing when `off`, without a server, or when the
/// answer is No; ask once when there is none; send on Yes. `ask` shows the question and gives the
/// answer (`None` if it could not be asked).
fn start_with(
    off: bool,
    target: Option<&Target>,
    dir: Option<&Path>,
    ask: impl FnOnce(&str) -> Option<bool>,
    send: impl FnOnce(&Target) -> Result<(), String>,
) {
    if !off {
        crash::offer(target, dir, (KEY, "statistics"), || ask(&question()), send);
    }
}

fn this_target() -> Option<Target> {
    URL.zip(WEBSITE).and_then(|(u, w)| Target::parse(u, w))
}

/// Whether a start of this build asks the statistics question: it has a server and the
/// environment does not turn reports off.
pub fn asks() -> bool {
    !crash::disabled() && this_target().is_some()
}

/// [`start_with`] for this build: its server, the environment, the settings folder `dir`.
pub fn start(dir: Option<&Path>, ask: impl FnOnce(&str) -> Option<bool>) {
    start_with(crash::disabled(), this_target().as_ref(), dir, ask, send);
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::fixtures::TempDir;
    use crate::mcp::json;

    fn target() -> Target {
        Target::parse("https://stats.example.org/", "site-id").unwrap()
    }

    /// Whether it asked and whether it sent.
    fn run(
        off: bool,
        target: Option<&Target>,
        dir: Option<&Path>,
        answer: Option<bool>,
    ) -> (bool, bool) {
        let (asked, sent) = (Cell::new(false), Cell::new(false));
        start_with(
            off,
            target,
            dir,
            |q| {
                assert!(q.contains(POLICY) && q.contains("startStats"));
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
    fn the_payload_has_the_version_build_system_and_nothing_else() {
        let text = payload("site-id", "1.2.3", "release", "linux", "x86_64");
        let event = json::parse(&text).unwrap();
        assert_eq!(event.get("type").and_then(Json::as_str), Some("event"));
        let payload = event.get("payload").unwrap();
        let get = |v: &Json, k: &str| v.get(k).and_then(Json::as_str).map(str::to_string);
        assert_eq!(get(payload, "website").as_deref(), Some("site-id"));
        assert_eq!(get(payload, "url").as_deref(), Some("/start"));
        assert_eq!(get(payload, "name").as_deref(), Some("start"));
        assert_eq!(get(payload, "hostname").as_deref(), Some(HOSTNAME));
        let data = payload.get("data").unwrap();
        assert_eq!(
            data.to_string(),
            r#"{"version":"1.2.3","build":"release","os":"linux","arch":"x86_64"}"#
        );
        assert_eq!(
            text,
            r#"{"type":"event","payload":{"website":"site-id","hostname":"game.rustyzoo.com","url":"/start","name":"start","data":{"version":"1.2.3","build":"release","os":"linux","arch":"x86_64"}}}"#
        );
        assert_eq!(
            user_agent("1.2.3", "linux", "x86_64"),
            "RustyZoo/1.2.3 (linux; x86_64)"
        );
    }

    #[test]
    fn a_server_needs_an_address_and_a_site() {
        let t = target();
        assert_eq!(t.url, "https://stats.example.org/api/send");
        assert_eq!(t.website, "site-id");
        assert_eq!(
            Target::parse("http://localhost:3000///", "s").unwrap().url,
            "http://localhost:3000/api/send"
        );
        for (url, site) in [
            ("", "s"),
            ("stats.example.org", "s"),
            ("ftp://h", "s"),
            ("https://", "s"),
            ("https://h x", "s"),
            ("https://h", ""),
            ("https://h", "a b"),
            ("https://h", "a\"b"),
            ("/", "s"),
        ] {
            assert_eq!(Target::parse(url, site), None, "{url:?} {site:?}");
        }
    }

    #[test]
    fn without_a_server_nothing_is_asked_or_sent() {
        let dir = TempDir::new("stats-none");
        assert_eq!(run(false, None, Some(&dir.0), Some(true)), (false, false));
        assert!(!dir.0.join("zoo.ini").exists());
        // This build has no server unless the environment gave one: `start` then asks nothing.
        if URL.is_none() || WEBSITE.is_none() {
            start(Some(&dir.0), |_| panic!("asked"));
        }
    }

    #[test]
    fn the_first_start_asks_and_keeps_the_answer() {
        let t = target();
        let yes = TempDir::new("stats-yes");
        assert_eq!(run(false, Some(&t), Some(&yes.0), Some(true)), (true, true));
        assert_eq!(
            crate::userdir::setting(&yes.0, "RustyZoo", KEY).as_deref(),
            Some("1")
        );
        assert_eq!(
            run(false, Some(&t), Some(&yes.0), Some(false)),
            (false, true)
        );
        let no = TempDir::new("stats-no");
        assert_eq!(
            run(false, Some(&t), Some(&no.0), Some(false)),
            (true, false)
        );
        assert_eq!(
            crate::userdir::setting(&no.0, "RustyZoo", KEY).as_deref(),
            Some("0")
        );
        for _ in 0..3 {
            assert_eq!(
                run(false, Some(&t), Some(&no.0), Some(true)),
                (false, false)
            );
        }
        // A box that could not be shown or was closed keeps nothing and sends nothing.
        let closed = TempDir::new("stats-closed");
        assert_eq!(run(false, Some(&t), Some(&closed.0), None), (true, false));
        assert_eq!(crate::userdir::setting(&closed.0, "RustyZoo", KEY), None);
    }

    #[test]
    fn the_opt_out_sends_and_asks_nothing() {
        let t = target();
        let yes = TempDir::new("stats-off");
        crate::userdir::save(&yes.0, "RustyZoo", KEY, "1").unwrap();
        assert_eq!(
            run(true, Some(&t), Some(&yes.0), Some(true)),
            (false, false)
        );
        let fresh = TempDir::new("stats-off-fresh");
        assert_eq!(
            run(true, Some(&t), Some(&fresh.0), Some(true)),
            (false, false)
        );
        assert!(!fresh.0.join("zoo.ini").exists());
    }
}
