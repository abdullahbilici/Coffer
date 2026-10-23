//! The vault's secrets, decrypted for one reply of the in-app assistant while the unlock window
//! is open. The local model reads them next to their handles (`[n12.0: value]`), so it can
//! answer about them; what the user sees always comes from the vault through the handle. The
//! MCP server never builds one.

use std::collections::HashMap;

use rusqlite::Connection;
use zeroize::Zeroizing;

use super::{
    handles::{Handle, Source},
    inputs,
};
use crate::{
    commands::sealing::{open_spans, open_with},
    crypto::VaultKey,
    db::{items, projects, todos},
    error::{AppError, AppResult},
    secrets,
};

#[derive(Default)]
pub struct Plain {
    /// Inline secrets by where they live, in marker order.
    spans: HashMap<(Source, i64), Zeroizing<Vec<String>>>,
    /// Whole bodies of secure notes.
    secure: HashMap<i64, Zeroizing<String>>,
}

impl Plain {
    pub fn open(conn: &Connection, key: &VaultKey) -> AppResult<Self> {
        let mut plain = Self::default();
        for item in items::list(conn)? {
            if !item.secure && !item.has_secrets {
                continue;
            }
            let encrypted = items::encrypted(conn, item.id)?;
            if encrypted.template.is_some() {
                plain
                    .spans
                    .insert((Source::Note, item.id), open_spans(key, &encrypted.sealed)?);
            } else {
                plain
                    .secure
                    .insert(item.id, open_with(key, None, &encrypted.sealed)?);
            }
        }
        for project in projects::list(conn)? {
            if let Some((_, sealed)) = found(projects::readme_secrets(conn, project.id))? {
                plain
                    .spans
                    .insert((Source::Overview, project.id), open_spans(key, &sealed)?);
            }
        }
        for todo in todos::list(conn)? {
            if let Some((_, sealed)) = found(todos::description_secrets(conn, todo.id))? {
                plain
                    .spans
                    .insert((Source::Todo, todo.id), open_spans(key, &sealed)?);
            }
        }
        Ok(plain)
    }

    /// A template as the model reads it: each `` `!n!` `` marker becomes its handle and value.
    pub fn show(&self, template: &str, source: Source, id: i64) -> String {
        let values = self.spans.get(&(source, id));
        secrets::replace_markers(template, |index| {
            let handle = Handle {
                source,
                id,
                index: Some(index),
            };
            match values.and_then(|v| v.get(index)) {
                Some(value) => format!("[{}: {value}]", inner(handle)),
                None => handle.to_string(),
            }
        })
    }

    /// A secure note's body with its values and their handles.
    pub fn secure_body(&self, id: i64) -> Option<String> {
        self.secure
            .get(&id)
            .map(|body| inputs::note_values(id, body).0.to_string())
    }

    /// Every secret with its handle, for checking what a reply shows (`redact`).
    pub fn secrets(&self) -> Vec<(Handle, Zeroizing<String>)> {
        let mut out = Vec::new();
        for (&(source, id), values) in &self.spans {
            for (index, value) in values.iter().enumerate() {
                let handle = Handle {
                    source,
                    id,
                    index: Some(index),
                };
                out.push((handle, Zeroizing::new(value.clone())));
            }
        }
        for (&id, body) in &self.secure {
            let whole = Handle {
                source: Source::Note,
                id,
                index: None,
            };
            out.push((whole, Zeroizing::new(body.trim().to_string())));
            for (index, value) in inputs::note_values(id, body).1.iter().enumerate() {
                let handle = Handle {
                    source: Source::Note,
                    id,
                    index: Some(index),
                };
                out.push((handle, Zeroizing::new(value.clone())));
            }
        }
        out
    }
}

/// `n12.0` for `[n12.0]`.
fn inner(handle: Handle) -> String {
    let text = handle.to_string();
    text[1..text.len() - 1].to_string()
}

fn found<T>(result: AppResult<T>) -> AppResult<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(AppError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}
