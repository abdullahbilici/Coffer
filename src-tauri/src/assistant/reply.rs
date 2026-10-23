//! One reply to a conversation's last message, the same for the app and the evaluation: user
//! text reaches the model with its values as handles, tools read the vault or add to a draft,
//! and a draft that isn't empty is stored as the reply's last turn. While the vault is unlocked
//! the model reads secrets next to their handles; history keeps the handles only, and every
//! word of the reply passes `redact` before it is shown or stored.
//!
//! A message with pasted text is an import: the paste goes to the model in chunks of whole
//! blocks with numbered lines, so no single answer gets huge, and every chunk adds to the same
//! draft.

use std::{ops::Range, sync::MutexGuard};

use rusqlite::Connection;
use serde_json::Value;

use super::{
    DraftStatus, Turn,
    agent::{self, Hooks, Model, ToolResult},
    draft::{self, Ctx, Draft},
    inputs::Inputs,
    plain::Plain,
    redact::{self, Known},
    tools,
};
use crate::{
    crypto::VaultKey,
    db::{items, todos},
    error::AppResult,
};

/// About this much numbered text goes to the model per import chunk.
const CHUNK_CHARS: usize = 1_800;

pub struct Io<'a> {
    pub db: &'a dyn Fn() -> MutexGuard<'a, Connection>,
    /// Lets the model read secrets and a draft skip the ones a note holds already; `None`
    /// keeps every secret a handle.
    pub key: Option<VaultKey>,
    /// Whether the unlock window is still open; once it closes, tools show handles only.
    pub unlocked: &'a dyn Fn() -> bool,
    /// `YYYY-MM-DD`.
    pub today: &'a str,
    /// The visible text of the reply being written, so far.
    pub on_text: &'a mut dyn FnMut(&str),
    /// Stores a finished turn.
    pub save: &'a mut dyn FnMut(&Turn) -> AppResult<()>,
    /// An import about to read its next part, with the draft so far.
    pub on_progress: &'a mut dyn FnMut(Progress),
    pub cancelled: &'a dyn Fn() -> bool,
    /// Runs before every tool, e.g. to check the app is still entered.
    pub check: &'a dyn Fn() -> AppResult<()>,
}

/// Where an import is: `part` of `parts` starts, after the parts before it drafted `groups`.
pub struct Progress {
    pub part: usize,
    pub parts: usize,
    pub groups: Vec<draft::Group>,
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
        unlocked,
        today,
        on_text,
        save,
        on_progress,
        cancelled,
        check,
    } = io;
    let (inputs, mut turns) = take_inputs(history);
    let (stored, plain, known) = {
        let conn = db();
        let plain = key.as_ref().map(|k| Plain::open(&conn, k)).transpose()?;
        let known = match &plain {
            Some(plain) => known_values(&conn, plain)?,
            None => Known::default(),
        };
        (draft::stored_values(&conn, key.as_ref())?, plain, known)
    };
    drop(key);
    let on_text = &mut |text: &str| on_text(&redact::redact_partial(text, &known));
    let save = &mut |turn: &Turn| match turn {
        Turn::Assistant {
            text,
            tool_calls,
            stopped,
            thinking,
        } => save(&Turn::Assistant {
            text: redact::redact(text, &known),
            tool_calls: tool_calls.clone(),
            stopped: *stopped,
            thinking: redact::redact(thinking, &known),
        }),
        _ => save(turn),
    };
    let mut draft = Draft::default();

    let mut run_tool = |draft: &mut Draft, name: &str, args: &Value| -> AppResult<ToolResult> {
        check()?;
        let conn = db();
        let ctx = Ctx {
            conn: &conn,
            today,
            inputs: &inputs,
            stored: &stored,
        };
        if let Some(text) = draft::run(&ctx, draft, name, args) {
            return Ok(text.into());
        }
        let masked = tools::run(&conn, today, name, args);
        Ok(match plain.as_ref().filter(|_| unlocked()) {
            Some(plain) => ToolResult {
                model: tools::run_with(&conn, Some(plain), today, name, args),
                stored: masked,
            },
            None => masked.into(),
        })
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
            &mut |part, parts, ops: &[draft::Op]| {
                let conn = db();
                let ctx = Ctx {
                    conn: &conn,
                    today,
                    inputs: &inputs,
                    stored: &stored,
                };
                let groups = draft::view(&ctx, ops)?;
                drop(conn);
                on_progress(Progress {
                    part,
                    parts,
                    groups,
                });
                Ok(())
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

/// What the reply may show of the vault: its secrets with their handles, and its copyable values.
fn known_values(conn: &Connection, plain: &Plain) -> AppResult<Known> {
    let mut copyable: Vec<String> = Vec::new();
    let templates = items::templates(conn)?.into_iter().map(|(_, t)| t);
    let descriptions = todos::list(conn)?.into_iter().map(|t| t.description);
    for text in templates.chain(descriptions) {
        copyable.extend(redact::copyable_spans(&text).map(str::to_string));
    }
    Ok(Known::new(plain.secrets(), copyable))
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

type RunTool<'a> = dyn FnMut(&mut Draft, &str, &Value) -> AppResult<ToolResult> + 'a;
/// Told `part` of `parts` starts, with the draft's ops so far.
type OnPart<'a> = dyn FnMut(usize, usize, &[draft::Op]) -> AppResult<()> + 'a;

#[allow(clippy::too_many_arguments)]
fn import(
    model: &dyn Model,
    system: &str,
    instruction: &str,
    paste: &str,
    save: &mut dyn FnMut(&Turn) -> AppResult<()>,
    outline: &mut dyn FnMut(&str, &[draft::Op]) -> AppResult<String>,
    progress: &mut OnPart,
    draft: &mut Draft,
    run_tool: &mut RunTool,
    cancelled: &dyn Fn() -> bool,
) -> AppResult<()> {
    let lines: Vec<&str> = paste.lines().collect();
    let specs = draft::import_specs();
    let instruction = instruction.trim();
    let mut any = false;
    let chunks = chunks(&lines);
    let parts = chunks.len();
    for (n, range) in chunks.into_iter().enumerate() {
        if cancelled() {
            return save(&Turn::Assistant {
                text: String::new(),
                tool_calls: Vec::new(),
                stopped: true,
                thinking: String::new(),
            });
        }
        progress(n + 1, parts, &draft.ops)?;
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
                    thinking: String::new(),
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
            thinking: String::new(),
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
        commands::sealing::seal_inline_with,
        db::{
            items::{Body, ItemRecord},
            open_in_memory, projects,
        },
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
            if let Some(Turn::User { text, .. } | Turn::Tool { result: text, .. }) =
                request.turns.last()
            {
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
    fn the_model_reads_values_but_the_user_sees_the_vault() {
        let key = Zeroizing::new(vec![1; 32]);
        let conn = open_in_memory();
        let acme = projects::insert(&conn, "Acme Shop", "", 0).unwrap();
        let token = "ghp_8fK2mQ9xL4vR7tY1wZ3pN6sB0cD5eH2jA9kU";
        let (template, secrets) =
            seal_inline_with(&key, &format!("User: `apo`\nToken: `!{token}!`"))
                .unwrap()
                .unwrap();
        items::insert(
            &conn,
            &ItemRecord {
                project_id: acme,
                folder_id: None,
                title: "GitHub",
                body: &Body::Inline { template, secrets },
                tags: &[],
            },
            0,
        )
        .unwrap();
        let db = Mutex::new(conn);
        // The model copies the token with one character dropped.
        let miscopied = format!("{}{}", &token[..10], &token[11..]);
        let model = Script {
            replies: RefCell::new(vec![
                calls("get_note", json!({ "id": 1 })),
                Reply {
                    text: format!("The token is `{miscopied}` (GitHub)."),
                    thinking: format!("The note says {token}."),
                    ..Reply::default()
                },
            ]),
            asked: RefCell::new(Vec::new()),
        };
        let saved = RefCell::new(Vec::new());
        let streamed = RefCell::new(String::new());
        let lock = || db.lock().unwrap();
        reply(
            &model,
            &Prompts {
                chat: "chat",
                import: "import",
            },
            &[Turn::user("github token?")],
            Io {
                db: &lock,
                key: Some(key),
                unlocked: &|| true,
                today: "2026-10-23",
                on_text: &mut |t| *streamed.borrow_mut() = t.to_string(),
                save: &mut |turn| {
                    saved.borrow_mut().push(turn.clone());
                    Ok(())
                },
                on_progress: &mut |_| {},
                cancelled: &|| false,
                check: &|| Ok(()),
            },
        )
        .unwrap();
        assert!(model.asked.borrow()[1].contains(&format!("Token: [n1.0: {token}]")));
        let saved = saved.borrow();
        let Turn::Tool { result, .. } = &saved[1] else {
            panic!("{saved:?}");
        };
        assert!(
            result.contains("Token: [n1.0]") && !result.contains(token),
            "{result}"
        );
        let Turn::Assistant { text, thinking, .. } = &saved[2] else {
            panic!("{saved:?}");
        };
        assert_eq!(text, "The token is [n1.0] (GitHub).");
        assert_eq!(thinking, "The note says [n1.0].");
        assert!(!streamed.borrow().contains("ghp_"), "{}", streamed.borrow());
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
                unlocked: &|| true,
                today: "2026-10-23",
                on_text: &mut |_| {},
                save: &mut |turn| {
                    saved.borrow_mut().push(turn.clone());
                    Ok(())
                },
                on_progress: &mut |_| {},
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
        let progress = RefCell::new(Vec::new());
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
                unlocked: &|| false,
                today: "2026-10-23",
                on_text: &mut |_| {},
                save: &mut |turn| {
                    saved.borrow_mut().push(turn.clone());
                    Ok(())
                },
                on_progress: &mut |p| {
                    progress
                        .borrow_mut()
                        .push((p.part, p.parts, p.groups.len()))
                },
                cancelled: &|| false,
                check: &|| Ok(()),
            },
        )
        .unwrap();
        let chunks = n.load(Ordering::Relaxed);
        assert!(chunks > 1);
        // Every part is announced with the draft so far: nothing before the first.
        let progress = progress.borrow();
        let parts: Vec<usize> = progress.iter().map(|p| p.0).collect();
        assert_eq!(parts, (1..=chunks).collect::<Vec<_>>());
        assert!(progress.iter().all(|p| p.1 == chunks));
        assert_eq!(progress[0].2, 0);
        assert_eq!(progress[chunks - 1].2, 1);
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
