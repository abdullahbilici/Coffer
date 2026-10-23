//! The assistant: its settings, conversations, and replies that run on a background thread and
//! stream to the frontend as `assistant-event`s.
//!
//! History is sealed with the vault key, so reading or adding to it needs the unlock window. It
//! is not part of backups, so changing it doesn't mark the vault changed (`active_key` rather
//! than `require_unlocked`).

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

use super::{
    required_name,
    sealing::{open_spans, open_with, seal_with},
};
use crate::{
    assistant::{
        Turn,
        agent::{self, Hooks},
        handles::{Handle, Source},
        ollama::{self, Ollama},
        system_prompt, title_from, tools,
    },
    crypto::{self, VaultKey},
    db::{
        SecureBody, conversations, items, projects,
        settings::{self, Settings},
        todos,
    },
    error::{AppError, AppResult},
    state::AppState,
    time::{local_today, now_millis},
};

#[cfg(debug_assertions)]
const API_KEY_ACCOUNT: &str = "assistant-api-key-dev";
#[cfg(not(debug_assertions))]
const API_KEY_ACCOUNT: &str = "assistant-api-key";
const API_KEY_LABEL: &str = "assistant API key";

/// Mirrored by `useAssistant`.
const ASSISTANT_EVENT: &str = "assistant-event";

const MAX_CONTEXT_LEN: usize = 20_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    id: i64,
    title: String,
    created_at: i64,
    updated_at: i64,
    /// A reply is being written right now.
    replying: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    id: i64,
    created_at: i64,
    #[serde(flatten)]
    turn: Turn,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    conversation: Conversation,
    message: Message,
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum AssistantEvent {
    /// The reply's text so far, before its turn is stored.
    Text {
        conversation_id: i64,
        text: String,
    },
    /// A stored turn of the reply.
    Message {
        conversation_id: i64,
        message: Message,
    },
    Done {
        conversation_id: i64,
    },
    Failed {
        conversation_id: i64,
        message: String,
    },
}

/// The key that seals history; fails with `Locked` outside the unlock window.
fn history_key(state: &AppState) -> AppResult<VaultKey> {
    state.session().active_key()
}

fn seal_turn(key: &VaultKey, turn: &Turn) -> AppResult<SecureBody> {
    let json = Zeroizing::new(serde_json::to_string(turn).expect("a turn always serializes"));
    seal_with(key, &json)
}

fn open_turn(key: &VaultKey, sealed: &SecureBody) -> AppResult<Turn> {
    let json = open_with(key, None, sealed)?;
    serde_json::from_str(&json).map_err(|_| AppError::Crypto("Corrupted message".to_string()))
}

fn load_messages(state: &AppState, id: i64) -> AppResult<Vec<Message>> {
    let conn = state.db();
    let key = history_key(state)?;
    conversations::messages(&conn, id)?
        .into_iter()
        .map(|row| {
            Ok(Message {
                id: row.id,
                created_at: row.created_at,
                turn: open_turn(&key, &row.turn)?,
            })
        })
        .collect()
}

#[tauri::command]
pub fn save_assistant_settings(
    state: State<AppState>,
    url: String,
    model: String,
    context: String,
    api_key: Option<String>,
) -> AppResult<Settings> {
    let api_key = api_key.map(Zeroizing::new);
    let conn = state.db();
    state.session().require_unlocked()?;
    let url = ollama::normalize_url(&url)?;
    let model = required_name(&model, "Model")?;
    if context.len() > MAX_CONTEXT_LEN {
        return Err(AppError::Invalid("That text is too long".to_string()));
    }
    // `None` keeps the stored key, an empty one removes it.
    match api_key.as_deref().map(|k| k.trim()) {
        None => {}
        Some("") => {
            crypto::delete_secret(API_KEY_ACCOUNT, API_KEY_LABEL)?;
            settings::set_assistant_key_set(&conn, false)?;
        }
        Some(key) => {
            crypto::store_secret(API_KEY_ACCOUNT, key.as_bytes(), API_KEY_LABEL)?;
            settings::set_assistant_key_set(&conn, true)?;
        }
    }
    settings::set_assistant(&conn, &url, &model, context.trim())?;
    settings::load(&conn)
}

fn stored_api_key() -> AppResult<Option<Zeroizing<String>>> {
    let Some(bytes) = crypto::load_secret(API_KEY_ACCOUNT, API_KEY_LABEL)? else {
        return Ok(None);
    };
    String::from_utf8(bytes.to_vec())
        .map(|key| Some(Zeroizing::new(key)))
        .map_err(|_| AppError::Keychain("Corrupted assistant API key".to_string()))
}

/// Checks a server and lists its models. Takes the form's values, so a URL can be tried before
/// it's saved; the stored API key only ever goes to the saved URL.
#[tauri::command]
pub async fn assistant_models(
    state: State<'_, AppState>,
    url: String,
    api_key: Option<String>,
) -> AppResult<Vec<String>> {
    state.session().require_entered()?;
    let assistant = settings::load(&state.db())?.assistant;
    let typed = api_key
        .map(Zeroizing::new)
        .filter(|key| !key.trim().is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        let url = ollama::normalize_url(&url)?;
        let key = match typed {
            Some(key) => Some(key),
            None if assistant.has_api_key && url == assistant.url => stored_api_key()?,
            None => None,
        };
        ollama::list_models(&url, key.as_deref().map(String::as_str))
    })
    .await
    .map_err(|e| AppError::Network(e.to_string()))?
}

#[tauri::command]
pub fn list_conversations(state: State<AppState>) -> AppResult<Vec<Conversation>> {
    let rows = conversations::list(&state.db())?;
    let key = history_key(&state)?;
    let replies = state.replies();
    rows.into_iter()
        .map(|row| {
            Ok(Conversation {
                id: row.id,
                title: open_with(&key, None, &row.title)?.to_string(),
                created_at: row.created_at,
                updated_at: row.updated_at,
                replying: replies.contains_key(&row.id),
            })
        })
        .collect()
}

#[tauri::command]
pub fn get_conversation(state: State<AppState>, id: i64) -> AppResult<Vec<Message>> {
    load_messages(&state, id)
}

#[tauri::command]
pub fn rename_conversation(state: State<AppState>, id: i64, title: String) -> AppResult<()> {
    let title = required_name(&title, "Title")?;
    let conn = state.db();
    let sealed = seal_with(&history_key(&state)?, &title)?;
    conversations::rename(&conn, id, &sealed)
}

#[tauri::command]
pub fn delete_conversation(state: State<AppState>, id: i64) -> AppResult<()> {
    let conn = state.db();
    history_key(&state)?;
    if let Some(cancel) = state.replies().get(&id) {
        cancel.store(true, Ordering::Relaxed);
    }
    conversations::delete(&conn, id)
}

/// Stores the user's message (in a new conversation without an id) and starts the reply in the
/// background. Returns at once; the reply arrives as events.
#[tauri::command]
pub fn send_message(
    app: AppHandle,
    state: State<AppState>,
    conversation_id: Option<i64>,
    text: String,
) -> AppResult<Sent> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::Invalid("Message is empty".to_string()));
    }
    let (conversation, message) = {
        let conn = state.db();
        let key = history_key(&state)?;
        let now = now_millis();
        let (id, title, created_at) = match conversation_id {
            Some(id) => {
                let row = conversations::list(&conn)?
                    .into_iter()
                    .find(|row| row.id == id)
                    .ok_or(AppError::NotFound)?;
                (
                    id,
                    open_with(&key, None, &row.title)?.to_string(),
                    row.created_at,
                )
            }
            None => {
                let title = title_from(&text);
                let id = conversations::insert(&conn, &seal_with(&key, &title)?, now)?;
                (id, title, now)
            }
        };
        let turn = Turn::User { text };
        if state.replies().contains_key(&id) {
            return Err(AppError::Invalid(
                "Still answering the last message".to_string(),
            ));
        }
        let message_id = conversations::insert_message(&conn, id, &seal_turn(&key, &turn)?, now)?;
        let conversation = Conversation {
            id,
            title,
            created_at,
            updated_at: now,
            replying: true,
        };
        let message = Message {
            id: message_id,
            created_at: now,
            turn,
        };
        (conversation, message)
    };

    let cancel = Arc::new(AtomicBool::new(false));
    state.replies().insert(conversation.id, Arc::clone(&cancel));
    let id = conversation.id;
    thread::spawn(move || {
        let result = reply(&app, id, &cancel);
        let state = app.state::<AppState>();
        state.replies().remove(&id);
        let event = match result {
            Ok(()) => AssistantEvent::Done {
                conversation_id: id,
            },
            Err(e) => AssistantEvent::Failed {
                conversation_id: id,
                message: e.to_string(),
            },
        };
        let _ = app.emit(ASSISTANT_EVENT, event);
    });
    Ok(Sent {
        conversation,
        message,
    })
}

/// Runs the tool loop for the conversation's last message, storing each turn as it completes.
fn reply(app: &AppHandle, id: i64, cancel: &Arc<AtomicBool>) -> AppResult<()> {
    let state = app.state::<AppState>();
    let assistant = settings::load(&state.db())?.assistant;
    let mut history: Vec<Turn> = load_messages(&state, id)?
        .into_iter()
        .map(|m| m.turn)
        .collect();
    let api_key = if assistant.has_api_key {
        stored_api_key()?
    } else {
        None
    };
    let model = Ollama::new(
        &assistant.url,
        &assistant.model,
        api_key,
        Arc::clone(cancel),
    )?;
    let system = system_prompt(&local_today("EEEE, d MMMM yyyy"), &assistant.context);
    let today = local_today("yyyy-MM-dd");
    let specs = tools::specs();
    let emit = |event: AssistantEvent| {
        let _ = app.emit(ASSISTANT_EVENT, event);
    };

    agent::respond(
        &model,
        &system,
        &specs,
        &mut history,
        Hooks {
            on_text: &mut |text| {
                emit(AssistantEvent::Text {
                    conversation_id: id,
                    text: text.to_string(),
                })
            },
            save: &mut |turn| {
                let conn = state.db();
                let sealed = seal_turn(&history_key(&state)?, turn)?;
                let now = now_millis();
                let message_id = conversations::insert_message(&conn, id, &sealed, now)?;
                emit(AssistantEvent::Message {
                    conversation_id: id,
                    message: Message {
                        id: message_id,
                        created_at: now,
                        turn: turn.clone(),
                    },
                });
                Ok(())
            },
            run_tool: &mut |call| {
                let conn = state.db();
                state.session().require_entered()?;
                Ok(tools::run(&conn, &today, &call.name, &call.arguments))
            },
            cancelled: &|| cancel.load(Ordering::Relaxed),
        },
    )
}

#[tauri::command]
pub fn cancel_reply(state: State<AppState>, conversation_id: i64) {
    if let Some(cancel) = state.replies().get(&conversation_id) {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// The value behind a handle the assistant wrote, for the user to see or copy.
#[tauri::command]
pub fn reveal_handle(state: State<AppState>, handle: String) -> AppResult<String> {
    let handle = Handle::parse(&handle)?;
    let conn = state.db();
    let key = state.session().active_key()?;
    let sealed = match handle.source {
        Source::Note => {
            let encrypted = items::encrypted(&conn, handle.id)?;
            if encrypted.template.is_none() {
                return match handle.index {
                    None => Ok(open_with(&key, None, &encrypted.sealed)?.to_string()),
                    Some(_) => Err(AppError::NotFound),
                };
            }
            encrypted.sealed
        }
        Source::Overview => projects::readme_secrets(&conn, handle.id)?.1,
        Source::Todo => todos::description_secrets(&conn, handle.id)?.1,
    };
    let index = handle.index.ok_or(AppError::NotFound)?;
    open_spans(&key, &sealed)?
        .get(index)
        .cloned()
        .ok_or(AppError::NotFound)
}
