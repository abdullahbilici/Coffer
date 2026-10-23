//! One reply to a conversation's last message, the same for the app and the evaluation: user
//! text reaches the model with its values as handles, tools read the vault or add to a draft,
//! and a draft that isn't empty is stored as the reply's last turn.
//!
//! A message with pasted text is an import: the paste goes to the model in chunks of whole
//! blocks with numbered lines, so no single answer gets huge, and every chunk adds to the same
//! draft.

use std::{ops::Range, sync::MutexGuard};

use rusqlite::Connection;
use serde_json::Value;

use super::{
    DraftStatus, Turn,
    agent::{self, Hooks, Model},
    draft::{self, Ctx, Draft},
    inputs::Inputs,
    tools,
};
use crate::{crypto::VaultKey, error::AppResult};

/// About this much numbered text goes to the model per import chunk.
const CHUNK_CHARS: usize = 1_800;

pub struct Io<'a> {
    pub db: &'a dyn Fn() -> MutexGuard<'a, Connection>,
    /// Lets a draft skip secrets a note holds already; `None` checks readable values only.
    pub key: Option<VaultKey>,
    /// `YYYY-MM-DD`.
    pub today: &'a str,
    /// The visible text of the reply being written, so far.
    pub on_text: &'a mut dyn FnMut(&str),
    /// Stores a finished turn.
    pub save: &'a mut dyn FnMut(&Turn) -> AppResult<()>,
    pub cancelled: &'a dyn Fn() -> bool,
    /// Runs before every tool, e.g. to check the app is still entered.
    pub check: &'a dyn Fn() -> AppResult<()>,
}

pub struct Prompts<'a> {
    pub chat: &'a str,
    pub import: &'a str,
}

/// The values of every user message, numbered in order, and the history as the model reads it.
pub fn take_inputs(history: &[Turn]) -> (Inputs, Vec<Turn>) {
    let mut inputs = Inputs::default();
    let turns = history
        .iter()
        .map(|turn| match turn {
            Turn::User { text, paste } => {
                let text = inputs.take(text);
                let paste = paste.as_ref().map(|p| inputs.take(p));
                Turn::User { text, paste }
            }
            other => other.clone(),
        })
        .collect();
    (inputs, turns)
}

/// Answers the last turn of `history`, saving each new turn through `io.save`.
pub fn reply(model: &dyn Model, prompts: &Prompts, history: &[Turn], io: Io) -> AppResult<()> {
    let Io {
        db,
        key,
        today,
        on_text,
        save,
        cancelled,
        check,
    } = io;
    let (inputs, mut turns) = take_inputs(history);
    let stored = draft::stored_values(&db(), key.as_ref())?;
    drop(key);
    let mut draft = Draft::default();

    let mut run_tool = |draft: &mut Draft, name: &str, args: &Value| -> AppResult<String> {
        check()?;
        let conn = db();
        let ctx = Ctx {
            conn: &conn,
            today,
            inputs: &inputs,
            stored: &stored,
        };
        Ok(draft::run(&ctx, draft, name, args)
            .unwrap_or_else(|| tools::run(&conn, today, name, args)))
    };

    let paste = match turns.last() {
        Some(Turn::User {
            text,
            paste: Some(paste),
        }) => Some((text.clone(), paste.clone())),
        _ => None,
    };
    let result = match paste {
        None => {
            let mut specs = tools::specs();
            if let Value::Array(list) = &mut specs {
                list.extend(draft::specs());
            }
            let mut respond = |turns: &mut Vec<Turn>, draft: &mut Draft| {
                agent::respond(
                    model,
                    prompts.chat,
                    &specs,
                    turns,
                    Hooks {
                        on_text: &mut *on_text,
                        save: &mut *save,
                        run_tool: &mut |call| run_tool(draft, &call.name, &call.arguments),
                        cancelled,
                    },
                )
            };
            respond(&mut turns, &mut draft).and_then(|()| {
                // Small models sometimes say they drafted a change without calling a tool:
                // tell it once, as the model's own context only.
                if draft.ops.is_empty() && !cancelled() && claims_a_draft(&turns) {
                    turns.push(Turn::user(NOTHING_DRAFTED));
                    respond(&mut turns, &mut draft)
                } else {
                    Ok(())
                }
            })
        }
        Some((instruction, paste)) => import(
            model,
            prompts.import,
            &instruction,
            &paste,
            &mut *save,
            &mut |chunk, ops: &[draft::Op]| {
                let conn = db();
                let ctx = Ctx {
                    conn: &conn,
                    today,
                    inputs: &inputs,
                    stored: &stored,
                };
                draft::outline(&ctx, ops, chunk)
            },
            &mut draft,
            &mut run_tool,
            cancelled,
        ),
    };

    // A draft is kept even when the reply failed or stopped part way.
    if !draft.ops.is_empty() {
        let groups = {
            let conn = db();
            let ctx = Ctx {
                conn: &conn,
                today,
                inputs: &inputs,
                stored: &stored,
            };
            draft::view(&ctx, &draft.ops)?
        };
        save(&Turn::Draft {
            ops: draft.ops,
            groups,
            status: DraftStatus::Pending,
        })?;
    }
    result
}

const NOTHING_DRAFTED: &str = "Nothing was drafted: no write tool was called. Call the write \
tool now if a change is needed, without asking.";

const NO_CALL: &str = "Call add_notes now with every note from these lines. Do not ask or \
explain.";

/// Whether the reply's last words speak of a draft.
fn claims_a_draft(turns: &[Turn]) -> bool {
    let Some(Turn::Assistant { text, .. }) = turns.last() else {
        return false;
    };
    let text = text.to_lowercase();
    ["draft", "taslak"].iter().any(|w| text.contains(w))
}

type RunTool<'a> = dyn FnMut(&mut Draft, &str, &Value) -> AppResult<String> + 'a;

#[allow(clippy::too_many_arguments)]
fn import(
    model: &dyn Model,
    system: &str,
    instruction: &str,
    paste: &str,
    save: &mut dyn FnMut(&Turn) -> AppResult<()>,
    outline: &mut dyn FnMut(&str, &[draft::Op]) -> AppResult<String>,
    draft: &mut Draft,
    run_tool: &mut RunTool,
    cancelled: &dyn Fn() -> bool,
) -> AppResult<()> {
    let lines: Vec<&str> = paste.lines().collect();
    let specs = draft::import_specs();
    let instruction = instruction.trim();
    let mut any = false;
    for (n, range) in chunks(&lines).into_iter().enumerate() {
        if cancelled() {
            return save(&Turn::Assistant {
                text: String::new(),
                tool_calls: Vec::new(),
                stopped: true,
            });
        }
        let numbered = blocks(&lines, range.clone());
        let mut prompt = String::new();
        if !instruction.is_empty() {
            prompt.push_str(&format!("The user says: {instruction}\n\n"));
        }
        prompt.push_str(&format!(
            "Vault now:\n{}\n\nLines {}-{} of the pasted text:\n{numbered}",
            outline(&numbered, &draft.ops)?,
            range.start + 1,
            range.end,
        ));
        let mut local = vec![Turn::user(prompt)];
        // Only the tool traffic of a chunk is kept: its prompt and closing words aren't the
        // conversation. Call ids get the chunk's number so they stay unique.
        let prefix = format!("import{n}_");
        let mut keep = |turn: &Turn| match turn {
            Turn::Assistant {
                tool_calls,
                stopped,
                ..
            } if !tool_calls.is_empty() || *stopped => {
                let mut calls = tool_calls.clone();
                for call in &mut calls {
                    call.id = format!("{prefix}{}", call.id);
                }
                save(&Turn::Assistant {
                    text: String::new(),
                    tool_calls: calls,
                    stopped: *stopped,
                })
            }
            Turn::Tool {
                call_id,
                name,
                result,
            } => save(&Turn::Tool {
                call_id: format!("{prefix}{call_id}"),
                name: name.clone(),
                result: result.clone(),
            }),
            _ => Ok(()),
        };
        // A chunk whose answer calls no tool (it asked or explained instead) gets one reminder.
        for attempt in 0..2 {
            if attempt == 1 {
                let called = local.iter().any(|t| matches!(t, Turn::Tool { .. }));
                if called || cancelled() {
                    break;
                }
                local.push(Turn::user(NO_CALL));
            }
            agent::respond(
                model,
                system,
                &specs,
                &mut local,
                Hooks {
                    on_text: &mut |_| {},
                    save: &mut keep,
                    run_tool: &mut |call| run_tool(draft, &call.name, &call.arguments),
                    cancelled,
                },
            )?;
        }
        any = true;
    }
    if any && draft.ops.is_empty() {
        save(&Turn::Assistant {
            text: "Nothing new to import.".to_string(),
            tool_calls: Vec::new(),
            stopped: false,
        })?;
    }
    Ok(())
}

/// Numbered lines grouped into the blocks the text has (runs between blank lines), so a
/// heading visibly belongs to the lines under it.
fn blocks(lines: &[&str], range: Range<usize>) -> String {
    let mut out = String::new();
    let mut open = false;
    let mut n = 0;
    for i in range {
        if lines[i].trim().is_empty() {
            open = false;
            continue;
        }
        if !open {
            n += 1;
            out.push_str(&format!("{}Block {n}:\n", if n > 1 { "\n" } else { "" }));
            open = true;
        }
        out.push_str(&format!("{}: {}\n", i + 1, lines[i]));
    }
    out.trim_end().to_string()
}

/// Line ranges of about `CHUNK_CHARS` each, cut at blank lines so a block (a project heading
/// and what follows it) stays whole; a block without blank lines is cut when twice as long.
fn chunks(lines: &[&str]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut size = 0;
    for (i, line) in lines.iter().enumerate() {
        size += line.len() + 8;
        let blank = line.trim().is_empty();
        if (blank && size >= CHUNK_CHARS) || size >= 2 * CHUNK_CHARS {
            out.push(start..i + 1);
            start = i + 1;
            size = 0;
        }
    }
    if start < lines.len() {
        out.push(start..lines.len());
    }
    out.retain(|r| lines[r.clone()].iter().any(|l| !l.trim().is_empty()));
    out
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use serde_json::json;
    use zeroize::Zeroizing;

    use super::*;
    use crate::{
        assistant::{Reply, Request, ToolCall},
        db::{items, open_in_memory, projects},
    };

    #[test]
    fn numbers_lines_by_block() {
        let lines = ["## Acme", "key: [u1]", "", "", "pin 42", "other"];
        assert_eq!(
            blocks(&lines, 0..lines.len()),
            "Block 1:\n1: ## Acme\n2: key: [u1]\n\nBlock 2:\n5: pin 42\n6: other"
        );
    }

    #[test]
    fn chunks_keep_blocks_whole() {
        let block = ["Header", "a: 1", "b: 2", ""];
        let lines: Vec<&str> = block.iter().cycle().take(4 * 200).copied().collect();
        let ranges = chunks(&lines);
        assert!(ranges.len() > 1);
        for r in &ranges {
            assert_eq!(lines[r.start], "Header", "{r:?}");
        }
        assert_eq!(ranges.last().unwrap().end, lines.len());
        let flat: Vec<&str> = std::iter::repeat_n("x: 1", 2_000).collect();
        assert!(chunks(&flat).len() > 1);
        assert!(chunks(&["", "  "]).is_empty());
    }

    /// Answers every request with the next scripted reply and records what it was asked.
    struct Script {
        replies: RefCell<Vec<Reply>>,
        asked: RefCell<Vec<String>>,
    }

    impl Model for Script {
        fn chat(&self, request: &Request, _: &mut dyn FnMut(&str)) -> AppResult<Reply> {
            if let Some(Turn::User { text, .. }) = request.turns.last() {
                self.asked.borrow_mut().push(text.clone());
            }
            Ok(if self.replies.borrow().is_empty() {
                Reply::default()
            } else {
                self.replies.borrow_mut().remove(0)
            })
        }
    }

    fn calls(name: &str, arguments: Value) -> Reply {
        Reply {
            tool_calls: vec![ToolCall {
                id: String::new(),
                name: name.into(),
                arguments,
            }],
            ..Reply::default()
        }
    }

    #[test]
    fn values_reach_the_draft_but_never_the_model() {
        let conn = open_in_memory();
        projects::insert(&conn, "Acme Shop", "", 0).unwrap();
        let db = Mutex::new(conn);
        let key = "sk_test_51HxYz9KqLmN0pQrStUv";
        let history = [Turn::User {
            text: format!("add the stripe key {key} to acme"),
            paste: None,
        }];
        let model = Script {
            replies: RefCell::new(vec![
                calls(
                    "create_note",
                    json!({ "project": "acme", "title": "Stripe", "fields": [{ "label": "Secret key", "value": "[u1]", "secret": true }] }),
                ),
                Reply {
                    text: "Drafted.".into(),
                    ..Reply::default()
                },
            ]),
            asked: RefCell::new(Vec::new()),
        };
        let saved = RefCell::new(Vec::new());
        let lock = || db.lock().unwrap();
        reply(
            &model,
            &Prompts {
                chat: "chat",
                import: "import",
            },
            &history,
            Io {
                db: &lock,
                key: Some(Zeroizing::new(vec![1; 32])),
                today: "2026-10-23",
                on_text: &mut |_| {},
                save: &mut |turn| {
                    saved.borrow_mut().push(turn.clone());
                    Ok(())
                },
                cancelled: &|| false,
                check: &|| Ok(()),
            },
        )
        .unwrap();
        let asked = model.asked.borrow();
        assert_eq!(
            asked[0],
            "add the stripe key [u1: sk_test_…, 28 chars] to acme"
        );
        let saved = saved.borrow();
        let Some(Turn::Draft { groups, status, .. }) = saved.last() else {
            panic!("no draft: {saved:?}");
        };
        assert_eq!(*status, DraftStatus::Pending);
        assert_eq!(groups[0].project, "Acme Shop");
        for turn in saved.iter().filter(|t| !matches!(t, Turn::Draft { .. })) {
            assert!(!format!("{turn:?}").contains(key), "{turn:?}");
        }
    }

    #[test]
    fn imports_run_chunk_by_chunk_into_one_draft() {
        let db = Mutex::new(open_in_memory());
        let block = |i: usize| format!("Project{i}\nadmin pass Secret{i}x99!\n");
        let paste: String = (0..60).map(block).collect::<Vec<_>>().join("\n");
        let history = [Turn::User {
            text: String::new(),
            paste: Some(paste),
        }];
        let n = AtomicUsize::new(0);
        struct Each<'a>(&'a AtomicUsize);
        impl Model for Each<'_> {
            fn chat(&self, request: &Request, _: &mut dyn FnMut(&str)) -> AppResult<Reply> {
                // Answer a tool result with words; a chunk prompt with one note.
                if matches!(request.turns.last(), Some(Turn::Tool { .. })) {
                    return Ok(Reply::default());
                }
                let i = self.0.fetch_add(1, Ordering::Relaxed);
                Ok(calls(
                    "add_notes",
                    json!({ "notes": [{ "project": "Imported", "title": format!("Admin {i}"), "fields": [{ "label": "Password", "value": format!("[u{}]", i + 1), "secret": true }] }] }),
                ))
            }
        }
        let saved = RefCell::new(Vec::new());
        let lock = || db.lock().unwrap();
        reply(
            &Each(&n),
            &Prompts {
                chat: "chat",
                import: "import",
            },
            &history,
            Io {
                db: &lock,
                key: None,
                today: "2026-10-23",
                on_text: &mut |_| {},
                save: &mut |turn| {
                    saved.borrow_mut().push(turn.clone());
                    Ok(())
                },
                cancelled: &|| false,
                check: &|| Ok(()),
            },
        )
        .unwrap();
        let chunks = n.load(Ordering::Relaxed);
        assert!(chunks > 1);
        let saved = saved.borrow();
        let Some(Turn::Draft { ops, groups, .. }) = saved.last() else {
            panic!("no draft");
        };
        // One project, drafted once, then one note per chunk.
        assert_eq!(ops.len(), chunks + 1);
        assert_eq!(groups.len(), 1);
        let ids: Vec<&str> = saved
            .iter()
            .filter_map(|t| match t {
                Turn::Tool { call_id, .. } => Some(call_id.as_str()),
                _ => None,
            })
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(items::list(&db.lock().unwrap()).unwrap().is_empty());
    }
}
