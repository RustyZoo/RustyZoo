//! The part of JSON the `mcp` server speaks: values, a parser that never panics on bad text, and
//! the text of a value.

use std::fmt;

/// How deep arrays and objects may nest in a request.
const MAX_DEPTH: u32 = 32;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn text(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }

    pub fn obj<const N: usize>(fields: [(&str, Json); N]) -> Json {
        Json::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Json::Null => f.write_str("null"),
            Json::Bool(b) => write!(f, "{b}"),
            Json::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            Json::Num(n) if n.is_finite() => write!(f, "{n}"),
            Json::Num(_) => f.write_str("null"),
            Json::Str(s) => {
                f.write_str("\"")?;
                for c in s.chars() {
                    match c {
                        '"' => f.write_str("\\\"")?,
                        '\\' => f.write_str("\\\\")?,
                        '\n' => f.write_str("\\n")?,
                        c if c < ' ' => write!(f, "\\u{:04x}", c as u32)?,
                        c => write!(f, "{c}")?,
                    }
                }
                f.write_str("\"")
            }
            Json::Arr(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    write!(f, "{}{item}", if i > 0 { "," } else { "" })?;
                }
                f.write_str("]")
            }
            Json::Obj(fields) => {
                f.write_str("{")?;
                for (i, (key, value)) in fields.iter().enumerate() {
                    let key = Json::text(key.as_str());
                    write!(f, "{}{key}:{value}", if i > 0 { "," } else { "" })?;
                }
                f.write_str("}")
            }
        }
    }
}

/// Reads one JSON value, with nothing but white space after it.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser {
        rest: text.trim_start(),
    };
    let value = p.value(0)?;
    match p.rest.trim_start().is_empty() {
        true => Ok(value),
        false => Err("text after the value".into()),
    }
}

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn skip(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip();
        let found = self.rest.starts_with(c);
        if found {
            self.rest = &self.rest[c.len_utf8()..];
        }
        found
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        match self.rest.strip_prefix(word) {
            Some(rest) => {
                self.rest = rest;
                Ok(value)
            }
            None => Err("bad value".into()),
        }
    }

    fn value(&mut self, depth: u32) -> Result<Json, String> {
        self.skip();
        if depth > MAX_DEPTH {
            return Err("nested too deep".into());
        }
        match self.rest.chars().next() {
            Some('{') => {
                self.eat('{');
                let mut fields = Vec::new();
                while !self.eat('}') {
                    if !fields.is_empty() && !self.eat(',') {
                        return Err("expected , or }".into());
                    }
                    self.skip();
                    let key = self.string()?;
                    if !self.eat(':') {
                        return Err("expected :".into());
                    }
                    fields.push((key, self.value(depth + 1)?));
                }
                Ok(Json::Obj(fields))
            }
            Some('[') => {
                self.eat('[');
                let mut items = Vec::new();
                while !self.eat(']') {
                    if !items.is_empty() && !self.eat(',') {
                        return Err("expected , or ]".into());
                    }
                    items.push(self.value(depth + 1)?);
                }
                Ok(Json::Arr(items))
            }
            Some('"') => self.string().map(Json::Str),
            Some('t') => self.word("true", Json::Bool(true)),
            Some('f') => self.word("false", Json::Bool(false)),
            Some('n') => self.word("null", Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => {
                let end = (self.rest)
                    .find(|c: char| !matches!(c, '0'..='9' | '+' | '-' | '.' | 'e' | 'E'))
                    .unwrap_or(self.rest.len());
                let n = self.rest[..end]
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite());
                self.rest = &self.rest[end..];
                n.map(Json::Num).ok_or_else(|| "bad number".into())
            }
            _ => Err("bad value".into()),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let Some(mut rest) = self.rest.strip_prefix('"') else {
            return Err("expected a string".into());
        };
        let mut out = String::new();
        let mut high = None; // a high surrogate waiting for its low half
        loop {
            let mut chars = rest.chars();
            let c = chars.next().ok_or("unterminated string")?;
            rest = chars.as_str();
            let lone = |high: &mut Option<u32>, out: &mut String| {
                if high.take().is_some() {
                    out.push('\u{fffd}');
                }
            };
            match c {
                '"' => {
                    lone(&mut high, &mut out);
                    self.rest = rest;
                    return Ok(out);
                }
                '\\' => {
                    let e = rest.chars().next().ok_or("unterminated string")?;
                    rest = &rest[e.len_utf8()..];
                    if e == 'u' {
                        let hex = rest.get(..4).ok_or("bad \\u escape")?;
                        let unit = u32::from_str_radix(hex, 16).map_err(|_| "bad \\u escape")?;
                        rest = &rest[4..];
                        match (high.take(), unit) {
                            (Some(h), 0xdc00..=0xdfff) => out.extend(char::from_u32(
                                0x10000 + ((h - 0xd800) << 10) + (unit - 0xdc00),
                            )),
                            (_, 0xd800..=0xdbff) => high = Some(unit),
                            (None, _) => out.push(char::from_u32(unit).unwrap_or('\u{fffd}')),
                            (Some(_), _) => {
                                out.push('\u{fffd}');
                                out.push(char::from_u32(unit).unwrap_or('\u{fffd}'));
                            }
                        }
                        continue;
                    }
                    lone(&mut high, &mut out);
                    out.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        '"' | '\\' | '/' => e,
                        _ => return Err("bad escape".into()),
                    });
                }
                c if c < ' ' => return Err("control character in a string".into()),
                c => {
                    lone(&mut high, &mut out);
                    out.push(c);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip() {
        let text = r#"{"a":[1,-2.5,true,null],"b":"x\"\n\u00e9\ud83d\ude00","c":{}}"#;
        let v = parse(text).unwrap();
        assert_eq!(v.get("b").and_then(Json::as_str), Some("x\"\né😀"));
        assert_eq!(parse(&v.to_string()).unwrap(), v);
        assert_eq!(
            v.get("a").map(|a| a.to_string()).unwrap(),
            "[1,-2.5,true,null]"
        );
    }

    #[test]
    fn bad_text_is_an_error() {
        for bad in [
            "",
            "{",
            "[1,",
            "{\"a\"}",
            "\"x",
            "tru",
            "1e999",
            "[1] 2",
            "\"\\x\"",
            "\"\\ud800\"x",
            "{\"a\":1,}",
            "\u{1}",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(parse(&"[".repeat(100)).is_err());
    }
}
