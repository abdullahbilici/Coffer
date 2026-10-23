//! The assistant: a local model (Ollama) that reads Coffer through tools. It never sees a secret
//! value: tools replace secrets with handles (`handles`), which only the user can reveal.

pub mod agent;
pub mod handles;
pub mod ollama;
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

/// The system prompt: role, data model, how secrets look, today, and the user's own notes.
pub fn system_prompt(today: &str, context: &str) -> String {
    let mut prompt = format!(
        "You are the assistant inside Coffer, a private vault on the user's Mac for project \
credentials, notes, todos and payments. You can only read Coffer through your tools. You cannot \
change anything, browse the web or run commands.

Coffer's data:
- Projects hold nested folders and notes. Each project has a markdown overview.
- A note has a title, tags such as env/prod, and a markdown body. `value` in a body is a value the \
user can copy.
- Todos belong to a project or are general, with an optional due date. Payments are monthly, \
yearly or once, in their own currency, with price history. Amounts in different currencies are \
never converted or added together.

Secrets never reach you. Tools show each one as a handle: [n12.0] is secret 0 of note 12, [p3.0] \
is in a project overview, [t4.0] in a todo, and [n12] is a note whose whole body is secret. The \
app shows a handle to the user as a hidden value they can reveal and copy. Put the exact handle \
where the value belongs, never guess a value, and never claim to know one.

How to answer:
- Look things up before answering. Use search to find notes by name, then get_note or \
get_project for details. Use project_spend for any cost question instead of adding amounts \
yourself.
- Answer briefly in the user's language, in markdown. Name the note you used.
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
            Turn::User { .. } => Some(turn.clone()),
            Turn::Assistant { text, .. } if !text.is_empty() => Some(Turn::Assistant {
                text: text.clone(),
                tool_calls: Vec::new(),
                stopped: false,
            }),
            _ => None,
        })
        .collect();
    let recent = &history[current..];
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

fn turn_len(turn: &Turn) -> usize {
    match turn {
        Turn::User { text } => text.len(),
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
        Turn::User { text: text.into() }
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
