//! Inline markup inside item bodies and project overviews:
//! - `` `value` `` is a copyable value (plain text, handled by the frontend).
//! - `` `!value!` `` is a secret: the backtick and `!` must touch on both sides. `` ` !x! ` ``
//!   is just a copyable value.
//!
//! Secrets are stored as a template where each one becomes `` `!0!` ``, `` `!1!` ``, … plus an
//! encrypted list of their values. Everything else stays readable without Touch ID.
//! Mirrors `src/lib/markup.ts`.

const TICK: char = '`';
const BANG: char = '!';

/// A backtick span on one line: (start, end) byte offsets of the content, excluding ticks.
/// Scans like the frontend's /`([^`\n]+)`/g: on a failed match it retries one char later.
fn next_span(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while let Some(offset) = text[i..].find(TICK) {
        let open = i + offset;
        let start = open + 1;
        match text[start..].find(['`', '\n']) {
            Some(len) if len > 0 && text[start + len..].starts_with(TICK) => {
                return Some((start, start + len));
            }
            _ => i = start,
        }
    }
    None
}

/// The secret inside a span's content, if it is one (`!value!` with at least one char).
fn as_secret(content: &str) -> Option<&str> {
    let inner = content.strip_prefix(BANG)?.strip_suffix(BANG)?;
    (!inner.is_empty()).then_some(inner)
}

/// Splits a body into a template and its secrets, in order.
pub fn extract(body: &str) -> (String, Vec<String>) {
    let mut template = String::with_capacity(body.len());
    let mut secrets = Vec::new();
    let mut last = 0;
    let mut from = 0;
    while let Some((start, end)) = next_span(body, from) {
        if let Some(secret) = as_secret(&body[start..end]) {
            template.push_str(&body[last..start]);
            template.push_str(&format!("{BANG}{}{BANG}", secrets.len()));
            secrets.push(secret.to_string());
            last = end;
        }
        from = end + 1;
    }
    template.push_str(&body[last..]);
    (template, secrets)
}

/// Inverse of [`extract`]: replaces each `` `!n!` `` marker with its secret.
pub fn restore(template: &str, secrets: &[String]) -> String {
    let mut body = String::with_capacity(template.len());
    let mut last = 0;
    let mut from = 0;
    while let Some((start, end)) = next_span(template, from) {
        let secret = as_secret(&template[start..end])
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| secrets.get(index));
        if let Some(secret) = secret {
            body.push_str(&template[last..start]);
            body.push(BANG);
            body.push_str(secret);
            body.push(BANG);
            last = end;
        }
        from = end + 1;
    }
    body.push_str(&template[last..]);
    body
}

/// A template with its `` `!n!` `` markers removed: the text anyone can read without a key.
pub fn strip_markers(template: &str) -> String {
    replace_markers(template, |_| String::new())
}

/// A template with each `` `!n!` `` marker (ticks included) replaced by `with(n)`.
pub fn replace_markers(template: &str, mut with: impl FnMut(usize) -> String) -> String {
    let mut text = String::with_capacity(template.len());
    let mut last = 0;
    let mut from = 0;
    while let Some((start, end)) = next_span(template, from) {
        let index = as_secret(&template[start..end]).and_then(|index| index.parse().ok());
        if let Some(index) = index {
            text.push_str(&template[last..start - 1]);
            text.push_str(&with(index));
            last = end + 1;
        }
        from = end + 1;
    }
    text.push_str(&template[last..]);
    text
}

/// Rewrites the `{{n}}` markers used before migration 3 into `` `!n!` ``.
pub fn upgrade_legacy_markers(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let digits = after.find("}}").map(|end| (&after[..end], end));
        match digits {
            Some((index, end))
                if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) =>
            {
                out.push_str(&format!("{TICK}{BANG}{index}{BANG}{TICK}"));
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared with `src/lib/markup.test.ts`, so both parsers agree on every case.
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        body: String,
        template: String,
        secrets: Vec<String>,
    }

    #[test]
    fn matches_shared_fixtures() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../../fixtures/markup.json")).unwrap();
        for case in cases {
            let (template, secrets) = extract(&case.body);
            assert_eq!(template, case.template, "{}", case.name);
            assert_eq!(secrets, case.secrets, "{}", case.name);
            assert_eq!(restore(&template, &secrets), case.body, "{}", case.name);
        }
    }

    #[test]
    fn round_trips_secrets() {
        let body = "user: `apo`\npass: `!hunter 2!`\ntoken: `!abc!`!";
        let (template, secrets) = extract(body);
        assert_eq!(template, "user: `apo`\npass: `!0!`\ntoken: `!1!`!");
        assert_eq!(secrets, vec!["hunter 2", "abc"]);
        assert_eq!(restore(&template, &secrets), body);
    }

    #[test]
    fn spaced_or_empty_spans_are_not_secrets() {
        let body = "a ` !x! ` b ` ! y ! ` c `!!` d `!` e `unclosed\n`x`";
        let (template, secrets) = extract(body);
        assert_eq!(template, body);
        assert!(secrets.is_empty());
    }

    #[test]
    fn strips_markers_but_keeps_other_spans() {
        assert_eq!(
            strip_markers("pass: `!0!` user: `apo` note `!x!` end`!12!`"),
            "pass:  user: `apo` note `!x!` end"
        );
    }

    #[test]
    fn upgrades_legacy_markers() {
        assert_eq!(
            upgrade_legacy_markers("p: {{0}} and {{12}} but {{x}} {{"),
            "p: `!0!` and `!12!` but {{x}} {{"
        );
    }
}
