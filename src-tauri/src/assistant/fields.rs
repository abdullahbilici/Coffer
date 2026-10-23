//! Fields as the assistant sees them: a note body line such as ``Password: `!hunter2!` `` is
//! the field "Password". Bodies stay free markdown; these helpers find, add, change and remove
//! such lines. They work the same on a template (`` `!0!` `` markers) and on decrypted text, so
//! a draft checks a change against the readable template and applies it to the real body.

/// A field line's value, as markup.
pub fn span(value: &str, secret: bool) -> String {
    if secret {
        format!("`!{value}!`")
    } else {
        format!("`{value}`")
    }
}

pub fn line(label: &str, value: &str, secret: bool) -> String {
    format!("{}: {}", label.trim(), span(value, secret))
}

/// The label of a field line, as written: the text before its first colon (or its first value),
/// without list markers or emphasis. `None` for lines that hold no value.
pub fn label_of(line: &str) -> Option<&str> {
    let text = line
        .trim()
        .trim_start_matches(['-', '*', '+', '>', '#', '|', ' '])
        .trim_start();
    let end = match (text.find(':'), text.find('`')) {
        (Some(colon), Some(tick)) => colon.min(tick),
        (Some(colon), None) => colon,
        (None, Some(tick)) => tick,
        (None, None) => return None,
    };
    let label = text[..end].trim().trim_matches(['*', '_']).trim();
    let rest = text[end..].trim_start_matches(':').trim();
    let short = !label.is_empty() && label.chars().count() <= 60;
    (short && !rest.is_empty()).then_some(label)
}

/// Lowercase words only, so "API key", "api-key:" and "**Api Key**" compare equal.
pub fn normalize(label: &str) -> String {
    label
        .chars()
        .map(|c| match c {
            'ı' | 'İ' => 'i',
            c if c.is_alphanumeric() => c.to_lowercase().next().unwrap_or(c),
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every field label of a body, in order.
pub fn labels(body: &str) -> Vec<&str> {
    body.lines().filter_map(label_of).collect()
}

/// The line holding field `label`: an exact (normalized) match, else the only line whose label
/// contains it or is contained in it. On failure, the error lists the labels there are.
pub fn find(body: &str, label: &str) -> Result<usize, String> {
    let wanted = normalize(label);
    if wanted.is_empty() {
        return Err("label is empty".to_string());
    }
    let lines: Vec<(usize, String)> = body
        .lines()
        .enumerate()
        .filter_map(|(i, line)| label_of(line).map(|l| (i, normalize(l))))
        .collect();
    if let Some((i, _)) = lines.iter().find(|(_, l)| *l == wanted) {
        return Ok(*i);
    }
    let close: Vec<usize> = lines
        .iter()
        .filter(|(_, l)| l.contains(&wanted) || (l.len() >= 3 && wanted.contains(l.as_str())))
        .map(|(i, _)| *i)
        .collect();
    match close[..] {
        [i] => Ok(i),
        _ => {
            let all = labels(body);
            Err(if all.is_empty() {
                "the note has no fields".to_string()
            } else {
                format!("its fields are: {}", all.join(", "))
            })
        }
    }
}

/// The first value span of a line: (start, end) byte offsets of its content, ticks excluded.
fn first_span(line: &str) -> Option<(usize, usize)> {
    let start = line.find('`')? + 1;
    let len = line[start..].find('`')?;
    (len > 0).then_some((start, start + len))
}

/// Whether the field's current value is a secret (`!…!`).
pub fn is_secret(line: &str) -> bool {
    first_span(line).is_some_and(|(start, end)| {
        let content = &line[start..end];
        content.len() >= 2 && content.starts_with('!') && content.ends_with('!')
    })
}

/// The field's current value: its first span's content, without secret marks.
pub fn value_of(line: &str) -> Option<&str> {
    let (start, end) = first_span(line)?;
    let content = &line[start..end];
    Some(
        content
            .strip_prefix('!')
            .and_then(|c| c.strip_suffix('!'))
            .unwrap_or(content),
    )
}

/// Replaces line `index` with `new`, or removes it when `new` is `None`.
fn set_line(body: &str, index: usize, new: Option<&str>) -> String {
    let mut lines: Vec<&str> = body.split('\n').collect();
    match new {
        Some(text) => lines[index] = text,
        None => {
            lines.remove(index);
        }
    }
    lines.join("\n")
}

/// Gives field `label` a new value. `secret` `None` keeps whether the old one was secret.
pub fn update(
    body: &str,
    label: &str,
    value: &str,
    secret: Option<bool>,
) -> Result<String, String> {
    let index = find(body, label)?;
    let old = body.split('\n').nth(index).unwrap_or_default();
    let secret = secret.unwrap_or_else(|| is_secret(old));
    let new = match first_span(old) {
        Some((start, end)) => format!(
            "{}{}{}",
            &old[..start - 1],
            span(value, secret),
            &old[end + 1..]
        ),
        None => {
            let name = label_of(old).unwrap_or(label);
            let indent = &old[..old.len() - old.trim_start().len()];
            format!("{indent}{}", line(name, value, secret))
        }
    };
    Ok(set_line(body, index, Some(&new)))
}

pub fn remove(body: &str, label: &str) -> Result<String, String> {
    let index = find(body, label)?;
    Ok(set_line(body, index, None))
}

/// Appends a field line.
pub fn add(body: &str, label: &str, value: &str, secret: bool) -> String {
    let body = body.trim_end_matches('\n');
    let new = line(label, value, secret);
    if body.trim().is_empty() {
        new
    } else {
        format!("{body}\n{new}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str =
        "Panel for the shop.\n- **URL:** `https://panel.example`\nUser: `apo`\nPassword: `!0!`\n";

    #[test]
    fn reads_labels_of_value_lines_only() {
        assert_eq!(labels(BODY), ["URL", "User", "Password"]);
        assert_eq!(label_of("AWS_KEY=`x`"), Some("AWS_KEY="));
        assert_eq!(label_of("Note: call Ali"), Some("Note"));
        assert_eq!(label_of("Just text"), None);
        assert_eq!(label_of("Title:"), None);
    }

    #[test]
    fn finds_fields_loosely_but_never_guesses() {
        assert_eq!(find(BODY, "password"), Ok(3));
        assert_eq!(find(BODY, "url"), Ok(1));
        assert_eq!(find(BODY, "Panel URL"), Ok(1));
        assert_eq!(
            find(BODY, "token"),
            Err("its fields are: URL, User, Password".to_string())
        );
    }

    #[test]
    fn updates_keep_the_line_and_its_secrecy() {
        let body = update(BODY, "password", "s3cret", None).unwrap();
        assert!(body.contains("Password: `!s3cret!`"));
        let body = update(&body, "User", "root", None).unwrap();
        assert!(body.contains("User: `root`\n"));
        assert!(body.contains("- **URL:** `https://panel.example`"));
        let plain = update("Region: eu-west-1", "region", "eu-central-1", Some(false)).unwrap();
        assert_eq!(plain, "Region: `eu-central-1`");
    }

    #[test]
    fn adds_and_removes_lines() {
        let body = add(BODY, "2FA codes", "aaaa-bbbb", true);
        assert!(body.ends_with("Password: `!0!`\n2FA codes: `!aaaa-bbbb!`"));
        assert_eq!(add("", "Key", "k", false), "Key: `k`");
        let body = remove(&body, "user").unwrap();
        assert!(!body.contains("User"));
        assert_eq!(labels(&body), ["URL", "Password", "2FA codes"]);
        assert_eq!(value_of("Password: `!0!`"), Some("0"));
        assert!(is_secret("Password: `!0!`") && !is_secret("User: `apo`"));
    }
}
