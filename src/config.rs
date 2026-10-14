//! The INI-like dialect in which the game describes almost everything: animals (`.ai`), lists
//! (`.cfg`), animations (`.ani`), scenarios (`.scn`), UI layouts (`.lyt`), `.uca`, `.ucb`, `.ucs`
//! and the `.ini` members of the archives. (The `zoo.ini` next to the executable is a different
//! thing, read by Windows; see `vfs.rs`.)
//!
//! The reader is a scanner for items, not a line parser, and it never rejects a file for its
//! content. Following the original reader, quirks included:
//!
//! * Only the line feed ends a line. Every byte from 0x00 to 0x20 is white space: it is skipped
//!   before an item and trimmed from both ends of every name and value. Bytes from 0x80 up are
//!   ordinary and never converted; names and values are bytes, not text.
//! * `[name]` starts a section. The name runs to the first `]` on the line, trimmed. Text after
//!   the `]` is scanned as the next item. A second header with the same name continues the first
//!   section. A header without `]` on its line, or with an empty name, creates nothing and closes
//!   the current section: what follows is ignored until the next valid header. Text before the
//!   first valid header is ignored too.
//! * `;` at the start of an item is a comment to the end of the line. Nothing else is a comment
//!   mark, and nothing is cut from the end of a value.
//! * Any other item is `key = value`, split at the first `=`. A line without `=` is a key without
//!   value, and so is `key =`; the two cannot be told apart. An item that starts with `=` stores
//!   nothing, and the rest of the line is scanned as a new item.
//! * A key repeated in a section keeps all its values in file order, wherever the lines stand;
//!   "the" value of a key is the first.
//! * Names are compared byte for byte: `Icon` and `icon` are different keys.
//! * The order of sections, and of different keys, is not kept: listings come back in ascending
//!   byte order, each name once. A list in file order is one key written several times.
//! * A file in which no section was created counts as not loaded ([`Config::is_empty`]).
//!
//! Numbers are converted when a caller asks for them: [`to_int`], [`to_float`], [`to_flag`].

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// The largest input accepted: 1 MiB. The reader of the original has no limit; this one bounds
/// the memory a hostile archive member can cost, at about 40 times its size. The largest file
/// in the game has 72,908 bytes.
pub const MAX_INPUT_LEN: usize = 1024 * 1024;

/// The input is longer than [`MAX_INPUT_LEN`].
#[derive(Debug, PartialEq, Eq)]
pub struct TooLarge {
    pub len: usize,
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bytes, more than the {MAX_INPUT_LEN} byte limit for a configuration file",
            self.len
        )
    }
}

impl Error for TooLarge {}

/// A parsed file: sections by exact name, in ascending byte order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Config {
    sections: BTreeMap<Vec<u8>, Section>,
    /// How many items (lines to the line end) outside any section were skipped before the first
    /// header of any kind. Comments do not count.
    pub ignored_before_first_header: usize,
    /// The same, after a header without `]` or with an empty name.
    pub ignored_after_broken_header: usize,
}

/// The keys of one section by exact name, in ascending byte order, each with its values in file
/// order. The list of values may be empty.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Section {
    keys: BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
}

impl Config {
    /// Reads a file. Fails only if it is longer than [`MAX_INPUT_LEN`]; any content is accepted.
    pub fn parse(bytes: &[u8]) -> Result<Config, TooLarge> {
        if bytes.len() > MAX_INPUT_LEN {
            return Err(TooLarge { len: bytes.len() });
        }
        let n = bytes.len();
        // The index of the line feed that ends the line `from` is on, or `n`.
        let line_end = |from: usize| {
            let from = from.min(n);
            bytes[from..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(n, |p| from + p)
        };
        let mut config = Config::default();
        // The section being filled is held here and put back into the map at the next header
        // and at the end, so no entry looks it up by name.
        let mut current: Option<(Vec<u8>, Section)> = None;
        let mut header_seen = false;
        let mut i = 0;
        while i < n {
            let c = bytes[i];
            if c <= 0x20 {
                i += 1;
            } else if c == b'[' {
                header_seen = true;
                let rest = &bytes[i + 1..];
                match rest.iter().position(|&b| b == b']' || b == b'\n') {
                    Some(p) if rest[p] == b']' => {
                        let name = trim(&rest[..p]);
                        i += p + 2;
                        config.close(current.take());
                        current = (!name.is_empty()).then(|| {
                            let section = config.sections.remove(name).unwrap_or_default();
                            (name.to_vec(), section)
                        });
                    }
                    _ => {
                        i = line_end(i);
                        config.close(current.take());
                    }
                }
            } else if c == b';' {
                i = line_end(i);
            } else if let Some((_, section)) = &mut current {
                let stop = bytes[i..].iter().position(|&b| b == b'=' || b == b'\n');
                let j = stop.map_or(n, |p| i + p);
                let at_equals = j < n && bytes[j] == b'=';
                let key = trim(&bytes[i..j]);
                i = j + 1;
                // ponytail: an item that starts with `=` stores nothing and the rest of the
                // line is scanned as a new item, as in the original (`= 5` makes the key `5`).
                if key.is_empty() {
                    continue;
                }
                let values = section.keys.entry(key.to_vec()).or_default();
                if at_equals {
                    let end = line_end(i);
                    let value = trim(&bytes[i..end]);
                    i = end + 1;
                    if !value.is_empty() {
                        values.push(value.to_vec());
                    }
                }
            } else {
                if header_seen {
                    config.ignored_after_broken_header += 1;
                } else {
                    config.ignored_before_first_header += 1;
                }
                i = line_end(i);
            }
        }
        config.close(current);
        Ok(config)
    }

    fn close(&mut self, section: Option<(Vec<u8>, Section)>) {
        if let Some((name, section)) = section {
            self.sections.insert(name, section);
        }
    }

    /// Whether no section was created: the original counts such a file as not loaded.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// The section with exactly this name.
    pub fn section(&self, name: impl AsRef<[u8]>) -> Option<&Section> {
        self.sections.get(name.as_ref())
    }

    /// All sections, by name in ascending byte order.
    pub fn sections(&self) -> impl Iterator<Item = (&[u8], &Section)> {
        self.sections
            .iter()
            .map(|(name, section)| (name.as_slice(), section))
    }

    /// Gives the key the single value `value`, replacing its values. The section and the key
    /// are found ignoring letter case (the way the ini file next to the executable is read) and
    /// created, with this spelling, if missing. The value must fit on one line.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        self.set_values(
            section.as_bytes(),
            key.as_bytes(),
            vec![value.as_bytes().to_vec()],
        );
    }

    fn set_values(&mut self, section: &[u8], key: &[u8], values: Vec<Vec<u8>>) {
        let name = self
            .sections
            .keys()
            .find(|name| name.eq_ignore_ascii_case(section))
            .map_or(section, Vec::as_slice)
            .to_vec();
        let keys = &mut self.sections.entry(name).or_default().keys;
        let name = keys
            .keys()
            .find(|name| name.eq_ignore_ascii_case(key))
            .map_or(key, Vec::as_slice)
            .to_vec();
        keys.insert(name, values);
    }

    /// Puts every key of `over` on top of this file: it replaces the values of the same key of
    /// the same section, names compared ignoring letter case, and the rest stays.
    pub fn overlay(&mut self, over: &Config) {
        for (section, keys) in over.sections() {
            for (key, values) in &keys.keys {
                self.set_values(section, key, values.clone());
            }
        }
    }

    /// The file written out, to be read back as the same sections, keys and values (comments
    /// and the order of the items are lost, as in the listings).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, section) in &self.sections {
            out.extend([b"[", name.as_slice(), b"]\n"].concat());
            for (key, values) in &section.keys {
                if values.is_empty() {
                    out.extend([key.as_slice(), b"\n"].concat());
                }
                for value in values {
                    out.extend([key.as_slice(), b"=", value, b"\n"].concat());
                }
            }
        }
        out
    }
}

impl Section {
    /// The first value of the key; `None` if the key is missing or has no value (`key` and
    /// `key =` are the same).
    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<&[u8]> {
        self.get_all(key).first().map(Vec::as_slice)
    }

    /// Whether the key exists, with or without values.
    pub fn has(&self, key: impl AsRef<[u8]>) -> bool {
        self.keys.contains_key(key.as_ref())
    }

    /// All values of the key in file order; empty if the key is missing or has no value.
    pub fn get_all(&self, key: impl AsRef<[u8]>) -> &[Vec<u8>] {
        self.keys.get(key.as_ref()).map_or(&[], Vec::as_slice)
    }

    /// All keys, with or without values, in ascending byte order. This is how a list written
    /// as bare lines (the class chain under `[Member]`) is read.
    pub fn keys(&self) -> impl Iterator<Item = &[u8]> {
        self.keys.keys().map(Vec::as_slice)
    }
}

/// Removes bytes 0x00 to 0x20 from both ends, then cuts at the first zero byte left inside.
/// ponytail: the original keeps text after a zero byte in memory but never sees it, so it is
/// cut here. No shipped file has a zero byte.
fn trim(text: &[u8]) -> &[u8] {
    let start = text.iter().position(|&b| b > 0x20).unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&b| b > 0x20)
        .map_or(start, |p| p + 1);
    let text = &text[start..end];
    text.split(|&b| b == 0).next().unwrap_or_default()
}

/// A value as an integer, the way the original converts: a value starting with `0x` is
/// hexadecimal (digits in either case, 32 bits, `0xFFFFFFFF` is -1); anything else is read like
/// C `atol`: optional white space, one optional sign, decimal digits, the rest ignored, 0 if
/// there is no digit. Too many digits wrap around. `None` for `0x` without a hexadecimal digit:
/// the caller then leaves its variable as it was.
pub fn to_int(value: &[u8]) -> Option<i32> {
    if let Some(digits) = value.strip_prefix(b"0x") {
        let digits = digits.iter().map_while(|&b| char::from(b).to_digit(16));
        let mut number: Option<u32> = None;
        for digit in digits {
            number = Some(number.unwrap_or(0).wrapping_mul(16).wrapping_add(digit));
        }
        return number.map(|n| n as i32);
    }
    let value = value.trim_ascii_start();
    let (negative, digits) = match value.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, value),
    };
    let number = digits
        .iter()
        .map_while(|&b| char::from(b).to_digit(10))
        .fold(0i32, |n, digit| {
            n.wrapping_mul(10).wrapping_add(digit as i32)
        });
    Some(if negative {
        number.wrapping_neg()
    } else {
        number
    })
}

/// A value as a decimal number, the way C `atof` reads it: optional white space, optional sign,
/// digits with an optional `.` and more digits, optional exponent, the rest ignored, 0 if no
/// number can be read. The decimal mark is always `.`.
/// ponytail: the original's use of a locale with another decimal mark is not ruled out.
pub fn to_float(value: &[u8]) -> f32 {
    let value = value.trim_ascii_start();
    let digits_from = |at: usize| {
        value[at..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(value.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let int_digits = digits_from(end);
    end += int_digits;
    if value.get(end) == Some(&b'.') {
        let fraction = digits_from(end + 1);
        if int_digits + fraction > 0 {
            end += 1 + fraction;
        }
    }
    if matches!(value.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(value.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits_from(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    // The prefix is ASCII by construction.
    std::str::from_utf8(&value[..end])
        .ok()
        .and_then(|number| number.parse::<f64>().ok())
        .map_or(0.0, |number| number as f32)
}

/// A flag is an integer tested against 0: `1`, `2` and `-1` are on; `0`, `yes` and `true` are
/// off, the last two because they convert to 0. `None` as for [`to_int`].
pub fn to_flag(value: &[u8]) -> Option<bool> {
    to_int(value).map(|n| n != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sections in listing order, each as `name: key(value,value), key()`; sections are
    /// separated by `; `. Bytes are shown as `escape_ascii` does.
    fn dump(text: &[u8]) -> String {
        let config = Config::parse(text).unwrap();
        let mut out = Vec::new();
        for (name, section) in config.sections() {
            let keys: Vec<String> = section
                .keys()
                .map(|key| {
                    let values: Vec<String> = section
                        .get_all(key)
                        .iter()
                        .map(|v| v.escape_ascii().to_string())
                        .collect();
                    format!("{}({})", key.escape_ascii(), values.join(","))
                })
                .collect();
            out.push(
                format!("{}: {}", name.escape_ascii(), keys.join(", "))
                    .trim_end()
                    .to_string(),
            );
        }
        out.join("; ")
    }

    fn check(text: &[u8], expected: &str) {
        assert_eq!(dump(text), expected, "{}", text.escape_ascii());
    }

    #[test]
    fn rule_2_3_whitespace_and_line_ends() {
        check(b"[a]\r\nk = v\r\n", "a: k(v)");
        check(b"[a]\n\tk\t=\tv\t\n", "a: k(v)");
        // Only the line feed ends a line; a lone CR stays inside the value.
        check(b"[a]\nk=1\rj=2\n", "a: k(1\\rj=2)");
        // Every byte up to 0x20 is white space, the old end-of-file mark included.
        check(b"[a]\nk=v\n\x1A", "a: k(v)");
        check(b"[a]\n\x01k\x02=\x03v\x1F\n", "a: k(v)");
        // Bytes from 0x80 up are ordinary: 0xA0 is not trimmed.
        check(b"[a]\nk=caf\xE9\xA0\n", "a: k(caf\\xe9\\xa0)");
        check(b"[a]\nk\n k = 1\n", "a: k(1)");
    }

    #[test]
    fn rule_4_headers() {
        check(b"[ a b ]\nk=v\n", "a b: k(v)");
        check(b"[a] k=v\n", "a: k(v)");
        check(b"[a] ; c\nk=v\n", "a: k(v)");
        check(b"[a][b]\nk=v\n", "a:; b: k(v)");
        check(b"[[x]\nk=v\n", "[x: k(v)");
        check(b"[a]b]\nk=v\n", "a: b](), k(v)");
        check(
            b"[f/Characteristics/Integers]\n[f\\B\\bHappy1]\n",
            "f/Characteristics/Integers:; f\\\\B\\\\bHappy1:",
        );
        check(b"[a]", "a:");
    }

    #[test]
    fn rule_4_same_section_twice_continues_the_first() {
        check(
            b"[a]\nx=1\nx=2\n[b]\ny=3\n[a]\nx=4\nz\n",
            "a: x(1,2,4), z(); b: y(3)",
        );
    }

    #[test]
    fn rule_4_8_broken_and_empty_headers_close_the_section() {
        check(b"[a]\nk=v\n[b\nx=1\n[c]\ny=2\n", "a: k(v); c: y(2)");
        check(b"[a]\nk=v\n[]\nx=1\n[b]\ny=2\n", "a: k(v); b: y(2)");
        check(b"[a]\nk=v\n[  ]\nx=1\n", "a: k(v)");
        // A header without `]` on its line, though a `]` follows on a later line.
        check(b"[a]\n[b\n]\nx=1\n", "a:");
    }

    #[test]
    fn rule_8_text_before_the_first_header_is_ignored() {
        check(b"k=1\n[a]\nx=2\n", "a: x(2)");
        let config = Config::parse(b"k=1\nj\n; c\n[a]\nx=2\n[b\ny=1\nz\n[]\nw\n[c]\nv\n").unwrap();
        assert_eq!(config.ignored_before_first_header, 2);
        assert_eq!(config.ignored_after_broken_header, 3);
    }

    #[test]
    fn rule_5_only_a_semicolon_starts_a_comment() {
        check(b"[a]\n; c\n// c\n:c\n#c\n", "a: #c(), // c(), :c()");
        check(b"[a]\n  ; indented\n\n\t\nk = v ; w\n", "a: k(v ; w)");
        check(
            b"[a]\nlayer=1;\nc = -20;\nb = x//y\n",
            "a: b(x//y), c(-20;), layer(1;)",
        );
    }

    #[test]
    fn rule_6_entries_split_at_the_first_equals() {
        check(b"[a]\nurl = a = b\n", "a: url(a = b)");
        check(b"[a]\ntext==HELP_GSTAND\t\n", "a: text(=HELP_GSTAND)");
        check(b"[a]\nSmall Rock 4\n", "a: Small Rock 4()");
        check(
            b"[a]\nSubtype = \nDefaultSubtype = f\n",
            "a: DefaultSubtype(f), Subtype()",
        );
        // An item that starts with `=` stores nothing; the rest of the line is a new item.
        check(b"[a]\n= 5\n", "a: 5()");
        check(b"[a]\n=x=y\n", "a: x(y)");
    }

    #[test]
    fn rule_6_7_key_without_value_reads_as_not_found() {
        let config = Config::parse(b"[a]\nk\nj =\nm = 1\nm=\n").unwrap();
        let a = config.section("a").unwrap();
        assert_eq!((a.get("k"), a.get("j"), a.get("zz")), (None, None, None));
        assert!(a.get_all("k").is_empty());
        assert_eq!(a.keys().collect::<Vec<_>>(), [b"j", b"k", b"m"]);
        assert!(config.section("b").is_none());
    }

    #[test]
    fn rule_7_repeated_keys_keep_all_values_and_the_first_is_the_value() {
        check(b"[a]\nk =\nk= 5 \nk=\nk=6\n", "a: k(5,6)");
        let config = Config::parse(b"[a]\nid=1002\nx=1\nid=3251\n[a]\nid=9300\n").unwrap();
        let a = config.section("a").unwrap();
        assert_eq!(a.get("id"), Some(&b"1002"[..]));
        assert_eq!(a.get_all("id"), [b"1002", b"3251", b"9300"]);
        // The values do not keep the key borrowed.
        let mut key = b"id".to_vec();
        let ids = a.get_all(&key);
        key.clear();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn rule_9_names_are_compared_byte_for_byte() {
        check(b"[A]\nk=1\n[a]\nK=2\n", "A: k(1); a: K(2)");
        let config = Config::parse(b"[Sounds]\nIcon=1\n").unwrap();
        assert!(config.section("sounds").is_none());
        assert!(config.section("Sounds").unwrap().get("icon").is_none());
        assert_eq!(
            config.section("Sounds").unwrap().get("Icon"),
            Some(&b"1"[..])
        );
        // Neither case nor separators are folded.
        check(b"[a/b]\n[a\\b]\n", "a/b:; a\\\\b:");
    }

    #[test]
    fn rule_10_listings_are_in_ascending_byte_order_each_name_once() {
        check(b"[a]\nzebra\nape\nzebra\n", "a: ape(), zebra()");
        check(b"[b]\n[B]\n[10]\n[a]\n[\xE9]\n", "10:; B:; a:; b:; \\xe9:");
        // A name that is the start of a longer one comes first.
        check(b"[a]\nab\na\nB\n1\n", "a: 1(), B(), a(), ab()");
        // The class chain of `[Member]` is the keys of the section.
        check(
            b"[Member]\nentity\nunit\nztunit\nanimals\n",
            "Member: animals(), entity(), unit(), ztunit()",
        );
    }

    #[test]
    fn rule_12_a_file_without_section_is_not_loaded() {
        assert!(Config::parse(b"; only a comment\n").unwrap().is_empty());
        assert!(Config::parse(b"").unwrap().is_empty());
        assert!(Config::parse(b"k=1\n[]\n[x\n").unwrap().is_empty());
        assert!(!Config::parse(b"[a]").unwrap().is_empty());
        assert!(!Config::parse(b"[a]\nk=v").unwrap().is_empty());
    }

    #[test]
    fn nothing_is_an_error_but_the_size() {
        // A zero byte is white space at the ends and cuts the text inside (a ponytail choice).
        check(b"[a]\nk\0=v\0\0\nj=x\0y\nm\0n=1\n", "a: j(x), k(v), m(1)");
        check(b"\x00\x01\x02[a]\n", "a:");
        assert!(
            Config::parse(&vec![b'['; MAX_INPUT_LEN])
                .unwrap()
                .is_empty()
        );
        let err = Config::parse(&vec![b'a'; MAX_INPUT_LEN + 1]).unwrap_err();
        assert_eq!(
            err,
            TooLarge {
                len: MAX_INPUT_LEN + 1
            }
        );
        assert!(err.to_string().starts_with("1048577 bytes"));
    }

    /// Truncated and mutated copies of a valid text never panic, and many of them reach entries.
    #[test]
    fn mutated_input_never_panics() {
        let text = b"[a]\r\nk = v\r\nk=2\n[b/c]\n; c\nz\n[a] x=1\n=5\n[]\nq\n[d\n[e]\nf==g\n";
        let mut state = 12345u32;
        let mut next = move || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 16) as usize
        };
        let mut with_entries = 0;
        for len in 0..=text.len() {
            let _ = Config::parse(&text[..len]);
        }
        for _ in 0..2000 {
            let mut bytes = text.to_vec();
            for _ in 0..1 + next() % 4 {
                let at = next() % bytes.len();
                bytes[at] = [b'[', b']', b'=', b'\n', 0, 0xFF, b';'][next() % 7];
            }
            let config = Config::parse(&bytes).unwrap();
            with_entries += config.sections().any(|(_, s)| s.keys().next().is_some()) as usize;
        }
        assert!(with_entries > 1500, "{with_entries}");
    }

    /// A long section name must not make each entry cost its length.
    #[test]
    fn a_long_section_name_costs_nothing_per_entry() {
        let mut text = b"[".to_vec();
        text.extend(std::iter::repeat_n(b'n', 400_000));
        text.extend_from_slice(b"]\n");
        text.extend(std::iter::repeat_n(*b"a\n", 300_000).flatten());
        let config = Config::parse(&text).unwrap();
        assert_eq!(config.sections().next().unwrap().1.keys().count(), 1);
    }

    #[test]
    fn integers() {
        for (text, expected) in [
            ("90", Some(90)),
            ("-20;", Some(-20)),
            ("1;", Some(1)),
            ("+7", Some(7)),
            ("3.5", Some(3)),
            ("0.3", Some(0)),
            ("2surfsic", Some(2)),
            ("center", Some(0)),
            ("--5", Some(0)),
            ("0x1F", Some(31)),
            ("0x1f", Some(31)),
            ("0X1F", Some(0)),
            ("0xFFFFFFFF", Some(-1)),
            ("0x", None),
            ("0xg", None),
            ("0x1Fz", Some(31)),
            ("0x123456789", Some(0x2345_6789)),
            ("0x1FFFFFFFFF", Some(-1)),
            ("4294967297", Some(1)),
            ("", Some(0)),
            ("-", Some(0)),
        ] {
            assert_eq!(to_int(text.as_bytes()), expected, "{text:?}");
        }
        assert_eq!(to_int(b"-2147483648"), Some(i32::MIN));
    }

    #[test]
    fn decimal_numbers() {
        for (text, expected) in [
            ("90", 90.0),
            ("-20;", -20.0),
            ("1;", 1.0),
            ("+7", 7.0),
            ("3.5", 3.5),
            ("0.3", 0.3),
            (".5", 0.5),
            ("5.", 5.0),
            ("2surfsic", 2.0),
            ("center", 0.0),
            ("--5", 0.0),
            (".", 0.0),
            ("0x1F", 0.0),
            ("1e3", 1000.0),
            ("1.5E-1x", 0.15),
            ("2e", 2.0),
            ("2e+", 2.0),
            ("4294967297", 4294967296.0),
            ("", 0.0),
        ] {
            assert_eq!(to_float(text.as_bytes()), expected, "{text:?}");
        }
    }

    #[test]
    fn flags_are_integers_tested_against_zero() {
        for (text, expected) in [
            ("1", Some(true)),
            ("2", Some(true)),
            ("-1", Some(true)),
            ("0", Some(false)),
            ("yes", Some(false)),
            ("true", Some(false)),
            ("0x", None),
        ] {
            assert_eq!(to_flag(text.as_bytes()), expected, "{text:?}");
        }
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        use crate::vfs::Vfs;
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
        let read = |path: &str| Config::parse(&vfs.read(path).unwrap()).unwrap();

        // `[2100]` stands twice; the two parts read as one section with all three ids.
        let scenario = read("scenario/scn28b/scn28b.scn");
        let ids = scenario.section("2100").unwrap().get_all("id");
        assert_eq!(ids, [b"1002", b"3251", b"9300"]);

        // `Subtype =` is a key without value.
        let lion = read("animals/lion.ai");
        let global = lion.section("Global").unwrap();
        assert_eq!(global.get("Type"), Some(&b"lion"[..]));
        assert_eq!(global.get("Subtype"), None);
        assert_eq!(
            global.keys().collect::<Vec<_>>(),
            [&b"Class"[..], b"DefaultSubtype", b"Subtype", b"Type"]
        );
        let integers = lion.section("f/Characteristics/Integers").unwrap();
        assert_eq!(integers.get_all("cPrey").len(), 62);
        assert_eq!(integers.get("cPrey").map(to_int), Some(Some(9503)));

        // The class chain is four bare lines in file order, listed in byte order.
        let animals = read("animals.ai");
        let member = animals.section("Member").unwrap();
        assert_eq!(
            member.keys().collect::<Vec<_>>(),
            [&b"animals"[..], b"entity", b"unit", b"ztunit"]
        );
        assert_eq!(
            lion.section("Member").unwrap().keys().collect::<Vec<_>>(),
            [b"animals"]
        );

        // `help.cfg` writes `topic=Animation settings` but names the section with a capital S.
        let help = read("help.cfg");
        assert!(help.section("Animation Settings").is_some());
        assert!(help.section("Animation settings").is_none());

        // `scenario/scn01/economy.cfg` holds one comment: no section.
        assert!(read("scenario/scn01/economy.cfg").is_empty());
        let (mut sections, mut keys, mut values, mut empty) = (0, 0, 0, 0);
        for path in vfs.list(None) {
            let is_config = [
                ".ai", ".cfg", ".ani", ".scn", ".lyt", ".uca", ".ucb", ".ucs", ".ini",
            ]
            .iter()
            .any(|ext| path.ends_with(ext));
            if !is_config {
                continue;
            }
            let config = read(&path);
            empty += usize::from(config.is_empty());
            assert_eq!(
                config.ignored_before_first_header + config.ignored_after_broken_header,
                0
            );
            for (_, section) in config.sections() {
                sections += 1;
                for key in section.keys() {
                    keys += 1;
                    values += section.get_all(key).len();
                }
            }
        }
        assert_eq!(
            (sections, keys, values, empty),
            (33_621, 202_459, 300_138, 3)
        );
    }

    #[test]
    fn an_overlay_replaces_keys_ignoring_case_and_keeps_the_rest() {
        let mut game =
            Config::parse(b"[User]\nFullScreen=0\nscreenwidth=800\n[UI]\nx=1\nx=2\n").unwrap();
        game.overlay(
            &Config::parse(b"[user]\nfullscreen=1\nnew=5\n[UI]\nx=3\n[more]\nk\n").unwrap(),
        );
        assert_eq!(
            dump(&game.to_bytes()),
            "UI: x(3); User: FullScreen(1), new(5), screenwidth(800); more: k()"
        );
    }

    #[test]
    fn set_creates_and_replaces_and_survives_a_round_trip() {
        let mut config = Config::default();
        config.set("user", "fullscreen", "1");
        config.set("USER", "FULLSCREEN", "-7");
        config.set("user", "huge", "99999999999999999999");
        let back = Config::parse(&config.to_bytes()).unwrap();
        assert_eq!(back, config);
        assert_eq!(
            dump(&config.to_bytes()),
            "user: fullscreen(-7), huge(99999999999999999999)"
        );
    }
}
