//! The assistant: a local model (Ollama) that reads Coffer through tools. It never sees a secret
//! value: tools replace secrets with handles (`handles`), which only the user can reveal.

pub mod agent;
pub mod draft;
pub mod fields;
pub mod handles;
pub mod inputs;
pub mod ollama;
pub mod reply;
pub mod spend;
pub mod tools;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One stored message of a conversation, sealed as JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Turn {
    User {
        text: String,
        /// Text pasted for import (a dump of notes and credentials), sent with the message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        paste: Option<String>,
    },
    Assistant {
        text: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
        /// The user stopped the reply before it finished.
        #[serde(default)]
        stopped: bool,
    },
    Tool {
        call_id: String,
        name: String,
        result: String,
    },
    /// The changes a reply drafted, for the user to apply or discard.
    Draft {
        ops: Vec<draft::Op>,
        groups: Vec<draft::Group>,
        #[serde(default)]
        status: DraftStatus,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftStatus {
    #[default]
    Pending,
    Applied,
    Discarded,
}

impl Turn {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
            paste: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// What the model is asked to continue.
pub struct Request<'a> {
    pub system: &'a str,
    pub turns: &'a [Turn],
    /// Tool definitions to offer; the last step of a reply goes without, so it has to answer.
    pub tools: Option<&'a Value>,
}

/// One model response.
#[derive(Default)]
pub struct Reply {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Cancelled while streaming; `text` holds what arrived until then.
    pub stopped: bool,
}

/// Context the model gets with every request (Ollama truncates silently past it).
pub const NUM_CTX: u32 = 16_384;

/// Room for earlier turns, in characters (roughly a third of `NUM_CTX` tokens each way).
const HISTORY_BUDGET: usize = 36_000;

/// The system prompt: role, data model, how secrets and values look, today, and the user's
/// own notes.
pub fn system_prompt(today: &str, context: &str) -> String {
    let mut prompt = format!(
        "You are the assistant inside Coffer, a private vault on the user's Mac for project \
credentials, notes, todos and payments. You work only inside Coffer through your tools: you \
cannot browse the web or run commands.

Coffer's data:
- Projects hold nested folders and notes. Each project has a markdown overview.
- A note has a title, tags such as env/prod, and a markdown body. Its fields are lines like \
\"Password: value\".
- Todos belong to a project or are general, with an optional due date. Payments are monthly, \
yearly or once, in their own currency, with price history. Amounts in different currencies are \
never converted or added together.

Secrets never reach you. Tools show each one as a handle: [n12.0] is secret 0 of note 12, [p3.0] \
is in a project overview, [t4.0] in a todo, and [n12] is a note whose whole body is secret. The \
app shows a handle to the user as a hidden value they can reveal and copy. Put the exact handle \
where the value belongs, never guess a value, and never claim to know one.

Values the user types reach you as handles too: [u3: sk_live_…, 32 chars] is a value from their \
message; only its kind and length are shown.

Changing Coffer:
- Write tools (create_note, add_field, update_field, move_note, complete_todo, create_payment …) \
only draft changes. The user reviews the draft and applies or discards it, so say what you \
drafted, never that it is done.
- A field value is always a handle: [u3] for a value from the user's message, [n12.0] to copy a \
stored secret. Never type a value yourself; a value the user wrote plainly as a short word may be \
passed as written.
- Look up ids first (search, get_project, get_note, list_todos, list_payments) and change what \
exists: add a field to the right note rather than making a new one, and update a field that \
already has a value. Use the field's label as it is in the note.
- Mark a field secret for passwords, keys, tokens, PINs and recovery codes; usernames, emails, \
hosts, public URLs and ids are not secret.
- If a tool answers with an error, fix the call and try again.

How to answer:
- Look things up before answering. Use search to find notes by name, then get_note or \
get_project for details. Use project_spend for any cost question instead of adding amounts \
yourself.
- Answer briefly in the user's language, in markdown, without emoji. Name the note you used.
- If nothing in Coffer answers the question, say so.

Today is {today}."
    );
    let context = context.trim();
    if !context.is_empty() {
        prompt.push_str("\n\nWhat the user wants you to know:\n");
        prompt.push_str(context);
    }
    prompt
}

/// The system prompt for importing pasted text into a draft, one chunk of lines at a time.
pub fn import_prompt(context: &str) -> String {
    let mut prompt = "You file pasted text into Coffer, a private vault: projects hold \
folders and notes, and a note's fields are lines like \"Password: value\". The text is a messy \
dump of credentials and notes. Every value in it is shown as a handle such as \
[u3: sk_live_…, 32 chars]; you never see values, only their kind and length.

Call add_notes once with every note from the lines you are given, then stop. Rules:
- One note per account, service or environment (prod and staging are separate notes). Its \
fields are the values of that account, each with a short label (Password, API key, Host, User) \
and the handle as the value, e.g. \"[u3]\".
- Use the project the text puts a value under. The same project appears under different \
spellings and with suffixes (\"Name\", \"name projesi\", \"NAME işleri\"): use one clean \
name, and when the vault has the project already, its exact name. Accounts with no project \
around them that look private go to project \"Personal\".
- Folders only for projects with many notes (Infra, Payments, Email, Marketing, Mobile), or the \
folders a vault project already has; otherwise leave folder empty.
- Secret: passwords, API keys, tokens, secret keys, PINs, codes, webhook URLs with keys. Not \
secret: usernames, emails, URLs, hosts, IPs, regions, public ids.
- When a note the vault has already gets a new value, call add_field on it; when the text says \
a stored value changed, call update_field with the field's label as it is in the note. Values \
the vault has already are skipped for you.
- Lines without values (todos, comments) are not imported."
        .to_string();
    let context = context.trim();
    if !context.is_empty() {
        prompt.push_str("\n\nWhat the user wants you to know:\n");
        prompt.push_str(context);
    }
    prompt
}

/// The turns sent to the model. The current exchange (from the last user turn on) goes as is;
/// earlier ones keep only what was said, without tool traffic, and the oldest go first once
/// they no longer fit `HISTORY_BUDGET`.
pub fn model_turns(history: &[Turn]) -> Vec<Turn> {
    let current = history
        .iter()
        .rposition(|turn| matches!(turn, Turn::User { .. }))
        .unwrap_or(0);
    let mut earlier: Vec<Turn> = history[..current]
        .iter()
        .filter_map(|turn| match turn {
            Turn::User { text, paste } => Some(Turn::user(match paste {
                Some(paste) => format!(
                    "{text}\n(Pasted {} lines to import.)",
                    paste.lines().count()
                ),
                None => text.clone(),
            })),
            Turn::Assistant { text, .. } if !text.is_empty() => Some(said(text.clone())),
            Turn::Draft { .. } => Some(said(draft_note(turn))),
            _ => None,
        })
        .collect();
    let recent: Vec<Turn> = history[current..]
        .iter()
        .map(|turn| match turn {
            Turn::Draft { .. } => said(draft_note(turn)),
            _ => turn.clone(),
        })
        .collect();
    let recent = &recent[..];
    let size = |turns: &[Turn]| turns.iter().map(turn_len).sum::<usize>();
    let mut total = size(&earlier) + size(recent);
    while total > HISTORY_BUDGET && !earlier.is_empty() {
        // Drop the oldest exchange: its user turn and the replies up to the next one.
        let end = earlier[1..]
            .iter()
            .position(|turn| matches!(turn, Turn::User { .. }))
            .map_or(earlier.len(), |i| i + 1);
        total -= size(&earlier[..end]);
        earlier.drain(..end);
    }
    earlier.extend_from_slice(recent);
    earlier
}

fn said(text: String) -> Turn {
    Turn::Assistant {
        text,
        tool_calls: Vec::new(),
        stopped: false,
    }
}

/// What the model remembers of a draft: what it changed and what the user did with it.
fn draft_note(turn: &Turn) -> String {
    let Turn::Draft { groups, status, .. } = turn else {
        return String::new();
    };
    let outcome = match status {
        DraftStatus::Pending => "not applied yet",
        DraftStatus::Applied => "the user applied it",
        DraftStatus::Discarded => "the user discarded it",
    };
    format!("(Draft: {}. {outcome}.)", draft::summary(groups))
}

fn turn_len(turn: &Turn) -> usize {
    match turn {
        Turn::User { text, paste } => text.len() + paste.as_ref().map_or(0, String::len),
        Turn::Draft { .. } => draft_note(turn).len(),
        Turn::Assistant {
            text, tool_calls, ..
        } => {
            text.len()
                + tool_calls
                    .iter()
                    .map(|c| c.name.len() + c.arguments.to_string().len())
                    .sum::<usize>()
        }
        Turn::Tool { result, .. } => result.len(),
    }
}

/// What to show of a reply. Some models (qwen3 after tool calls) still reason in their content
/// with `think: false`, ending it with `</think>`: only the text after the last one is the
/// answer, and nothing shows while an opened `<think>` block runs.
pub fn visible_text(raw: &str) -> &str {
    if let Some(end) = raw.rfind("</think>") {
        return raw[end + "</think>".len()..].trim_start();
    }
    if raw.trim_start().starts_with("<think>") {
        return "";
    }
    raw
}

/// A short title for a new conversation: the first line of its first message.
pub fn title_from(text: &str) -> String {
    const MAX: usize = 60;
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> Turn {
        Turn::user(text)
    }

    fn assistant(text: &str, calls: usize) -> Turn {
        Turn::Assistant {
            text: text.into(),
            tool_calls: (0..calls)
                .map(|i| ToolCall {
                    id: format!("c{i}"),
                    name: "search".into(),
                    arguments: serde_json::json!({ "query": "x" }),
                })
                .collect(),
            stopped: false,
        }
    }

    fn tool(result: &str) -> Turn {
        Turn::Tool {
            call_id: "c0".into(),
            name: "search".into(),
            result: result.into(),
        }
    }

    #[test]
    fn turns_round_trip_as_tagged_json() {
        let turn = assistant("hi", 1);
        let json = serde_json::to_string(&turn).unwrap();
        assert!(json.contains(r#""role":"assistant""#) && json.contains(r#""toolCalls""#));
        assert_eq!(serde_json::from_str::<Turn>(&json).unwrap(), turn);
        let old: Turn = serde_json::from_str(r#"{"role":"assistant","text":"x"}"#).unwrap();
        assert_eq!(old, assistant("x", 0));
    }

    #[test]
    fn earlier_exchanges_drop_tool_traffic() {
        let history = [
            user("first"),
            assistant("", 1),
            tool("result one"),
            assistant("answer one", 0),
            user("second"),
            assistant("", 1),
            tool("result two"),
        ];
        assert_eq!(
            model_turns(&history),
            [
                user("first"),
                assistant("answer one", 0),
                user("second"),
                assistant("", 1),
                tool("result two"),
            ]
        );
    }

    #[test]
    fn oldest_exchanges_go_first_when_over_budget() {
        let long = "x".repeat(HISTORY_BUDGET / 2);
        let history = [
            user("old"),
            assistant(&long, 0),
            user("middle"),
            assistant(&long, 0),
            user("now"),
        ];
        assert_eq!(
            model_turns(&history),
            [user("middle"), assistant(&long, 0), user("now")]
        );
    }

    #[test]
    fn hides_reasoning_before_the_answer() {
        assert_eq!(
            visible_text("Draft answer.\n</think>\n\nAnswer."),
            "Answer."
        );
        assert_eq!(visible_text("<think>still going"), "");
        assert_eq!(visible_text("Plain answer"), "Plain answer");
    }

    #[test]
    fn titles_use_the_first_line() {
        assert_eq!(title_from("\n  How do I log in?\nmore"), "How do I log in?");
        let long = "word ".repeat(20);
        let title = title_from(&long);
        assert!(title.ends_with('…') && title.chars().count() <= 61);
    }

    #[test]
    fn prompt_includes_the_users_context() {
        let prompt = system_prompt("Friday, 23 October 2026", "  I use eu-central-1.  ");
        assert!(prompt.contains("Today is Friday, 23 October 2026."));
        assert!(prompt.ends_with("know:\nI use eu-central-1."));
        assert!(!system_prompt("today", " ").contains("wants you to know"));
    }
}
