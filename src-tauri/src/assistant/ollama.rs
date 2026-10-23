//! Ollama's native API: `/api/chat` streamed line by line, and `/api/tags` for the model list.
//! Requests go to the configured URL only.

use std::{
    io::{BufRead, BufReader},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::Duration,
};

use serde_json::{Value, json};
use ureq::{
    Agent,
    tls::{RootCerts, TlsConfig, TlsProvider},
};
use zeroize::Zeroizing;

use super::{NUM_CTX, Reply, Request, ToolCall, Turn, agent::Model, visible_text};
use crate::error::{AppError, AppResult};

/// How often a waiting reply checks for cancel.
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// Trims the URL and checks it is plain http(s) with a host, without a trailing slash.
pub fn normalize_url(url: &str) -> AppResult<String> {
    let url = url.trim().trim_end_matches('/');
    let host = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    match host {
        Some(host) if !host.is_empty() && !host.contains(char::is_whitespace) => {
            Ok(url.to_string())
        }
        _ => Err(AppError::Invalid(
            "Enter a URL starting with http:// or https://".to_string(),
        )),
    }
}

fn agent() -> Agent {
    Agent::new_with_config(
        Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(5)))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build(),
    )
}

fn unreachable(url: &str, error: &ureq::Error) -> AppError {
    AppError::Network(format!("Can't reach {url}: {error}"))
}

/// Ollama reports failures as `{ "error": "…" }`.
fn server_error(status: u16, body: &str) -> AppError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("The model server answered {status}"));
    AppError::Network(message)
}

/// The models the server has, sorted by name.
pub fn list_models(url: &str, api_key: Option<&str>) -> AppResult<Vec<String>> {
    let url = normalize_url(url)?;
    let mut request = agent().get(format!("{url}/api/tags"));
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let mut response = request.call().map_err(|e| unreachable(&url, &e))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| unreachable(&url, &e))?;
    if status != 200 {
        return Err(server_error(status, &body));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|_| AppError::Network(format!("{url} is not an Ollama server")))?;
    let mut names: Vec<String> = value["models"]
        .as_array()
        .ok_or_else(|| AppError::Network(format!("{url} is not an Ollama server")))?
        .iter()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();
    names.sort();
    Ok(names)
}

pub struct Ollama {
    url: String,
    model: String,
    api_key: Option<Zeroizing<String>>,
    cancel: Arc<AtomicBool>,
}

impl Ollama {
    pub fn new(
        url: &str,
        model: &str,
        api_key: Option<Zeroizing<String>>,
        cancel: Arc<AtomicBool>,
    ) -> AppResult<Self> {
        Ok(Self {
            url: normalize_url(url)?,
            model: model.trim().to_string(),
            api_key,
            cancel,
        })
    }

    fn body(&self, request: &Request) -> Value {
        let mut messages = vec![json!({ "role": "system", "content": request.system })];
        messages.extend(request.turns.iter().map(message));
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "think": false,
            "options": { "num_ctx": NUM_CTX },
        });
        if let Some(tools) = request.tools {
            body["tools"] = tools.clone();
        }
        body
    }
}

fn message(turn: &Turn) -> Value {
    match turn {
        Turn::User { text, .. } => json!({ "role": "user", "content": text }),
        // `model_turns` turns drafts into text before a request is built.
        Turn::Draft { .. } => json!({ "role": "assistant", "content": "" }),
        Turn::Assistant {
            text, tool_calls, ..
        } => {
            let mut message = json!({ "role": "assistant", "content": text });
            if !tool_calls.is_empty() {
                message["tool_calls"] = tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "function": { "name": call.name, "arguments": call.arguments },
                        })
                    })
                    .collect();
            }
            message
        }
        Turn::Tool {
            call_id,
            name,
            result,
        } => json!({
            "role": "tool",
            "content": result,
            "tool_name": name,
            "tool_call_id": call_id,
        }),
    }
}

enum Chunk {
    Line(Value),
    End,
    Failed(AppError),
}

impl Model for Ollama {
    /// Streams on a reader thread, so a cancel returns at once even while the server is still
    /// loading the model or reading the prompt. Dropping the channel ends the reader, which
    /// closes the connection and stops the generation.
    fn chat(&self, request: &Request, on_text: &mut dyn FnMut(&str)) -> AppResult<Reply> {
        let url = self.url.clone();
        let mut http = agent().post(format!("{url}/api/chat"));
        if let Some(key) = &self.api_key {
            http = http.header("Authorization", format!("Bearer {}", key.as_str()));
        }
        let body = self.body(request);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let read = || -> AppResult<()> {
                let response = http.send_json(&body).map_err(|e| unreachable(&url, &e))?;
                let status = response.status().as_u16();
                let mut body = response.into_body();
                if status != 200 {
                    let text = body.read_to_string().unwrap_or_default();
                    return Err(server_error(status, &text));
                }
                for line in BufReader::new(body.into_reader()).lines() {
                    let line = line.map_err(|e| AppError::Network(e.to_string()))?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    let value = serde_json::from_str(&line).map_err(|_| {
                        AppError::Network("Unexpected reply from the model server".to_string())
                    })?;
                    if tx.send(Chunk::Line(value)).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            };
            let end = match read() {
                Ok(()) => Chunk::End,
                Err(e) => Chunk::Failed(e),
            };
            let _ = tx.send(end);
        });

        let mut reply = Reply::default();
        let mut raw = String::new();
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                reply.stopped = true;
                return Ok(reply);
            }
            let line = match rx.recv_timeout(CANCEL_POLL) {
                Ok(Chunk::Line(line)) => line,
                Ok(Chunk::End) | Err(RecvTimeoutError::Disconnected) => return Ok(reply),
                Ok(Chunk::Failed(e)) => return Err(e),
                Err(RecvTimeoutError::Timeout) => continue,
            };
            if let Some(error) = line["error"].as_str() {
                return Err(AppError::Network(error.to_string()));
            }
            let message = &line["message"];
            if let Some(delta) = message["content"].as_str()
                && !delta.is_empty()
            {
                raw.push_str(delta);
                let shown = visible_text(&raw);
                if shown != reply.text {
                    reply.text = shown.to_string();
                    on_text(&reply.text);
                }
            }
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                reply.tool_calls.push(ToolCall {
                    id: call["id"].as_str().unwrap_or_default().to_string(),
                    name: call["function"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    arguments: call["function"]["arguments"].clone(),
                });
            }
            if line["done"].as_bool() == Some(true) {
                return Ok(reply);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_url(" http://localhost:11434/ ").unwrap(),
            "http://localhost:11434"
        );
        assert!(normalize_url("https://ollama.example.com").is_ok());
        for bad in ["localhost:11434", "ftp://x", "http://", "http://a b"] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn builds_native_chat_messages() {
        let cancel = Arc::new(AtomicBool::new(false));
        let ollama = Ollama::new("http://localhost:11434", "qwen3.8:27b", None, cancel).unwrap();
        let turns = [
            Turn::user("hi"),
            Turn::Assistant {
                text: String::new(),
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "search".into(),
                    arguments: json!({ "query": "aws" }),
                }],
                stopped: false,
            },
            Turn::Tool {
                call_id: "c1".into(),
                name: "search".into(),
                result: "nothing".into(),
            },
        ];
        let tools = json!([]);
        let body = ollama.body(&Request {
            system: "sys",
            turns: &turns,
            tools: Some(&tools),
        });
        assert_eq!(body["think"], false);
        assert_eq!(body["options"]["num_ctx"], NUM_CTX);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"]["query"],
            "aws"
        );
        assert_eq!(body["messages"][3]["tool_name"], "search");
        let body = ollama.body(&Request {
            system: "sys",
            turns: &turns,
            tools: None,
        });
        assert!(body.get("tools").is_none());
    }
}
