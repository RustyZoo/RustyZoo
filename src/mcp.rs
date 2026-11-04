//! `rustyzoo mcp`: a Model Context Protocol server on standard input and output (one JSON-RPC 2.0
//! message per line) that loads a zoo, steps it without an interface and tells what is in it, so
//! that a client can reproduce and inspect a game headless. It changes no game behaviour: it
//! drives a [`Run`] the way the `sim` command does.

mod json;

use std::collections::VecDeque;
use std::io::{self, BufRead, Write};

use json::Json;

use crate::entity::Catalogue;
use crate::money::Money;
use crate::sim::drive::{Command, Run};
use crate::vfs::Vfs;
use crate::zoo::Zoo;

/// The protocol version answered when the client names none.
const PROTOCOL: &str = "2025-06-18";
/// The most steps one `step` call runs, and the longest step in milliseconds.
const MAX_TICKS: f64 = 1_000_000.0;
const MAX_DT: f64 = 100_000.0;
/// How many log lines are kept for `log`, and how many it returns when no count is given.
const KEPT_LINES: usize = 10_000;
const TAIL_LINES: usize = 50;
const DEFAULT_DT: u32 = 67;

/// The tools: name, description and the properties of the arguments (`*` marks a required one).
const TOOLS: [(&str, &str, &[&str]); 8] = [
    (
        "load",
        "Start a game from a map or saved game of the game data (map, e.g. maps/default.zoo) or a saved zoo file on disk (file); seed defaults to 1",
        &["map", "file", "seed"],
    ),
    (
        "step",
        "Run ticks simulation steps of dt milliseconds (default 67); the log lines are kept for log",
        &["ticks*", "dt"],
    ),
    (
        "command",
        "Apply a script command now, between steps (e.g. cash 1000, animal lion 30,10 female)",
        &["text*"],
    ),
    ("state", "Date, cash, rating and counts of the game", &[]),
    ("entity", "The fields of one entity and its unit", &["id*"]),
    (
        "hash",
        "The hash of the state, as sim --hash prints it",
        &[],
    ),
    ("save", "Write the zoo to a file on disk", &["file*"]),
    ("log", "The last lines of the log (default 50)", &["lines"]),
];

struct Session<'a> {
    vfs: &'a Vfs,
    run: Option<Run>,
    log: VecDeque<String>,
}

/// Reads requests from `input` until it ends and writes the answers to `out`.
pub fn serve(vfs: &Vfs, input: impl BufRead, mut out: impl Write) -> io::Result<()> {
    let mut session = Session {
        vfs,
        run: None,
        log: VecDeque::new(),
    };
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = session.handle(&line) {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn reply(id: &Json, outcome: Result<Json, (i32, String)>) -> Json {
    let (key, body) = match outcome {
        Ok(result) => ("result", result),
        Err((code, message)) => (
            "error",
            Json::obj([
                ("code", Json::Num(code.into())),
                ("message", Json::text(message)),
            ]),
        ),
    };
    Json::obj([
        ("jsonrpc", Json::text("2.0")),
        ("id", id.clone()),
        (key, body),
    ])
}

/// A tool's answer: text, and whether it is an error the client should see.
fn content(text: String, is_error: bool) -> Json {
    let part = Json::obj([("type", Json::text("text")), ("text", Json::Str(text))]);
    Json::obj([
        ("content", Json::Arr(vec![part])),
        ("isError", Json::Bool(is_error)),
    ])
}

fn tool_list() -> Json {
    let tool = |&(name, about, props): &(&str, &str, &[&str])| {
        let field = |p: &&str| (p.trim_end_matches('*').to_string(), Json::obj([]));
        let required = (props.iter().filter(|p| p.ends_with('*')))
            .map(|p| Json::text(p.trim_end_matches('*')))
            .collect();
        Json::obj([
            ("name", Json::text(name)),
            ("description", Json::text(about)),
            (
                "inputSchema",
                Json::obj([
                    ("type", Json::text("object")),
                    ("properties", Json::Obj(props.iter().map(field).collect())),
                    ("required", Json::Arr(required)),
                ]),
            ),
        ])
    };
    Json::obj([("tools", Json::Arr(TOOLS.iter().map(tool).collect()))])
}

impl Session<'_> {
    /// The answer to one line; none for a notification.
    fn handle(&mut self, line: &str) -> Option<Json> {
        let request = match json::parse(line) {
            Ok(request @ Json::Obj(_)) => request,
            Ok(_) => return Some(reply(&Json::Null, Err((-32600, "not a request".into())))),
            Err(why) => return Some(reply(&Json::Null, Err((-32700, why)))),
        };
        let id = request.get("id")?;
        let Some(method) = request.get("method").and_then(Json::as_str) else {
            return Some(reply(id, Err((-32600, "no method".into()))));
        };
        let params = request.get("params").unwrap_or(&Json::Null);
        let outcome = match method {
            "initialize" => Ok(initialize(params)),
            "ping" => Ok(Json::obj([])),
            "tools/list" => Ok(tool_list()),
            "tools/call" => match params.get("name").and_then(Json::as_str) {
                Some(name) => {
                    let args = params.get("arguments").unwrap_or(&Json::Null);
                    Ok(match self.call(name, args) {
                        Ok(text) => content(text, false),
                        Err(why) => content(why, true),
                    })
                }
                None => Err((-32602, "no tool name".into())),
            },
            _ => Err((-32601, format!("unknown method {method}"))),
        };
        Some(reply(id, outcome))
    }

    fn call(&mut self, name: &str, args: &Json) -> Result<String, String> {
        match name {
            "load" => self.load(args),
            "step" => self.step(args),
            "command" => {
                let text = string(args, "text")?;
                let command: Command = text.parse()?;
                if matches!(command, Command::Save(_) | Command::Load(_)) {
                    return Err("use the save and load tools".into());
                }
                match self.run()?.apply(&command) {
                    true => Ok("applied".into()),
                    false => Err("changed nothing".into()),
                }
            }
            "state" => Ok(self.run()?.summary()),
            "entity" => {
                let id = whole(args, "id", u32::MAX.into())? as u32;
                (self.run()?.describe(id)).ok_or_else(|| format!("no entity {id}"))
            }
            "hash" => Ok(format!("{:016x}", self.run()?.hash())),
            "save" => {
                let file = string(args, "file")?;
                let bytes = self.run()?.save()?;
                std::fs::write(file, bytes).map_err(|e| format!("{file}: {e}"))?;
                Ok(format!("saved {file}"))
            }
            "log" => {
                let n = match args.get("lines") {
                    None => TAIL_LINES,
                    Some(_) => whole(args, "lines", KEPT_LINES as f64)? as usize,
                };
                let skip = self.log.len().saturating_sub(n);
                Ok(self
                    .log
                    .iter()
                    .skip(skip)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }

    fn run(&mut self) -> Result<&mut Run, String> {
        self.run
            .as_mut()
            .ok_or_else(|| "no game: call load first".into())
    }

    fn load(&mut self, args: &Json) -> Result<String, String> {
        let seed = match args.get("seed") {
            None => 1,
            Some(_) => whole(args, "seed", u32::MAX.into())? as u32,
        };
        let (saved, bytes, what) = match (string(args, "map"), string(args, "file")) {
            (Ok(map), Err(_)) => (false, self.vfs.read(map).map_err(|e| e.to_string())?, map),
            (Err(_), Ok(file)) => (
                true,
                std::fs::read(file).map_err(|e| format!("{file}: {e}"))?,
                file,
            ),
            _ => return Err("give either map or file".into()),
        };
        let cat = Catalogue::load(self.vfs).map_err(|e| e.to_string())?;
        let (zoo, money): (Zoo, Money) = Zoo::parse_money(&bytes)
            .and_then(|(zoo, money)| Ok((zoo.checked(&cat)?, money)))
            .map_err(|e| format!("{what}: {e}"))?;
        let economy = self.vfs.read("economy.cfg").unwrap_or_default();
        let fences = (!saved).then_some(false);
        let run = Run::from_vfs(self.vfs, &economy, cat, (zoo, money), seed, fences);
        self.log.clear();
        Ok(format!("loaded {what}\n{}", self.run.insert(run).summary()))
    }

    fn step(&mut self, args: &Json) -> Result<String, String> {
        let ticks = whole(args, "ticks", MAX_TICKS)? as u64;
        let dt = match args.get("dt") {
            None => DEFAULT_DT,
            Some(_) => match whole(args, "dt", MAX_DT)? as u32 {
                0 => return Err("dt must be at least 1".into()),
                dt => dt,
            },
        };
        let vfs = self.vfs;
        let run = self.run.as_mut().ok_or("no game: call load first")?;
        for _ in 0..ticks {
            for line in run.step_with(dt, Some(vfs)) {
                if self.log.len() == KEPT_LINES {
                    self.log.pop_front();
                }
                self.log.push_back(line);
            }
        }
        Ok(format!("stepped {ticks}\n{}", run.summary()))
    }
}

fn initialize(params: &Json) -> Json {
    let version = params.get("protocolVersion").and_then(Json::as_str);
    Json::obj([
        ("protocolVersion", Json::text(version.unwrap_or(PROTOCOL))),
        ("capabilities", Json::obj([("tools", Json::obj([]))])),
        (
            "serverInfo",
            Json::obj([
                ("name", Json::text("rustyzoo")),
                ("version", Json::text(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
    ])
}

fn string<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

/// A whole number argument from 0 to `max`.
fn whole(args: &Json, key: &str, max: f64) -> Result<f64, String> {
    let n = args.get(key).and_then(Json::as_f64);
    n.filter(|n| n.fract() == 0.0 && (0.0..=max).contains(n))
        .ok_or_else(|| format!("{key} must be a whole number from 0 to {max}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::Install;

    /// The answers (one per line) of a server without game data to the requests.
    fn ask(requests: &[&str]) -> Vec<Json> {
        let install = Install::new("mcp", &[]);
        let vfs = Vfs::open(&install.0).unwrap();
        let mut out = Vec::new();
        serve(&vfs, requests.join("\n").as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        text.lines().map(|l| json::parse(l).unwrap()).collect()
    }

    fn call(name: &str, arguments: &str) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"{name}","arguments":{arguments}}}}}"#
        )
    }

    fn is_error(answer: &Json) -> bool {
        answer.get("result").and_then(|r| r.get("isError")) == Some(&Json::Bool(true))
    }

    #[test]
    fn the_server_answers_over_lines() {
        let answers = ask(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"nothing"}"#,
            "not json",
        ]);
        assert_eq!(answers.len(), 4, "a notification gets no answer");
        let version = answers[0]
            .get("result")
            .and_then(|r| r.get("protocolVersion"));
        assert_eq!(version.and_then(Json::as_str), Some("2024-11-05"));
        assert_eq!(answers[1].get("id"), Some(&Json::text("a")));
        let tools = answers[1].get("result").and_then(|r| r.get("tools"));
        let Some(Json::Arr(tools)) = tools else {
            panic!("no tool list")
        };
        assert_eq!(tools.len(), TOOLS.len());
        let code = |a: &Json| {
            a.get("error")
                .and_then(|e| e.get("code"))
                .and_then(Json::as_f64)
        };
        assert_eq!(
            (code(&answers[2]), code(&answers[3])),
            (Some(-32601.0), Some(-32700.0))
        );
    }

    fn message(answer: &Json) -> String {
        let part = answer.get("result").and_then(|r| r.get("content"));
        let Some(Json::Arr(parts)) = part else {
            return String::new();
        };
        parts[0]
            .get("text")
            .and_then(Json::as_str)
            .unwrap_or("")
            .into()
    }

    #[test]
    fn a_tool_with_bad_arguments_or_no_game_is_an_error_not_the_end() {
        let cases = [
            (call("state", "{}"), "no game"),
            (call("hash", "{}"), "no game"),
            (call("step", r#"{"ticks":1}"#), "no game"),
            (call("step", r#"{"ticks":0}"#), "no game"),
            (call("step", "{}"), "ticks must be"),
            (call("step", r#"{"ticks":-1}"#), "ticks must be"),
            (call("step", r#"{"ticks":1e12}"#), "ticks must be"),
            (call("step", r#"{"ticks":1.5}"#), "ticks must be"),
            (call("step", r#"{"ticks":"3"}"#), "ticks must be"),
            (call("step", r#"{"ticks":1,"dt":0}"#), "dt must be"),
            (call("step", r#"{"ticks":1,"dt":1e9}"#), "dt must be"),
            (call("entity", r#"{"id":-1}"#), "id must be"),
            (call("entity", r#"{"id":4294967296}"#), "id must be"),
            (call("entity", r#"{"id":5}"#), "no game"),
            (call("command", r#"{"text":"cash 5"}"#), "no game"),
            (call("command", r#"{"text":"nothing"}"#), "unknown command"),
            (
                call("command", r#"{"text":"save x"}"#),
                "save and load tools",
            ),
            (call("load", "{}"), "either map or file"),
            (
                call("load", r#"{"map":"a","file":"b"}"#),
                "either map or file",
            ),
            (call("load", r#"{"map":"maps/none.zoo"}"#), "none.zoo"),
            (call("load", r#"{"file":"/none/x.zoo"}"#), "/none/x.zoo"),
            (call("load", r#"{"map":"m","seed":-1}"#), "seed must be"),
            (call("save", r#"{"file":"/none/x.zoo"}"#), "no game"),
            (call("nothing", "{}"), "unknown tool"),
        ];
        let requests: Vec<&str> = cases.iter().map(|(r, _)| r.as_str()).collect();
        let answers = ask(&requests);
        assert_eq!(answers.len(), cases.len());
        for ((request, want), answer) in cases.iter().zip(&answers) {
            assert!(is_error(answer), "{request}");
            assert!(
                message(answer).contains(want),
                "{request}: {}",
                message(answer)
            );
        }
        let log = ask(&[&call("log", "{}"), &call("log", r#"{"lines":0}"#)]);
        assert!(!is_error(&log[0]) && !is_error(&log[1]));
        let huge = ask(&[
            &call("log", r#"{"lines":1e9}"#),
            &call("log", r#"{"lines":-1}"#),
        ]);
        assert!(huge.iter().all(is_error));
    }

    /// A map loads, steps and hashes again: the same load gives the same hash, a step moves it,
    /// and the cash command and the entity of a unit are answered.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_load_step_and_hash() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let mut out = Vec::new();
        let load = call("load", r#"{"map":"maps/default.zoo","seed":3}"#);
        let hash = call("hash", "{}");
        let step = call("step", r#"{"ticks":20}"#);
        let cash = call("command", r#"{"text":"cash 1000"}"#);
        let state = call("state", "{}");
        let log = call("log", r#"{"lines":5}"#);
        let requests = [
            &load, &hash, &step, &hash, &load, &hash, &cash, &state, &log,
        ];
        serve(
            &vfs,
            requests.map(String::as_str).join("\n").as_bytes(),
            &mut out,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        let answers: Vec<Json> = text.lines().map(|l| json::parse(l).unwrap()).collect();
        assert!(!answers.iter().any(is_error), "{text}");
        let (before, after, again) = (
            message(&answers[1]),
            message(&answers[3]),
            message(&answers[5]),
        );
        assert_eq!(before.len(), 16);
        assert_ne!(before, after);
        assert_eq!(before, again);
        assert!(message(&answers[7]).contains("cash "));
    }
}
