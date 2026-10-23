//! The safety net between the model and the user. The model reads secrets next to their handles
//! and is told to write the handle, but local models also copy values, and corrupt long random
//! ones (JWTs, DSNs) when they do. Before a reply is shown or stored:
//! - `[n12.0: value]` becomes `[n12.0]`, whatever the model wrote after the colon;
//! - a known secret written out becomes its handle, so the app shows it from the vault;
//! - a near-copy of a known secret (length within 2, at least 80% alike) becomes the handle of
//!   the true value, and a near-copy of a long copyable value becomes that value.

use zeroize::Zeroizing;

use super::handles::Handle;

/// Values shorter than this are compared only when they hold more than letters.
const MIN_WORDLIKE: usize = 8;
/// Shortest secret replaced when written out verbatim.
const MIN_VERBATIM: usize = 4;
/// Shortest value a near-copy is looked for; shorter ones are too alike by chance.
const MIN_NEAR: usize = 8;
/// Shortest copyable value whose near-copies are corrected.
const MIN_NEAR_COPYABLE: usize = 12;
/// Longest length difference of a near-copy.
const MAX_LEN_DIFF: usize = 2;

/// What the model may have read in this reply.
#[derive(Default)]
pub struct Known {
    /// Handle text and value, longest value first.
    secrets: Vec<(String, Zeroizing<String>)>,
    copyable: Vec<String>,
}

impl Known {
    pub fn new(secrets: Vec<(Handle, Zeroizing<String>)>, copyable: Vec<String>) -> Self {
        let mut secrets: Vec<(String, Zeroizing<String>)> = secrets
            .into_iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(handle, value)| (handle.to_string(), value))
            .collect();
        secrets.sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
        Self { secrets, copyable }
    }

    fn handle_value(&self, handle: &str) -> Option<&str> {
        self.secrets
            .iter()
            .find(|(h, _)| h == handle)
            .map(|(_, v)| v.as_str())
    }
}

/// The copyable values (`` `value` ``) of a template; secret markers aren't.
pub fn copyable_spans(template: &str) -> impl Iterator<Item = &str> {
    template
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| !span.is_empty() && !(span.starts_with('!') && span.ends_with('!')))
}

/// A finished reply as the user may see it.
pub fn redact(text: &str, known: &Known) -> String {
    if known.secrets.is_empty() && known.copyable.is_empty() {
        return text.to_string();
    }
    let text = drop_values(text, known);
    let text = verbatim(&text, known);
    near_copies(&text, known)
}

/// A reply still being written: like `redact`, and a word still growing at the end is held back
/// until it is whole, so a value never shows half typed.
pub fn redact_partial(text: &str, known: &Known) -> String {
    let mut end = text.len();
    if !text.ends_with(char::is_whitespace) {
        end = text.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    }
    // A handle whose value is still being written.
    if let Some(open) = text[..end].rfind('[')
        && !text[open..end].contains(']')
    {
        end = open;
    }
    redact(&text[..end], known)
}

/// `[n12.0: anything]` → `[n12.0]`.
fn drop_values(text: &str, known: &Known) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(colon) = after.find(": ") else {
            out.push_str(after);
            return out;
        };
        let Ok(handle) = Handle::parse(&after[1..colon]) else {
            out.push('[');
            rest = &after[1..];
            continue;
        };
        let handle = handle.to_string();
        let value_start = colon + 2;
        // The exact value may hold a `]`; anything else ends at the first one on its line.
        let exact = known
            .handle_value(&handle)
            .map(|v| format!("{v}]"))
            .filter(|v| after[value_start..].starts_with(v.as_str()));
        let end = match exact {
            Some(v) => Some(value_start + v.len()),
            None => after[value_start..]
                .find([']', '\n'])
                .filter(|&i| after.as_bytes()[value_start + i] == b']')
                .map(|i| value_start + i + 1),
        };
        match end {
            Some(end) => {
                out.push_str(&handle);
                rest = &after[end..];
            }
            None => {
                out.push('[');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every known secret written out becomes its handle, with the backticks or `!` around it.
fn verbatim(text: &str, known: &Known) -> String {
    let mut text = text.to_string();
    for (handle, value) in &known.secrets {
        let value = value.trim();
        let len = value.chars().count();
        let wordlike = value.chars().all(char::is_alphabetic);
        if len < MIN_VERBATIM || (wordlike && len < MIN_WORDLIKE) || !text.contains(value) {
            continue;
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find(value) {
            let before = &rest[..at];
            let after = &rest[at + value.len()..];
            let joined = before
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                || after.chars().next().is_some_and(char::is_alphanumeric);
            if joined {
                out.push_str(&rest[..at + value.len()]);
                rest = after;
                continue;
            }
            let (before, after) = unwrap_marks(before, after);
            out.push_str(before);
            out.push_str(handle);
            rest = after;
        }
        out.push_str(rest);
        text = out;
    }
    text
}

/// Drops `` ` ``, `` `! `` / `` !` `` around a value that becomes a handle.
fn unwrap_marks<'a>(before: &'a str, after: &'a str) -> (&'a str, &'a str) {
    for (open, close) in [("`!", "!`"), ("`", "`"), ("**", "**")] {
        if before.ends_with(open) && after.starts_with(close) {
            return (&before[..before.len() - open.len()], &after[close.len()..]);
        }
    }
    (before, after)
}

/// Words that look like a corrupted copy of a known value become the true one.
fn near_copies(text: &str, known: &Known) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let space = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..space];
        out.push_str(&near_word(word, known).unwrap_or_else(|| word.to_string()));
        let after = &rest[space..];
        let next = after
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after.len());
        out.push_str(&after[..next]);
        rest = &after[next..];
    }
    out
}

/// A word, or its value after `KEY=` / `label:`, replaced when it is a near-copy.
fn near_word(word: &str, known: &Known) -> Option<String> {
    let start = word
        .find(|c: char| !matches!(c, '`' | '"' | '\'' | '(' | '<' | '*' | '!'))
        .unwrap_or(word.len());
    let end = word
        .rfind(|c: char| {
            !matches!(
                c,
                '`' | '"' | '\'' | ')' | '>' | '*' | '!' | ',' | '.' | ';'
            )
        })
        .map_or(start, |i| {
            i + word[i..].chars().next().map_or(1, char::len_utf8)
        });
    if start >= end {
        return None;
    }
    let core = &word[start..end];
    if core.starts_with('[') {
        return None;
    }
    let candidates = std::iter::once(0).chain(
        core.find(['=', ':'])
            .filter(|_| !core.contains("://"))
            .map(|i| i + 1),
    );
    for offset in candidates {
        let candidate = &core[offset..];
        if let Some(replacement) = nearest(candidate, known) {
            let (before, after) = (&word[..start + offset], &word[end..]);
            return Some(match replacement {
                Near::Secret(handle) => {
                    let (before, after) = unwrap_marks(before, after);
                    format!("{before}{handle}{after}")
                }
                Near::Copyable(value) => format!("{before}{value}{after}"),
            });
        }
    }
    None
}

enum Near<'a> {
    Secret(&'a str),
    Copyable(&'a str),
}

fn nearest<'a>(candidate: &str, known: &'a Known) -> Option<Near<'a>> {
    let chars: Vec<char> = candidate.chars().collect();
    let len = chars.len();
    if len < MIN_NEAR
        || known.copyable.iter().any(|v| v == candidate)
        || known.secrets.iter().any(|(_, v)| v.as_str() == candidate)
    {
        return None;
    }
    let mut best: Option<(usize, Near)> = None;
    let mut consider = |value: &str, min: usize, near: Near<'a>| {
        let value_len = value.chars().count();
        if value_len < min || value_len.abs_diff(len) > MAX_LEN_DIFF || value.contains('\n') {
            return;
        }
        let limit = value_len.max(len) / 5;
        let Some(distance) = distance(&chars, value, limit) else {
            return;
        };
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, near));
        }
    };
    for (handle, value) in &known.secrets {
        consider(value, MIN_NEAR, Near::Secret(handle));
    }
    for value in &known.copyable {
        consider(value, MIN_NEAR_COPYABLE, Near::Copyable(value));
    }
    best.map(|(_, near)| near)
}

/// Edit distance between `a` and `b` (a swap of neighbours counts once), if at most `limit`.
fn distance(a: &[char], b: &str, limit: usize) -> Option<usize> {
    let b: Vec<char> = b.chars().collect();
    let mut before: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut row = vec![0; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        row[0] = i + 1;
        let mut lowest = row[0];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let mut d = (prev[j] + cost).min(prev[j + 1] + 1).min(row[j] + 1);
            if i > 0 && j > 0 && ca == b[j - 1] && a[i - 1] == cb {
                d = d.min(before[j - 1] + 1);
            }
            row[j + 1] = d;
            lowest = lowest.min(d);
        }
        if lowest > limit {
            return None;
        }
        std::mem::swap(&mut before, &mut prev);
        std::mem::swap(&mut prev, &mut row);
    }
    Some(prev[b.len()]).filter(|&d| d <= limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::handles::Source;

    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhY21lLWRlcGxveSIsImlhdCI6MTcyOTY0MDAwMCwic2NvcGUiOiJyZWFkOmFsbCB3cml0ZTpkZXBsb3kifQ.Qm9vbGVhbi1kZW1vLXNpZ25hdHVyZS1ub3QtcmVhbC0xMjM0NTY";
    const API_KEY: &str = "sk_live_51NzQvLKq8Xh3mT2pWcR9bYfA7dE4gJ6uV0sZ";
    const PASSWORD: &str = "Tr0ub4dor&3-demo";

    fn handle(id: i64, index: Option<usize>) -> Handle {
        Handle {
            source: Source::Note,
            id,
            index,
        }
    }

    fn known() -> Known {
        Known::new(
            vec![
                (handle(1, Some(0)), Zeroizing::new(PASSWORD.to_string())),
                (handle(2, Some(0)), Zeroizing::new(API_KEY.to_string())),
                (handle(3, Some(1)), Zeroizing::new(JWT.to_string())),
                (handle(4, Some(0)), Zeroizing::new("4242".to_string())),
            ],
            vec!["https://acme-demo.signin.aws.amazon.com/console".to_string()],
        )
    }

    /// Changes a few characters the way a model miscopies: one dropped, one doubled, one swapped.
    fn corrupt(value: &str) -> String {
        let mut chars: Vec<char> = value.chars().collect();
        let n = chars.len();
        chars.remove(n / 3);
        chars.insert(n / 2, chars[n / 2]);
        chars.swap(2 * n / 3, 2 * n / 3 + 1);
        chars.into_iter().collect()
    }

    #[test]
    fn handles_with_values_lose_the_value() {
        let k = known();
        assert_eq!(
            redact(&format!("Password: [n1.0: {PASSWORD}]"), &k),
            "Password: [n1.0]"
        );
        assert_eq!(
            redact("Key: [n2.0: sk_live_WRONG] and [n9.9: who knows]", &k),
            "Key: [n2.0] and [n9.9]"
        );
        assert_eq!(redact("a [link](x) and [n1]", &k), "a [link](x) and [n1]");
    }

    #[test]
    fn verbatim_secrets_become_handles() {
        let k = known();
        assert_eq!(
            redact(&format!("The password is `{PASSWORD}`."), &k),
            "The password is [n1.0]."
        );
        assert_eq!(
            redact(&format!("key **{API_KEY}** and token\n{JWT}"), &k),
            "key [n2.0] and token\n[n3.1]"
        );
        assert_eq!(
            redact("PIN `!4242!`, not 142420", &k),
            "PIN [n4.0], not 142420"
        );
    }

    #[test]
    fn near_copies_become_the_true_value() {
        let k = known();
        for (value, handle) in [(API_KEY, "[n2.0]"), (JWT, "[n3.1]"), (PASSWORD, "[n1.0]")] {
            let bad = corrupt(value);
            assert_ne!(bad, value);
            assert_eq!(
                redact(&format!("Here: `{bad}`."), &k),
                format!("Here: {handle}."),
                "{bad}"
            );
            assert_eq!(
                redact(&format!("TOKEN={bad}"), &k),
                format!("TOKEN={handle}")
            );
        }
        // A copy that lost its end: two characters shorter.
        let short = &API_KEY[..API_KEY.len() - 2];
        assert_eq!(redact(short, &k), "[n2.0]");
        assert_eq!(
            redact(
                "Console: `https://acme-demo.signin.aws.amazon.con/console`",
                &k
            ),
            "Console: `https://acme-demo.signin.aws.amazon.com/console`"
        );
    }

    #[test]
    fn leaves_other_words_alone() {
        let k = known();
        for text in [
            "Use the AWS console with user apo.deploy, region eu-central-1.",
            "sk_live_99999999999999999999999999999999999999",
            "https://acme-demo.signin.aws.amazon.com/console",
            "Troubleshooting the deployment",
            "",
        ] {
            assert_eq!(redact(text, &k), text);
        }
        // Too far off: not a copy of the password.
        assert_eq!(redact("Tr0ub4dor&3-prod-x", &k), "Tr0ub4dor&3-prod-x");
    }

    #[test]
    fn partial_replies_hold_back_the_growing_word() {
        let k = known();
        assert_eq!(redact_partial("The key is sk_live_51Nz", &k), "The key is ");
        assert_eq!(redact_partial("Password: [n1.0: Tr0u", &k), "Password: ");
        assert_eq!(
            redact_partial(&format!("Key {API_KEY} and "), &k),
            "Key [n2.0] and "
        );
    }

    #[test]
    fn finds_copyable_spans() {
        let spans: Vec<&str> = copyable_spans("User: `apo` pass `!0!` and `x y`").collect();
        assert_eq!(spans, ["apo", "x y"]);
    }
}
