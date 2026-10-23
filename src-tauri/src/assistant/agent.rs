//! The tool loop: ask the model, run the tools it calls, feed the results back, until it
//! answers in text or the step limit is reached.

use serde_json::Value;

use super::{Reply, Request, ToolCall, Turn, model_turns};
use crate::error::AppResult;

/// Model calls per user message. The last one is offered no tools, so it has to answer.
pub const MAX_STEPS: usize = 8;

pub trait Model {
    /// Reports the reply's visible text so far through `on_text` as it grows (it can also
    /// shrink, when reasoning turns out to precede it) and returns the whole reply.
    fn chat(&self, request: &Request, on_text: &mut dyn FnMut(&str)) -> AppResult<Reply>;
}

/// What a tool returned: the text the model reads, and the text stored in history. They differ
/// only when the model reads decrypted values, which history keeps as handles.
pub struct ToolResult {
    pub model: String,
    pub stored: String,
}

impl From<String> for ToolResult {
    fn from(text: String) -> Self {
        Self {
            model: text.clone(),
            stored: text,
        }
    }
}

pub struct Hooks<'a> {
    /// The visible text of the reply being written, so far.
    pub on_text: &'a mut dyn FnMut(&str),
    /// Stores a finished turn; the frontend learns about it from here.
    pub save: &'a mut dyn FnMut(&Turn) -> AppResult<()>,
    pub run_tool: &'a mut dyn FnMut(&ToolCall) -> AppResult<ToolResult>,
    pub cancelled: &'a dyn Fn() -> bool,
}

/// Answers the last user turn of `history`, appending (and saving) every turn on the way.
pub fn respond(
    model: &dyn Model,
    system: &str,
    tools: &Value,
    history: &mut Vec<Turn>,
    hooks: Hooks,
) -> AppResult<()> {
    // Cancelled between steps: an empty stopped turn marks where the reply ended.
    let stop = |history: &mut Vec<Turn>, save: &mut dyn FnMut(&Turn) -> AppResult<()>| {
        let turn = Turn::Assistant {
            text: String::new(),
            tool_calls: Vec::new(),
            stopped: true,
            thinking: String::new(),
        };
        save(&turn)?;
        history.push(turn);
        Ok(())
    };
    for step in 0..MAX_STEPS {
        if step > 0 && (hooks.cancelled)() {
            return stop(history, hooks.save);
        }
        let last = step + 1 == MAX_STEPS;
        let turns = model_turns(history);
        let request = Request {
            system,
            turns: &turns,
            tools: (!last).then_some(tools),
        };
        let reply = model.chat(&request, hooks.on_text)?;
        let stopped = reply.stopped || (hooks.cancelled)();
        let mut calls = if stopped || last {
            Vec::new()
        } else {
            reply.tool_calls
        };
        for (i, call) in calls.iter_mut().enumerate() {
            if call.id.is_empty() {
                call.id = format!("call_{}_{i}", history.len());
            }
        }
        let text = reply.text.trim().to_string();
        let thinking = reply.thinking.trim().to_string();
        if text.is_empty() && thinking.is_empty() && calls.is_empty() && !stopped {
            return Ok(());
        }
        let turn = Turn::Assistant {
            text,
            tool_calls: calls.clone(),
            stopped,
            thinking,
        };
        (hooks.save)(&turn)?;
        history.push(turn);
        if stopped || calls.is_empty() {
            return Ok(());
        }
        for call in calls {
            if (hooks.cancelled)() {
                return stop(history, hooks.save);
            }
            let result = (hooks.run_tool)(&call)?;
            (hooks.save)(&Turn::Tool {
                call_id: call.id.clone(),
                name: call.name.clone(),
                result: result.stored,
            })?;
            history.push(Turn::Tool {
                call_id: call.id,
                name: call.name,
                result: result.model,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use serde_json::json;

    use super::*;

    /// Plays back scripted replies and records whether tools were offered.
    struct Script {
        replies: RefCell<Vec<Reply>>,
        offered: RefCell<Vec<bool>>,
        seen: RefCell<Vec<usize>>,
    }

    impl Script {
        fn new(replies: Vec<Reply>) -> Self {
            Self {
                replies: RefCell::new(replies),
                offered: RefCell::new(Vec::new()),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Model for Script {
        fn chat(&self, request: &Request, on_text: &mut dyn FnMut(&str)) -> AppResult<Reply> {
            self.offered.borrow_mut().push(request.tools.is_some());
            self.seen.borrow_mut().push(request.turns.len());
            let reply = self.replies.borrow_mut().remove(0);
            on_text(&reply.text);
            Ok(reply)
        }
    }

    fn calls(name: &str) -> Reply {
        Reply {
            text: String::new(),
            tool_calls: vec![ToolCall {
                id: String::new(),
                name: name.into(),
                arguments: json!({ "query": "aws" }),
            }],
            stopped: false,
            thinking: String::new(),
        }
    }

    fn says(text: &str) -> Reply {
        Reply {
            text: text.into(),
            ..Reply::default()
        }
    }

    fn run(model: &Script, cancel_after_tools: Option<usize>) -> (Vec<Turn>, Vec<Turn>, String) {
        let mut history = vec![Turn::user("where is aws?")];
        let mut saved = Vec::new();
        let mut streamed = String::new();
        let tools_run = Cell::new(0);
        let cancelled = || cancel_after_tools.is_some_and(|n| tools_run.get() >= n);
        respond(
            model,
            "system",
            &json!([]),
            &mut history,
            Hooks {
                on_text: &mut |t| streamed.push_str(t),
                save: &mut |turn| {
                    saved.push(turn.clone());
                    Ok(())
                },
                run_tool: &mut |call| {
                    tools_run.set(tools_run.get() + 1);
                    Ok(format!("{} found note 12", call.name).into())
                },
                cancelled: &cancelled,
            },
        )
        .unwrap();
        (history, saved, streamed)
    }

    #[test]
    fn runs_tools_and_feeds_results_back() {
        let model = Script::new(vec![calls("search"), says(" Note 12 has it. ")]);
        let (history, saved, streamed) = run(&model, None);
        assert_eq!(history.len(), 4);
        assert_eq!(saved, history[1..]);
        let Turn::Tool {
            call_id, result, ..
        } = &history[2]
        else {
            panic!("expected a tool turn");
        };
        assert_eq!(result, "search found note 12");
        let Turn::Assistant { tool_calls, .. } = &history[1] else {
            panic!("expected the call");
        };
        assert_eq!(&tool_calls[0].id, call_id);
        assert_eq!(
            history[3],
            Turn::Assistant {
                text: "Note 12 has it.".into(),
                tool_calls: Vec::new(),
                stopped: false,
                thinking: String::new(),
            }
        );
        assert_eq!(streamed, " Note 12 has it. ");
        // The second request carried the user turn, the call and its result.
        assert_eq!(*model.seen.borrow(), [1, 3]);
    }

    #[test]
    fn last_step_offers_no_tools_and_ignores_calls() {
        let model = Script::new((0..MAX_STEPS).map(|_| calls("search")).collect());
        let (history, _, _) = run(&model, None);
        let offered = model.offered.borrow();
        assert_eq!(offered.len(), MAX_STEPS);
        assert!(offered[..MAX_STEPS - 1].iter().all(|&o| o) && !offered[MAX_STEPS - 1]);
        // user + (call + result) per step but the last, whose calls are dropped: nothing to say.
        assert_eq!(history.len(), 1 + 2 * (MAX_STEPS - 1));
    }

    #[test]
    fn cancel_stops_before_the_next_step() {
        let model = Script::new(vec![calls("search"), says("never asked")]);
        let (history, saved, _) = run(&model, Some(1));
        assert_eq!(model.offered.borrow().len(), 1);
        assert_eq!(saved.len(), 3);
        assert_eq!(
            history.last(),
            Some(&Turn::Assistant {
                text: String::new(),
                tool_calls: Vec::new(),
                stopped: true,
                thinking: String::new(),
            })
        );
    }

    #[test]
    fn a_stopped_reply_keeps_its_text_and_drops_calls() {
        let mut reply = calls("search");
        reply.text = "Looking".into();
        reply.stopped = true;
        let model = Script::new(vec![reply]);
        let (history, saved, _) = run(&model, None);
        assert_eq!(saved.len(), 1);
        assert_eq!(
            history[1],
            Turn::Assistant {
                text: "Looking".into(),
                tool_calls: Vec::new(),
                stopped: true,
                thinking: String::new(),
            }
        );
    }
}
