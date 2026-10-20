//! Inline secrets: `{{...}}` spans inside a plain item body.
//!
//! Stored as a template where each span becomes `{{0}}`, `{{1}}`, … plus an encrypted list of
//! the span contents. Everything outside the spans stays readable without Touch ID.

const OPEN: &str = "{{";
const CLOSE: &str = "}}";

/// Splits a body into a template and its secret spans, in order. `{{}}` stays literal.
pub fn extract(body: &str) -> (String, Vec<String>) {
    let mut template = String::with_capacity(body.len());
    let mut secrets = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else { break };
        let secret = &after[..end];
        template.push_str(&rest[..start]);
        if secret.is_empty() {
            template.push_str(OPEN);
            template.push_str(CLOSE);
        } else {
            template.push_str(&format!("{OPEN}{}{CLOSE}", secrets.len()));
            secrets.push(secret.to_string());
        }
        rest = &after[end + CLOSE.len()..];
    }
    template.push_str(rest);
    (template, secrets)
}

/// Inverse of [`extract`]: puts each secret back as a `{{secret}}` span.
pub fn restore(template: &str, secrets: &[String]) -> String {
    let mut body = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find(OPEN) {
        body.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let secret = after
            .find(CLOSE)
            .and_then(|end| after[..end].parse::<usize>().ok().map(|i| (i, end)))
            .and_then(|(i, end)| secrets.get(i).map(|s| (s, end)));
        match secret {
            Some((secret, end)) => {
                body.push_str(OPEN);
                body.push_str(secret);
                body.push_str(CLOSE);
                rest = &after[end + CLOSE.len()..];
            }
            None => {
                body.push_str(OPEN);
                rest = after;
            }
        }
    }
    body.push_str(rest);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_secrets() {
        let body = "user: apo\npass: {{hunter2}}\ntoken: {{abc}}}";
        let (template, secrets) = extract(body);
        assert_eq!(template, "user: apo\npass: {{0}}\ntoken: {{1}}}");
        assert_eq!(secrets, vec!["hunter2", "abc"]);
        assert_eq!(restore(&template, &secrets), body);
    }

    #[test]
    fn leaves_empty_and_unclosed_spans_literal() {
        let body = "a {{}} b {{ never closed";
        let (template, secrets) = extract(body);
        assert_eq!(template, body);
        assert!(secrets.is_empty());
        assert_eq!(restore(&template, &secrets), body);
    }
}
