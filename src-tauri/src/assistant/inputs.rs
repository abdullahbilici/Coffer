//! Input handles keep the values a user types or pastes away from the model. Before a user
//! message reaches it, every value-like token (a backtick span, a URL, an email, a long token
//! mixing letters with digits or symbols) is replaced by a handle with a short hint, such as
//! `[u3: sk_live_…, 32 chars]`. Write tools take the handle, and the server puts the real value
//! in when a draft is applied. Local models corrupt long tokens when copying them, so a value
//! never passes through one.
//!
//! Handles are numbered across a conversation's user messages in order, so they can be rebuilt
//! from the stored (sealed) messages at any time.

use std::fmt::Write as _;

/// The values of a conversation's user messages, `u1` first.
#[derive(Debug, Default)]
pub struct Inputs {
    values: Vec<String>,
    /// What the model saw of each message, for checking short values it repeats verbatim.
    shown: Vec<String>,
    /// Writes values as a short masked form instead of handles (`masked`).
    masking: bool,
}

/// Values the model sees as they are; anything shorter is rarely worth hiding.
const MIN_LEN: usize = 6;

/// Prefixes that say what a key is without giving any of it away.
const PREFIXES: [&str; 22] = [
    "sk_live_",
    "sk_test_",
    "pk_live_",
    "pk_test_",
    "rk_live_",
    "rk_test_",
    "whsec_",
    "sk-proj-",
    "sk-ant-",
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "AKIA",
    "AIza",
    "SG.",
    "eyJ",
    "ya29.",
    "npm_",
];

impl Inputs {
    /// Splits the values out of `text`, numbering them after the ones already taken, and
    /// returns what the model should read instead.
    pub fn take(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            self.take_line(line, &mut out);
        }
        self.shown.push(out.clone());
        out
    }

    fn take_line(&mut self, line: &str, out: &mut String) {
        let mut rest = line;
        while !rest.is_empty() {
            // A backtick span is a value whatever it looks like.
            if let Some(after) = rest.strip_prefix('`')
                && let Some(end) = after.find('`')
                && end > 0
            {
                let value = after[..end].trim_matches('!');
                if value.is_empty() {
                    out.push_str(&rest[..end + 2]);
                } else {
                    self.push(value, out);
                }
                rest = &after[end + 1..];
                continue;
            }
            let space = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let first = rest.chars().next().map_or(0, char::len_utf8);
            let next_tick = rest[first..].find('`').map_or(rest.len(), |i| i + first);
            let end = space.min(next_tick);
            if end == 0 {
                let c = rest.chars().next().expect("rest is not empty");
                out.push(c);
                rest = &rest[c.len_utf8()..];
                continue;
            }
            self.take_word(&rest[..end], out);
            rest = &rest[end..];
        }
    }

    /// One whitespace-delimited word: a value, a `KEY=value` / `label:value` pair, or text.
    fn take_word(&mut self, word: &str, out: &mut String) {
        let (open, inner, close) = unwrap(word);
        out.push_str(open);
        if let Some((key, value)) = split_pair(inner) {
            out.push_str(key);
            self.push(value, out);
        } else if is_value(inner) {
            let (value, tail) = trailing(inner);
            self.push(value, out);
            out.push_str(tail);
        } else {
            out.push_str(inner);
        }
        out.push_str(close);
    }

    fn push(&mut self, value: &str, out: &mut String) {
        if self.masking {
            out.push_str(&short(value));
            return;
        }
        let n = match self.values.iter().position(|v| v == value) {
            Some(i) => i + 1,
            None => {
                self.values.push(value.to_string());
                self.values.len()
            }
        };
        let _ = write!(out, "[u{n}: {}]", hint(value));
    }

    /// The value behind `u3` (or `[u3]`, `u3: …`).
    pub fn get(&self, n: usize) -> Option<&str> {
        n.checked_sub(1)
            .and_then(|i| self.values.get(i))
            .map(String::as_str)
    }

    /// Whether the model saw `text` as it is in a user message (as whole words), so taking it
    /// back can't corrupt it.
    pub fn shown_verbatim(&self, text: &str) -> bool {
        !text.is_empty()
            && self.shown.iter().any(|shown| {
                shown.match_indices(text).any(|(i, _)| {
                    let before = shown[..i].chars().next_back();
                    let after = shown[i + text.len()..].chars().next();
                    !before.is_some_and(char::is_alphanumeric)
                        && !after.is_some_and(char::is_alphanumeric)
                })
            })
    }
}

/// `text` with its values masked, e.g. for a conversation title: "add sk_test_… to acme".
pub fn masked(text: &str) -> String {
    let mut inputs = Inputs {
        masking: true,
        ..Inputs::default()
    };
    inputs.take(text)
}

/// A value's masked form: a known key prefix, a URL's host or an email's domain, then "…".
fn short(value: &str) -> String {
    if let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    {
        let host = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
        return host.rsplit('@').next().unwrap_or(host).to_string();
    }
    if is_email(value)
        && let Some((_, domain)) = value.split_once('@')
    {
        return format!("…@{domain}");
    }
    match PREFIXES.iter().find(|p| value.starts_with(*p)) {
        Some(prefix) => format!("{prefix}…"),
        None => "…".to_string(),
    }
}

/// Parses `u3`, `[u3]` or `[u3: sk_live_…, 32 chars]` into 3.
pub fn parse(text: &str) -> Option<usize> {
    let text = text.trim().trim_start_matches('`').trim_end_matches('`');
    let text = text.strip_prefix('[').unwrap_or(text);
    let text = text.split([']', ':']).next()?.trim();
    text.strip_prefix('u')?.parse().ok().filter(|&n| n > 0)
}

/// What the model may know about a value: its kind and length, never its characters, except
/// for well-known key prefixes and a URL's or email's host.
fn hint(value: &str) -> String {
    let len = value.chars().count();
    if let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    {
        let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let host = &rest[..host_end];
        // Credentials in a URL stay hidden.
        let host = host.rsplit('@').next().unwrap_or(host);
        let scheme = &value[..value.len() - rest.len()];
        let more = if host_end < rest.len() { "/…" } else { "" };
        return format!("url {scheme}{host}{more}, {len} chars");
    }
    if let Some((_, domain)) = value.split_once('@')
        && is_email(value)
    {
        return format!("email …@{domain}");
    }
    if let Some(prefix) = PREFIXES.iter().find(|p| value.starts_with(*p)) {
        return format!("{prefix}…, {len} chars");
    }
    format!("{len} chars")
}

fn is_email(text: &str) -> bool {
    let Some((user, domain)) = text.split_once('@') else {
        return false;
    };
    !user.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !text.contains(char::is_whitespace)
}

/// Quotes, brackets and a trailing comma around a word are not part of its value.
fn unwrap(word: &str) -> (&str, &str, &str) {
    let mut inner = word;
    let mut open_len = 0;
    let mut close_len = 0;
    for (open, close) in [('"', '"'), ('\'', '\''), ('(', ')'), ('<', '>'), ('[', ']')] {
        if inner.len() >= 2 && inner.starts_with(open) && inner.ends_with(close) {
            inner = &inner[1..inner.len() - 1];
            open_len += 1;
            close_len += 1;
        }
    }
    let trimmed = inner.trim_end_matches(',');
    close_len += inner.len() - trimmed.len();
    inner = trimmed;
    let end = word.len() - close_len.min(word.len());
    (&word[..open_len], inner, &word[end.max(open_len)..])
}

/// `KEY=value` or `label:value` written without a space: the value part, if it is one.
fn split_pair(word: &str) -> Option<(&str, &str)> {
    if word.starts_with("http://") || word.starts_with("https://") {
        return None;
    }
    let at = word.find(['=', ':'])?;
    let (key, value) = (&word[..=at], &word[at + 1..]);
    let name = &key[..at];
    let named = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
    let is_env = word.as_bytes()[at] == b'=';
    let worth = if is_env {
        value.chars().count() >= 4
    } else {
        is_value(value)
    };
    // `123456:AAx…` (a bot token) is one value, not a label.
    let label = name.chars().any(char::is_alphabetic);
    (named && label && !value.is_empty() && worth).then_some((key, value))
}

/// Whether a word looks like a value worth hiding.
fn is_value(word: &str) -> bool {
    if word.starts_with("http://") || word.starts_with("https://") {
        return word.len() > "https://".len();
    }
    if is_email(word) {
        return true;
    }
    if is_date(word) {
        return false;
    }
    let len = word.chars().count();
    let digits = word.chars().filter(char::is_ascii_digit).count();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let dots = word.matches('.').count();
    // Turkish suffixes after a number or a name: "5'inde", "Ali'de".
    if let Some((head, tail)) = word.split_once('\'')
        && !head.is_empty()
        && tail.chars().all(char::is_lowercase)
    {
        return false;
    }
    if letters == 0 {
        // An IP address, or a long number (account, phone); amounts and short PINs stay.
        return dots >= 3 || (digits >= 7 && dots == 0);
    }
    if len < MIN_LEN {
        return false;
    }
    let core = word.trim_end_matches(['.', ',', ':', ';', '!', '?']);
    let symbols = core
        .chars()
        .filter(|c| !c.is_alphanumeric() && !matches!(c, '-' | '\''))
        .count();
    digits > 0 || symbols > 0
}

/// A URL's sentence punctuation isn't part of it; other values keep every character.
fn trailing(word: &str) -> (&str, &str) {
    if word.starts_with("http://") || word.starts_with("https://") {
        let value = word.trim_end_matches(['.', ',', ';', ')', '!', '?']);
        return (value, &word[value.len()..]);
    }
    (word, "")
}

fn is_date(word: &str) -> bool {
    let b = word.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(text: &str) -> (String, Inputs) {
        let mut inputs = Inputs::default();
        let shown = inputs.take(text);
        (shown, inputs)
    }

    #[test]
    fn hides_values_and_keeps_words() {
        let (shown, inputs) = take(
            "add a Stripe test key sk_test_51HxYz9KqLmN0pQrStUv to Acme, pass: `hunter2` and \
             https://panel.example.com/login?token=abc123.",
        );
        assert_eq!(
            shown,
            "add a Stripe test key [u1: sk_test_…, 28 chars] to Acme, pass: [u2: 7 chars] and \
             [u3: url https://panel.example.com/…, 44 chars]."
        );
        assert_eq!(inputs.get(1), Some("sk_test_51HxYz9KqLmN0pQrStUv"));
        assert_eq!(inputs.get(2), Some("hunter2"));
        assert_eq!(
            inputs.get(3),
            Some("https://panel.example.com/login?token=abc123")
        );
        assert_eq!(inputs.get(4), None);
    }

    #[test]
    fn leaves_dates_amounts_and_plain_words() {
        for text in [
            "add a monthly 300 TRY hosting payment from 2026-11-01",
            "mark the Renew SSL todo done",
            "faturayı ayın 5'inde kesiyorlar, Ali'de de var",
            "move side-project notes into Infra.",
            "PIN 4242",
        ] {
            assert_eq!(take(text).0, text, "{text}");
        }
    }

    #[test]
    fn splits_pairs_and_dump_lines() {
        let (shown, inputs) = take(
            "AWS_PROD_SECRET=wJalrXUtnFEMI/K7MDENG\nşifre:Kedi123!\n| ssh | host | 203.0.113.9 |\nbot 1234567890:AAH-x_9yZ\nmail mavi.abcd@hostfast.example",
        );
        assert_eq!(
            shown,
            "AWS_PROD_SECRET=[u1: 21 chars]\nşifre:[u2: 8 chars]\n| ssh | host | [u3: 11 chars] |\nbot [u4: 20 chars]\nmail [u5: email …@hostfast.example]"
        );
        assert_eq!(inputs.get(2), Some("Kedi123!"));
        assert_eq!(inputs.get(4), Some("1234567890:AAH-x_9yZ"));
    }

    #[test]
    fn numbers_across_messages_and_reuses_repeats() {
        let mut inputs = Inputs::default();
        assert_eq!(inputs.take("key abc123def"), "key [u1: 9 chars]");
        assert_eq!(
            inputs.take("again abc123def and xyz789uvw"),
            "again [u1: 9 chars] and [u2: 9 chars]"
        );
        assert!(inputs.shown_verbatim("again"));
        assert!(!inputs.shown_verbatim("abc123def"));
        assert!(!inputs.shown_verbatim("gai"));
    }

    #[test]
    fn masks_values_for_titles() {
        assert_eq!(
            masked("add sk_test_51HxYz9KqLmN0pQrStUv and pass `hunter2` from https://x.example/a"),
            "add sk_test_… and pass … from x.example"
        );
    }

    #[test]
    fn parses_handles() {
        assert_eq!(parse("u3"), Some(3));
        assert_eq!(parse("[u12]"), Some(12));
        assert_eq!(parse("[u3: sk_live_…, 32 chars]"), Some(3));
        assert_eq!(parse("`[u3]`"), Some(3));
        assert_eq!(parse("u0"), None);
        assert_eq!(parse("n12.0"), None);
    }
}
