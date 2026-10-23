//! Drafts: the assistant's write tools never touch the vault. Each call checks one change
//! against the vault as the draft so far would leave it, and adds it to the reply's draft, or
//! answers with a short error the model can fix. The user sees the finished draft as a diff and
//! applies it (Touch ID) in one transaction, or discards it.
//!
//! Values never pass through the model: a field's value is an input handle (`[u3]`, a value from
//! the user's message), a secret handle (`[n12.0]`, a value already in the vault), or a short
//! word the model saw as it is. The server puts the real value in when the draft is applied.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    fields,
    handles::{self, Handle, Source},
    inputs::{self, Inputs},
    spend,
    tools::{clip, int_arg, str_arg},
};
use crate::{
    commands::{
        sealing::{open_with, seal_inline_with, seal_with},
        valid_date,
    },
    crypto::VaultKey,
    db::{
        Change, folders,
        items::{self, Body, ItemRecord},
        payments::{self, Payment, PaymentRecord},
        projects,
        todos::{self, Todo, TodoRecord},
    },
    error::{AppError, AppResult},
};

/// Longest name or title a draft accepts, like the editors.
const MAX_NAME: usize = 200;
/// Longest value the model may pass as written instead of as a handle.
const MAX_WORD: usize = 64;

/// Something that exists in the vault (`Id`) or that op `New(i)` of the draft creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ref {
    Id(i64),
    New(usize),
}

/// Where a field's value comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Val {
    /// `u3`: a value from the user's messages.
    Input(usize),
    /// `n12.0`: a secret already in the vault.
    Secret(String),
    /// A short word the user wrote and the model saw as it is.
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub label: String,
    pub value: Val,
    pub secret: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    CreateProject {
        name: String,
    },
    CreateFolder {
        project: Ref,
        parent: Option<Ref>,
        name: String,
    },
    /// Field changes to a note created in the same draft land in its `fields`.
    CreateNote {
        project: Ref,
        folder: Option<Ref>,
        title: String,
        tags: Vec<String>,
        fields: Vec<Field>,
    },
    AddField {
        note: i64,
        field: Field,
    },
    UpdateField {
        note: i64,
        label: String,
        value: Val,
        /// `None` keeps whether the old value was secret.
        secret: Option<bool>,
    },
    RemoveField {
        note: i64,
        label: String,
    },
    RenameNote {
        note: i64,
        title: String,
    },
    RenameFolder {
        folder: i64,
        name: String,
    },
    RenameProject {
        project: i64,
        name: String,
    },
    MoveNote {
        note: i64,
        project: Ref,
        folder: Option<Ref>,
    },
    DeleteNote {
        note: i64,
    },
    CreateTodo {
        project: Option<Ref>,
        title: String,
        due: Option<String>,
    },
    UpdateTodo {
        todo: i64,
        title: Option<String>,
        due: Option<String>,
    },
    CompleteTodo {
        todo: i64,
    },
    CreatePayment {
        project: Option<Ref>,
        name: String,
        /// Integer hundredths.
        amount: i64,
        currency: String,
        cycle: String,
        start: String,
    },
    UpdatePayment {
        payment: i64,
        name: Option<String>,
        amount: Option<i64>,
        /// The first due date the new amount applies to.
        from: String,
    },
    CancelPayment {
        payment: i64,
        date: String,
    },
}

/// What a reply has drafted so far.
#[derive(Debug, Default)]
pub struct Draft {
    pub ops: Vec<Op>,
}

/// What write tools need besides the draft.
pub struct Ctx<'a> {
    pub conn: &'a Connection,
    pub today: &'a str,
    pub inputs: &'a Inputs,
    /// Values already in the vault (secrets included), with the note holding each, so a draft
    /// skips what is stored. Built once per reply by `stored_values`.
    pub stored: &'a HashMap<String, i64>,
}

/// Every field value in the vault's notes, decrypted, by the note that holds it. Used only to
/// leave out values a draft would store twice; none of it reaches the model.
pub fn stored_values(conn: &Connection, key: Option<&VaultKey>) -> AppResult<HashMap<String, i64>> {
    let mut stored = HashMap::new();
    for (id, template) in items::templates(conn)? {
        let text = match key {
            Some(key) if template.contains("`!") => match items::encrypted(conn, id) {
                Ok(encrypted) => open_with(key, encrypted.template.as_deref(), &encrypted.sealed)?,
                Err(_) => Zeroizing::new(template),
            },
            _ => Zeroizing::new(template),
        };
        for line in text.lines() {
            if let Some(value) = fields::value_of(line) {
                stored.entry(value.to_string()).or_insert(id);
            }
        }
    }
    Ok(stored)
}

// --- the vault as a draft would leave it ---------------------------------------------

struct WsProject {
    id: Ref,
    name: String,
}

struct WsFolder {
    id: Ref,
    project: Ref,
    parent: Option<Ref>,
    name: String,
}

struct WsNote {
    id: Ref,
    project: Ref,
    folder: Option<Ref>,
    title: String,
    secure: bool,
    /// The readable body: a template for existing notes; secrets show as `` `!…!` ``.
    text: String,
    deleted: bool,
}

struct Workspace {
    projects: Vec<WsProject>,
    folders: Vec<WsFolder>,
    notes: Vec<WsNote>,
    todos: Vec<Todo>,
    payments: Vec<Payment>,
}

/// Words that say nothing about which project is meant ("Acme projesi", "side project - x").
const NAME_NOISE: [&str; 12] = [
    "project", "projects", "projesi", "proje", "isleri", "hobi", "side", "yeni", "musteri", "new",
    "client", "the",
];

fn name_words(name: &str) -> Vec<String> {
    let folded: String = fields::normalize(name)
        .chars()
        .map(|c| match c {
            'ş' => 's',
            'ğ' => 'g',
            'ü' => 'u',
            'ö' => 'o',
            'ç' => 'c',
            c => c,
        })
        .collect();
    let words: Vec<String> = folded.split(' ').map(str::to_string).collect();
    let meaningful: Vec<String> = words
        .iter()
        .filter(|w| !NAME_NOISE.contains(&w.as_str()))
        .cloned()
        .collect();
    if meaningful.is_empty() {
        words
    } else {
        meaningful
    }
}

/// Whether two project names mean the same project: the same words, or one name the start of
/// the other ("acme" and "Acme Shop").
fn same_name(a: &[String], b: &[String]) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    !short.is_empty() && short[0].chars().count() >= 3 && long.starts_with(short)
}

fn bad(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

impl Workspace {
    fn load(ctx: &Ctx, ops: &[Op]) -> AppResult<Self> {
        let conn = ctx.conn;
        let templates: HashMap<i64, String> = items::templates(conn)?.into_iter().collect();
        let mut ws = Self {
            projects: projects::list(conn)?
                .into_iter()
                .map(|p| WsProject {
                    id: Ref::Id(p.id),
                    name: p.name,
                })
                .collect(),
            folders: folders::list(conn)?
                .into_iter()
                .map(|f| WsFolder {
                    id: Ref::Id(f.id),
                    project: Ref::Id(f.project_id),
                    parent: f.parent_id.map(Ref::Id),
                    name: f.name,
                })
                .collect(),
            notes: items::list(conn)?
                .into_iter()
                .map(|i| WsNote {
                    id: Ref::Id(i.id),
                    project: Ref::Id(i.project_id),
                    folder: i.folder_id.map(Ref::Id),
                    text: templates.get(&i.id).cloned().unwrap_or_default(),
                    title: i.title,
                    secure: i.secure,
                    deleted: false,
                })
                .collect(),
            todos: todos::list(conn)?,
            payments: payments::list(conn)?,
        };
        for (i, op) in ops.iter().enumerate() {
            ws.replay(ctx.inputs, i, op);
        }
        Ok(ws)
    }

    /// Applies an op that already passed its checks.
    fn replay(&mut self, inputs: &Inputs, i: usize, op: &Op) {
        let shown = |value: &Val, secret: bool| {
            if secret {
                "!…!".to_string()
            } else {
                plain_value(inputs, value).unwrap_or_default()
            }
        };
        match op {
            Op::CreateProject { name } => self.projects.push(WsProject {
                id: Ref::New(i),
                name: name.clone(),
            }),
            Op::CreateFolder {
                project,
                parent,
                name,
            } => self.folders.push(WsFolder {
                id: Ref::New(i),
                project: *project,
                parent: *parent,
                name: name.clone(),
            }),
            Op::CreateNote {
                project,
                folder,
                title,
                fields: list,
                ..
            } => {
                let text = list.iter().fold(String::new(), |body, f| {
                    fields::add(&body, &f.label, &shown(&f.value, f.secret), f.secret)
                });
                self.notes.push(WsNote {
                    id: Ref::New(i),
                    project: *project,
                    folder: *folder,
                    title: title.clone(),
                    secure: false,
                    text,
                    deleted: false,
                });
            }
            Op::AddField { note, field } => {
                if let Some(n) = self.note_mut(*note) {
                    let value = shown(&field.value, field.secret);
                    n.text = fields::add(&n.text, &field.label, &value, field.secret);
                }
            }
            Op::UpdateField {
                note,
                label,
                value,
                secret,
            } => {
                if let Some(n) = self.note_mut(*note) {
                    let keep = fields::find(&n.text, label)
                        .ok()
                        .and_then(|l| n.text.lines().nth(l).map(fields::is_secret));
                    let is_secret = secret.or(keep).unwrap_or(true);
                    let value = shown(value, is_secret);
                    if let Ok(text) = fields::update(&n.text, label, &value, Some(is_secret)) {
                        n.text = text;
                    }
                }
            }
            Op::RemoveField { note, label } => {
                if let Some(n) = self.note_mut(*note)
                    && let Ok(text) = fields::remove(&n.text, label)
                {
                    n.text = text;
                }
            }
            Op::RenameNote { note, title } => {
                if let Some(n) = self.note_mut(*note) {
                    n.title = title.clone();
                }
            }
            Op::RenameFolder { folder, name } => {
                if let Some(f) = self.folders.iter_mut().find(|f| f.id == Ref::Id(*folder)) {
                    f.name = name.clone();
                }
            }
            Op::RenameProject { project, name } => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == Ref::Id(*project)) {
                    p.name = name.clone();
                }
            }
            Op::MoveNote {
                note,
                project,
                folder,
            } => {
                if let Some(n) = self.note_mut(*note) {
                    n.project = *project;
                    n.folder = *folder;
                }
            }
            Op::DeleteNote { note } => {
                if let Some(n) = self.note_mut(*note) {
                    n.deleted = true;
                }
            }
            Op::CompleteTodo { todo } => {
                if let Some(t) = self.todos.iter_mut().find(|t| t.id == *todo) {
                    t.done = true;
                }
            }
            Op::CreateTodo { .. }
            | Op::UpdateTodo { .. }
            | Op::CreatePayment { .. }
            | Op::UpdatePayment { .. }
            | Op::CancelPayment { .. } => {}
        }
    }

    fn note_mut(&mut self, id: i64) -> Option<&mut WsNote> {
        self.notes.iter_mut().find(|n| n.id == Ref::Id(id))
    }

    fn note(&self, id: Ref) -> Option<&WsNote> {
        self.notes.iter().find(|n| n.id == id && !n.deleted)
    }

    fn project_name(&self, id: Ref) -> &str {
        self.projects
            .iter()
            .find(|p| p.id == id)
            .map_or("?", |p| p.name.as_str())
    }

    fn folder_path(&self, mut id: Option<Ref>) -> String {
        let mut names = Vec::new();
        while let Some(folder) = id.and_then(|id| self.folders.iter().find(|f| f.id == id)) {
            names.push(folder.name.as_str());
            id = folder.parent;
        }
        names.reverse();
        names.join("/")
    }

    fn place(&self, project: Ref, folder: Option<Ref>) -> String {
        match self.folder_path(folder) {
            path if path.is_empty() => self.project_name(project).to_string(),
            path => format!("{} / {path}", self.project_name(project)),
        }
    }

    fn project_list(&self) -> String {
        self.projects
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The project `text` names: an id, the same name, or a name it starts or is the start of.
    /// `Ok(None)` when nothing matches; an error when several do.
    fn find_project(&self, text: &str) -> AppResult<Option<Ref>> {
        let text = text.trim().trim_start_matches('#');
        if let Ok(id) = text.parse::<i64>() {
            return match self.projects.iter().find(|p| p.id == Ref::Id(id)) {
                Some(p) => Ok(Some(p.id)),
                None => Err(bad(format!(
                    "there is no project {id}. Projects: {}",
                    self.project_list()
                ))),
            };
        }
        let words = name_words(text);
        let exact: Vec<Ref> = self
            .projects
            .iter()
            .filter(|p| name_words(&p.name) == words)
            .map(|p| p.id)
            .collect();
        let found: Vec<Ref> = if exact.is_empty() {
            self.projects
                .iter()
                .filter(|p| same_name(&name_words(&p.name), &words))
                .map(|p| p.id)
                .collect()
        } else {
            exact
        };
        match found[..] {
            [] => Ok(None),
            [id] => Ok(Some(id)),
            _ => Err(bad(format!(
                "\"{text}\" could be {}; use one of these names",
                found
                    .iter()
                    .map(|id| format!("\"{}\"", self.project_name(*id)))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ))),
        }
    }

    /// A project that must exist.
    fn existing_project(&self, text: &str) -> AppResult<Ref> {
        self.find_project(text)?.ok_or_else(|| {
            bad(format!(
                "no project matches \"{}\". Projects: {}",
                text.trim(),
                self.project_list()
            ))
        })
    }

    /// The deepest existing folder along `path` in `project`, and the names left to create.
    fn folder_path_parts(&self, project: Ref, path: &str) -> AppResult<(Option<Ref>, Vec<String>)> {
        let path = path.trim().trim_matches('/');
        if let Ok(id) = path.trim_start_matches('#').parse::<i64>() {
            return match self.folders.iter().find(|f| f.id == Ref::Id(id)) {
                Some(f) if f.project == project => Ok((Some(f.id), Vec::new())),
                Some(_) => Err(bad(format!("folder {id} is in another project"))),
                None => Err(bad(format!("there is no folder {id}"))),
            };
        }
        let mut parent = None;
        let segments: Vec<&str> = path
            .split('/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        for (i, segment) in segments.iter().enumerate() {
            let wanted = fields::normalize(segment);
            match self.folders.iter().find(|f| {
                f.project == project && f.parent == parent && fields::normalize(&f.name) == wanted
            }) {
                Some(f) => parent = Some(f.id),
                None => {
                    return Ok((
                        parent,
                        segments[i..].iter().map(|s| s.to_string()).collect(),
                    ));
                }
            }
        }
        Ok((parent, Vec::new()))
    }

    fn existing_note(&self, args: &Value, name: &str) -> AppResult<i64> {
        let raw = args.get(name);
        let id = match raw.and_then(Value::as_str) {
            Some(text) => {
                let text = text.trim().trim_start_matches('[').trim_end_matches(']');
                let text = text.strip_prefix('n').unwrap_or(text);
                text.trim_start_matches('#')
                    .parse::<i64>()
                    .map_err(|_| bad(format!("{name} must be a note id (a number)")))?
            }
            None => int_arg(args, name)?,
        };
        match self.note(Ref::Id(id)) {
            Some(_) => Ok(id),
            None => Err(bad(format!("there is no note {id}"))),
        }
    }

    /// Where a value is stored already: a note of the vault, or one this draft creates.
    fn stored_in(&self, ctx: &Ctx, ops: &[Op], value: &Val) -> Option<String> {
        if let Val::Input(n) = value
            && ops.iter().any(|op| op_uses_input(op, *n))
        {
            return Some("already in this draft".to_string());
        }
        let text = resolve_known(ctx.inputs, value)?;
        if text.chars().count() < 4 {
            return None;
        }
        let id = ctx.stored.get(text.as_str())?;
        let note = self.note(Ref::Id(*id))?;
        Some(format!("already stored in note {id} ({})", note.title))
    }
}

fn op_uses_input(op: &Op, n: usize) -> bool {
    let is = |v: &Val| *v == Val::Input(n);
    match op {
        Op::CreateNote { fields, .. } => fields.iter().any(|f| is(&f.value)),
        Op::AddField { field, .. } => is(&field.value),
        Op::UpdateField { value, .. } => is(value),
        _ => false,
    }
}

/// The value of a handle or word the draft can know without a key: inputs and words.
fn resolve_known(inputs: &Inputs, value: &Val) -> Option<String> {
    match value {
        Val::Input(n) => inputs.get(*n).map(str::to_string),
        Val::Text(text) => Some(text.clone()),
        Val::Secret(_) => None,
    }
}

fn plain_value(inputs: &Inputs, value: &Val) -> Option<String> {
    match value {
        Val::Secret(handle) => Some(format!("[{handle}]")),
        other => resolve_known(inputs, other),
    }
}

/// The vault as an import sees it, the draft so far included: every project with its folders,
/// and the notes (with their field labels) of the projects `text` seems to mention.
pub fn outline(ctx: &Ctx, ops: &[Op], text: &str) -> AppResult<String> {
    let ws = Workspace::load(ctx, ops)?;
    if ws.projects.is_empty() {
        return Ok("The vault is empty.".to_string());
    }
    let mentioned: HashSet<String> = name_words(text).into_iter().collect();
    let mut out = String::new();
    for p in &ws.projects {
        let id = match p.id {
            Ref::Id(id) => format!("project {id}"),
            Ref::New(_) => "drafted".to_string(),
        };
        out.push_str(&format!("- {} ({id})", p.name));
        let folders: Vec<String> = ws
            .folders
            .iter()
            .filter(|f| f.project == p.id)
            .map(|f| ws.folder_path(Some(f.id)))
            .collect();
        if !folders.is_empty() {
            out.push_str(&format!("; folders: {}", folders.join(", ")));
        }
        out.push('\n');
        let words = name_words(&p.name);
        if !words
            .iter()
            .any(|w| w.chars().count() >= 3 && mentioned.contains(w))
        {
            continue;
        }
        for n in ws.notes.iter().filter(|n| n.project == p.id && !n.deleted) {
            let id = match n.id {
                Ref::Id(id) => format!("note {id}"),
                Ref::New(_) => "drafted note".to_string(),
            };
            let place = ws.folder_path(n.folder);
            let place = if place.is_empty() {
                String::new()
            } else {
                format!(" in {place}")
            };
            let labels = fields::labels(&n.text).join(", ");
            out.push_str(&format!("  - {id} \"{}\"{place}: {labels}\n", n.title));
        }
    }
    Ok(clip(out.trim_end(), 8_000))
}

// --- the write tools ----------------------------------------------------------------

/// The write tools in Ollama's format.
pub fn specs() -> Vec<Value> {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": { "type": "object", "properties": properties, "required": required },
            },
        })
    };
    let string = |description: &str| json!({ "type": "string", "description": description });
    let value = string(
        "A handle: [u3] for a value from the user's message, [n12.0] to copy a stored secret. A short word the user wrote plainly may be given as written.",
    );
    let secret = json!({
        "type": "boolean",
        "description": "true for passwords, keys, tokens, PINs, recovery codes and private URLs; false for usernames, emails, hosts, public URLs and IDs.",
    });
    let note = json!({ "type": "integer", "description": "Note id." });
    vec![
        tool(
            "create_project",
            "Drafts a new project.",
            json!({ "name": string("Project name.") }),
            &["name"],
        ),
        tool(
            "create_folder",
            "Drafts a new folder in a project.",
            json!({
                "project": string("Project name or id."),
                "name": string("Folder name."),
                "parent": string("Parent folder path or id; empty for the project root."),
            }),
            &["project", "name"],
        ),
        tool(
            "create_note",
            "Drafts a new note with fields (one per value). A missing project or folder is drafted too.",
            json!({
                "project": string("Project name or id."),
                "folder": string("Folder path such as Infra or Infra/AWS, or empty for the project root."),
                "title": string("Short title, e.g. the service and environment."),
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional tags such as env/prod." },
                "fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "label": string("Field label, e.g. Password."), "value": value, "secret": secret },
                        "required": ["label", "value", "secret"],
                    },
                },
            }),
            &["project", "title", "fields"],
        ),
        tool(
            "add_field",
            "Drafts a new field on an existing note.",
            json!({ "note": note, "label": string("Field label."), "value": value, "secret": secret }),
            &["note", "label", "value", "secret"],
        ),
        tool(
            "update_field",
            "Drafts a new value for an existing field of a note.",
            json!({ "note": note, "label": string("The field's label as it is in the note."), "value": value }),
            &["note", "label", "value"],
        ),
        tool(
            "remove_field",
            "Drafts removing a field from a note.",
            json!({ "note": note, "label": string("The field's label as it is in the note.") }),
            &["note", "label"],
        ),
        tool(
            "rename",
            "Drafts renaming a note, folder or project.",
            json!({
                "kind": { "type": "string", "enum": ["note", "folder", "project"] },
                "id": { "type": "integer" },
                "name": string("The new name."),
            }),
            &["kind", "id", "name"],
        ),
        tool(
            "move_note",
            "Drafts moving a note into a folder (a missing folder is drafted too), or to a project's root.",
            json!({
                "note": note,
                "folder": string("Folder path or id; empty for the project root."),
                "project": string("Project name or id; leave out to stay in the note's project."),
            }),
            &["note"],
        ),
        tool(
            "delete_note",
            "Drafts deleting a note.",
            json!({ "note": note }),
            &["note"],
        ),
        tool(
            "create_todo",
            "Drafts a new todo.",
            json!({
                "title": string("What to do."),
                "project": string("Project name or id; leave out for a general todo."),
                "due": string("Due date YYYY-MM-DD, optional."),
            }),
            &["title"],
        ),
        tool(
            "update_todo",
            "Drafts changing a todo's title or due date.",
            json!({ "id": { "type": "integer" }, "title": string("New title."), "due": string("New due date YYYY-MM-DD.") }),
            &["id"],
        ),
        tool(
            "complete_todo",
            "Drafts marking a todo done.",
            json!({ "id": { "type": "integer" } }),
            &["id"],
        ),
        tool(
            "create_payment",
            "Drafts a new recurring or one-time payment.",
            json!({
                "name": string("What is paid for."),
                "amount": { "type": "number", "description": "Amount per cycle, e.g. 300 or 12.99." },
                "currency": string("Three-letter code such as TRY, USD, EUR."),
                "cycle": { "type": "string", "enum": ["monthly", "yearly", "once"] },
                "start": string("First due date YYYY-MM-DD; leave out for today."),
                "project": string("Project name or id; leave out for a personal payment."),
            }),
            &["name", "amount", "currency", "cycle"],
        ),
        tool(
            "update_payment",
            "Drafts a payment's new amount (kept in its price history) or name.",
            json!({
                "id": { "type": "integer" },
                "amount": { "type": "number" },
                "from": string("First due date the new amount applies to, YYYY-MM-DD; leave out for today."),
                "name": string("New name."),
            }),
            &["id"],
        ),
        tool(
            "cancel_payment",
            "Drafts cancelling a payment: nothing is due from the date on.",
            json!({ "id": { "type": "integer" }, "date": string("YYYY-MM-DD; leave out for today.") }),
            &["id"],
        ),
    ]
}

/// The tools an import offers: notes in bulk, and fields of notes the vault has.
pub fn import_specs() -> Value {
    let all = specs();
    let pick = |name: &str| {
        all.iter()
            .find(|t| t["function"]["name"] == name)
            .cloned()
            .expect("a write tool")
    };
    let mut note = pick("create_note");
    let properties = note["function"]["parameters"].take();
    json!([
        {
            "type": "function",
            "function": {
                "name": "add_notes",
                "description": "Drafts new notes, each with its project, optional folder, title, tags and fields.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "notes": { "type": "array", "items": properties },
                    },
                    "required": ["notes"],
                },
            },
        },
        pick("add_field"),
        pick("update_field"),
    ])
}

pub const WRITE_TOOLS: [&str; 15] = [
    "create_project",
    "create_folder",
    "create_note",
    "add_field",
    "update_field",
    "remove_field",
    "rename",
    "move_note",
    "delete_note",
    "create_todo",
    "update_todo",
    "complete_todo",
    "create_payment",
    "update_payment",
    "cancel_payment",
];

/// Runs a write tool, or `None` when `name` isn't one.
pub fn run(ctx: &Ctx, draft: &mut Draft, name: &str, args: &Value) -> Option<String> {
    if !WRITE_TOOLS.contains(&name) && name != "add_notes" {
        return None;
    }
    let result = Workspace::load(ctx, &draft.ops).and_then(|ws| {
        let mut w = Writer {
            ctx,
            ws,
            draft,
            notes: Vec::new(),
        };
        let text = match name {
            "create_project" => w.create_project(args),
            "create_folder" => w.create_folder(args),
            "create_note" => w.create_note(args),
            "add_notes" => w.add_notes(args),
            "add_field" => w.add_field(args),
            "update_field" => w.update_field(args),
            "remove_field" => w.remove_field(args),
            "rename" => w.rename(args),
            "move_note" => w.move_note(args),
            "delete_note" => w.delete_note(args),
            "create_todo" => w.create_todo(args),
            "update_todo" => w.update_todo(args),
            "complete_todo" => w.complete_todo(args),
            "create_payment" => w.create_payment(args),
            "update_payment" => w.update_payment(args),
            "cancel_payment" => w.cancel_payment(args),
            _ => unreachable!("checked above"),
        }?;
        let mut out = text;
        for note in w.notes {
            out.push_str(&format!("\n{note}"));
        }
        Ok(out)
    });
    Some(match result {
        Ok(text) => clip(&text, 3_000),
        Err(AppError::Invalid(message)) => format!("Error: {message}"),
        Err(e) => format!("Error: {e}"),
    })
}

struct Writer<'a, 'b> {
    ctx: &'a Ctx<'b>,
    ws: Workspace,
    draft: &'a mut Draft,
    /// Things worth telling the model besides the main result, such as skipped values.
    notes: Vec<String>,
}

fn opt_str<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && !matches!(*s, "null" | "none" | "/" | "root"))
}

fn name_arg(args: &Value, name: &str) -> AppResult<String> {
    let text = str_arg(args, name)?.trim();
    if text.is_empty() {
        return Err(bad(format!("{name} is empty")));
    }
    if text.chars().count() > MAX_NAME {
        return Err(bad(format!("{name} is too long")));
    }
    Ok(text.to_string())
}

fn opt_date(args: &Value, name: &str) -> AppResult<Option<String>> {
    opt_str(args, name)
        .map(|date| {
            valid_date(date)
                .map(str::to_string)
                .map_err(|_| bad(format!("{name} must be a date like 2026-10-23")))
        })
        .transpose()
}

/// An amount in hundredths; accepts 300, 12.99 or "1.249,50".
fn amount_arg(args: &Value, name: &str) -> AppResult<Option<i64>> {
    let value = match args.get(name) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => {
            let s: String = s
                .chars()
                .filter(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
                .collect();
            let s = match (s.rfind('.'), s.rfind(',')) {
                (Some(dot), Some(comma)) if comma > dot => s.replace('.', "").replace(',', "."),
                (None, Some(comma)) if s.len() - comma <= 3 => s.replace(',', "."),
                _ => s.replace(',', ""),
            };
            s.parse::<f64>().ok()
        }
        _ => None,
    };
    match value {
        Some(v) if (0.0..1e12).contains(&v) => Ok(Some((v * 100.0).round() as i64)),
        _ => Err(bad(format!("{name} must be a number like 300 or 12.99"))),
    }
}

fn currency_arg(args: &Value) -> AppResult<String> {
    let raw = str_arg(args, "currency")?.trim();
    let code = match raw.to_uppercase().as_str() {
        "TL" | "₺" | "TRL" => "TRY".to_string(),
        "$" | "US$" | "DOLLAR" => "USD".to_string(),
        "€" | "EURO" => "EUR".to_string(),
        "£" => "GBP".to_string(),
        other => other.to_string(),
    };
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(code)
    } else {
        Err(bad("currency must be a three-letter code such as USD"))
    }
}

impl Writer<'_, '_> {
    fn push(&mut self, op: Op) -> usize {
        self.ws.replay(self.ctx.inputs, self.draft.ops.len(), &op);
        self.draft.ops.push(op);
        self.draft.ops.len() - 1
    }

    /// A project to put something in: an existing one, or one drafted now.
    fn project_or_new(&mut self, text: &str) -> AppResult<Ref> {
        if let Some(id) = self.ws.find_project(text)? {
            return Ok(id);
        }
        let name = text.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(bad("project name is empty or too long"));
        }
        let i = self.push(Op::CreateProject {
            name: name.to_string(),
        });
        self.notes.push(format!("Drafted new project \"{name}\"."));
        Ok(Ref::New(i))
    }

    /// A folder path in `project`, drafting the folders it lacks.
    fn folder_or_new(&mut self, project: Ref, path: Option<&str>) -> AppResult<Option<Ref>> {
        let Some(path) = path else {
            return Ok(None);
        };
        let (mut parent, missing) = self.ws.folder_path_parts(project, path)?;
        for name in missing {
            if name.chars().count() > MAX_NAME {
                return Err(bad("folder name is too long"));
            }
            let i = self.push(Op::CreateFolder {
                project,
                parent,
                name: name.clone(),
            });
            self.notes.push(format!(
                "Drafted new folder {}.",
                self.ws.place(project, Some(Ref::New(i)))
            ));
            parent = Some(Ref::New(i));
        }
        Ok(parent)
    }

    /// A field value from the model: a handle, or a short word it saw verbatim.
    fn value(&self, raw: Option<&Value>) -> AppResult<Val> {
        let text = match raw {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => return Err(bad("value is required")),
        };
        if let Some(n) = inputs::parse(&text) {
            let value = self
                .ctx
                .inputs
                .get(n)
                .ok_or_else(|| bad(format!("there is no [u{n}] in the user's messages")))?;
            if value.contains('`') || value.contains('\n') {
                return Err(bad(format!(
                    "[u{n}] holds a backtick or line break and can't be stored as a field"
                )));
            }
            return Ok(Val::Input(n));
        }
        let unwrapped = text.trim_matches('`');
        if unwrapped.starts_with('[')
            && let Ok(handle) = Handle::parse(unwrapped)
        {
            self.check_handle(handle)?;
            return Ok(Val::Secret(
                handle.to_string().trim_matches(['[', ']']).to_string(),
            ));
        }
        let word = unwrapped;
        if word.chars().count() <= MAX_WORD
            && !word.contains('`')
            && self.ctx.inputs.shown_verbatim(word)
        {
            return Ok(Val::Text(word.to_string()));
        }
        Err(bad(
            "value must be a handle such as [u3] from the user's message; never type a value",
        ))
    }

    fn check_handle(&self, handle: Handle) -> AppResult<()> {
        let missing = || bad(format!("{handle} does not exist"));
        match handle.source {
            Source::Note => {
                let note = self.ws.note(Ref::Id(handle.id)).ok_or_else(missing)?;
                match handle.index {
                    None if note.secure => Ok(()),
                    Some(i) if note.text.contains(&format!("`!{i}!`")) => Ok(()),
                    _ => Err(missing()),
                }
            }
            Source::Overview => projects::readme_secrets(self.ctx.conn, handle.id)
                .map(|_| ())
                .map_err(|_| missing()),
            Source::Todo => todos::description_secrets(self.ctx.conn, handle.id)
                .map(|_| ())
                .map_err(|_| missing()),
        }
    }

    fn field(&self, raw: &Value) -> AppResult<Field> {
        let label = name_arg(raw, "label")?;
        let value = self.value(raw.get("value"))?;
        let secret = match &value {
            Val::Secret(_) => true,
            _ => raw.get("secret").and_then(Value::as_bool).unwrap_or(true),
        };
        Ok(Field {
            label,
            value,
            secret,
        })
    }

    fn create_project(&mut self, args: &Value) -> AppResult<String> {
        let name = name_arg(args, "name")?;
        if let Some(id) = self.ws.find_project(&name)? {
            return Ok(format!(
                "Project \"{}\" exists already; use it.",
                self.ws.project_name(id)
            ));
        }
        self.push(Op::CreateProject { name: name.clone() });
        Ok(format!("Drafted new project \"{name}\"."))
    }

    fn create_folder(&mut self, args: &Value) -> AppResult<String> {
        let project = self.ws.existing_project(str_arg(args, "project")?)?;
        let name = name_arg(args, "name")?;
        let parent = opt_str(args, "parent").unwrap_or("");
        let path = format!("{parent}/{name}");
        let (_, missing) = self.ws.folder_path_parts(project, &path)?;
        if missing.is_empty() {
            return Ok(format!(
                "Folder {} exists already.",
                self.ws.place(project, None) + " / " + path.trim_matches('/')
            ));
        }
        self.folder_or_new(project, Some(&path))?;
        Ok(String::new())
    }

    fn create_note(&mut self, args: &Value) -> AppResult<String> {
        let project_text = str_arg(args, "project")?;
        let title = name_arg(args, "title")?;
        let list: Vec<Field> = match args.get("fields") {
            Some(Value::Array(items)) => items
                .iter()
                .map(|f| self.field(f))
                .collect::<AppResult<_>>()?,
            Some(Value::String(s)) if s.trim().starts_with('[') => {
                let items: Vec<Value> =
                    serde_json::from_str(s).map_err(|_| bad("fields must be a list"))?;
                items
                    .iter()
                    .map(|f| self.field(f))
                    .collect::<AppResult<_>>()?
            }
            None | Some(Value::Null) => Vec::new(),
            _ => return Err(bad("fields must be a list of {label, value, secret}")),
        };
        let tags: Vec<String> = match args.get("tags") {
            Some(Value::Array(tags)) => tags
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        let project = self.project_or_new(project_text)?;
        let folder = self.folder_or_new(project, opt_str(args, "folder"))?;

        // Leave out values the vault or this draft holds already.
        let mut kept = Vec::new();
        for field in list {
            match self.ws.stored_in(self.ctx, &self.draft.ops, &field.value) {
                Some(why) => self.notes.push(format!("Skipped {}: {why}.", field.label)),
                None => kept.push(field),
            }
        }
        let place = self.ws.place(project, folder);
        // A note of the same title in the same place gets the new fields instead of a twin.
        let twin = self
            .ws
            .notes
            .iter()
            .find(|n| {
                !n.deleted
                    && n.project == project
                    && n.folder == folder
                    && fields::normalize(&n.title) == fields::normalize(&title)
            })
            .map(|n| n.id);
        match twin {
            Some(Ref::Id(id)) if !kept.is_empty() => {
                let labels = self.add_to_existing(id, kept)?;
                Ok(format!(
                    "Note {id} \"{title}\" exists in {place}; drafted adding {labels} to it."
                ))
            }
            Some(Ref::New(i)) if !kept.is_empty() => {
                let labels = field_labels(&kept);
                if let Some(Op::CreateNote { fields, .. }) = self.draft.ops.get_mut(i) {
                    fields.extend(kept);
                }
                self.ws = Workspace::load(self.ctx, &self.draft.ops)?;
                Ok(format!("Added {labels} to the drafted note \"{title}\"."))
            }
            Some(_) => Ok("Nothing new to add.".to_string()),
            None if kept.is_empty()
                && args
                    .get("fields")
                    .is_some_and(|f| f.as_array().is_some_and(|a| !a.is_empty())) =>
            {
                Ok("Nothing new to add; no note drafted.".to_string())
            }
            None => {
                let labels = field_labels(&kept);
                self.push(Op::CreateNote {
                    project,
                    folder,
                    title: title.clone(),
                    tags: items::normalize_tags(&tags),
                    fields: kept,
                });
                Ok(format!(
                    "Drafted note \"{title}\" in {place} with {labels}."
                ))
            }
        }
    }

    /// Import: many notes at once.
    fn add_notes(&mut self, args: &Value) -> AppResult<String> {
        let notes = match args.get("notes") {
            Some(Value::Array(notes)) => notes.clone(),
            Some(Value::String(s)) => {
                serde_json::from_str(s).map_err(|_| bad("notes must be a list"))?
            }
            _ => return Err(bad("notes must be a list of notes")),
        };
        let mut lines = Vec::new();
        for (i, note) in notes.iter().enumerate() {
            let before = self.draft.ops.len();
            match self.create_note(note) {
                Ok(text) => lines.push(format!("{}. {text}", i + 1)),
                Err(AppError::Invalid(e)) => {
                    self.draft.ops.truncate(before);
                    self.ws = Workspace::load(self.ctx, &self.draft.ops)?;
                    lines.push(format!("{}. Error: {e}", i + 1));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(lines.join("\n"))
    }

    /// Adds fields to an existing note as `AddField` ops; returns their labels.
    fn add_to_existing(&mut self, note: i64, list: Vec<Field>) -> AppResult<String> {
        let labels = field_labels(&list);
        for field in list {
            self.push(Op::AddField { note, field });
        }
        Ok(labels)
    }

    fn add_field(&mut self, args: &Value) -> AppResult<String> {
        let note = self.ws.existing_note(args, "note")?;
        let field = self.field(args)?;
        let existing = self.ws.note(Ref::Id(note)).expect("checked");
        if existing.secure {
            return Err(bad(format!(
                "note {note} is fully secret; its fields can't be changed here"
            )));
        }
        let title = existing.title.clone();
        if fields::find(&existing.text, &field.label).is_ok_and(|i| {
            fields::normalize(
                fields::label_of(existing.text.lines().nth(i).unwrap_or("")).unwrap_or(""),
            ) == fields::normalize(&field.label)
        }) {
            return Err(bad(format!(
                "note {note} has a {} field already; use update_field",
                field.label
            )));
        }
        if let Some(why) = self.ws.stored_in(self.ctx, &self.draft.ops, &field.value) {
            return Ok(format!("Skipped: {why}."));
        }
        let label = field.label.clone();
        self.push(Op::AddField { note, field });
        Ok(format!("Drafted adding {label} to note {note} ({title})."))
    }

    fn update_field(&mut self, args: &Value) -> AppResult<String> {
        let note = self.ws.existing_note(args, "note")?;
        let label = name_arg(args, "label")?;
        let value = self.value(args.get("value"))?;
        let existing = self.ws.note(Ref::Id(note)).expect("checked");
        if existing.secure {
            return Err(bad(format!(
                "note {note} is fully secret; its fields can't be changed here"
            )));
        }
        let line = fields::find(&existing.text, &label)
            .map_err(|e| bad(format!("note {note} has no field \"{label}\"; {e}")))?;
        let current = existing.text.lines().nth(line).unwrap_or_default();
        let found = fields::label_of(current).unwrap_or(&label).to_string();
        let title = existing.title.clone();
        let secret = match &value {
            Val::Secret(_) => Some(true),
            _ => args.get("secret").and_then(Value::as_bool),
        };
        if let Val::Input(n) = value
            && self.draft.ops.iter().any(|op| op_uses_input(op, n))
        {
            return Err(bad(format!(
                "[u{n}] is used in this draft already; each value goes to one place"
            )));
        }
        if let Some(text) = resolve_known(self.ctx.inputs, &value)
            && fields::value_of(current) == Some(text.as_str())
        {
            return Ok(format!("{found} of note {note} has that value already."));
        }
        self.push(Op::UpdateField {
            note,
            label: found.clone(),
            value,
            secret,
        });
        Ok(format!(
            "Drafted new value for {found} in note {note} ({title})."
        ))
    }

    fn remove_field(&mut self, args: &Value) -> AppResult<String> {
        let note = self.ws.existing_note(args, "note")?;
        let label = name_arg(args, "label")?;
        let existing = self.ws.note(Ref::Id(note)).expect("checked");
        if existing.secure {
            return Err(bad(format!(
                "note {note} is fully secret; its fields can't be changed here"
            )));
        }
        let line = fields::find(&existing.text, &label)
            .map_err(|e| bad(format!("note {note} has no field \"{label}\"; {e}")))?;
        let found = fields::label_of(existing.text.lines().nth(line).unwrap_or_default())
            .unwrap_or(&label)
            .to_string();
        let title = existing.title.clone();
        self.push(Op::RemoveField {
            note,
            label: found.clone(),
        });
        Ok(format!(
            "Drafted removing {found} from note {note} ({title})."
        ))
    }

    fn rename(&mut self, args: &Value) -> AppResult<String> {
        let kind = str_arg(args, "kind")?.trim().to_lowercase();
        let name = name_arg(args, "name")?;
        match kind.as_str() {
            "note" => {
                let note = self.ws.existing_note(args, "id")?;
                self.push(Op::RenameNote {
                    note,
                    title: name.clone(),
                });
                Ok(format!("Drafted renaming note {note} to \"{name}\"."))
            }
            "folder" => {
                let folder = int_arg(args, "id")?;
                if !self.ws.folders.iter().any(|f| f.id == Ref::Id(folder)) {
                    return Err(bad(format!("there is no folder {folder}")));
                }
                self.push(Op::RenameFolder {
                    folder,
                    name: name.clone(),
                });
                Ok(format!("Drafted renaming folder {folder} to \"{name}\"."))
            }
            "project" => {
                let project = int_arg(args, "id")?;
                if !self.ws.projects.iter().any(|p| p.id == Ref::Id(project)) {
                    return Err(bad(format!("there is no project {project}")));
                }
                self.push(Op::RenameProject {
                    project,
                    name: name.clone(),
                });
                Ok(format!("Drafted renaming project {project} to \"{name}\"."))
            }
            _ => Err(bad("kind must be note, folder or project")),
        }
    }

    fn move_note(&mut self, args: &Value) -> AppResult<String> {
        let note = self.ws.existing_note(args, "note")?;
        let current = self.ws.note(Ref::Id(note)).expect("checked");
        let (from_project, from_folder) = (current.project, current.folder);
        let title = current.title.clone();
        let project = match opt_str(args, "project") {
            Some(text) => self.ws.existing_project(text)?,
            None => from_project,
        };
        let folder = self.folder_or_new(project, opt_str(args, "folder"))?;
        if project == from_project && folder == from_folder {
            return Ok(format!(
                "Note {note} is in {} already.",
                self.ws.place(project, folder)
            ));
        }
        let place = self.ws.place(project, folder);
        self.push(Op::MoveNote {
            note,
            project,
            folder,
        });
        Ok(format!("Drafted moving note {note} ({title}) to {place}."))
    }

    fn delete_note(&mut self, args: &Value) -> AppResult<String> {
        let note = self.ws.existing_note(args, "note")?;
        let title = self.ws.note(Ref::Id(note)).expect("checked").title.clone();
        self.push(Op::DeleteNote { note });
        Ok(format!("Drafted deleting note {note} ({title})."))
    }

    /// A todo's or payment's project: `None` for general or personal ones.
    fn optional_project(&self, args: &Value) -> AppResult<Option<Ref>> {
        match opt_str(args, "project") {
            None => Ok(None),
            Some(text)
                if matches!(
                    text.to_lowercase().as_str(),
                    "personal" | "general" | "none" | "kişisel"
                ) && self.ws.find_project(text)?.is_none() =>
            {
                Ok(None)
            }
            Some(text) => self.ws.existing_project(text).map(Some),
        }
    }

    fn todo(&self, args: &Value) -> AppResult<&Todo> {
        let id = int_arg(args, "id")?;
        self.ws
            .todos
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| bad(format!("there is no todo {id}")))
    }

    fn create_todo(&mut self, args: &Value) -> AppResult<String> {
        let title = name_arg(args, "title")?;
        let project = self.optional_project(args)?;
        let due = opt_date(args, "due")?;
        let place = project.map_or("General".to_string(), |p| {
            self.ws.project_name(p).to_string()
        });
        self.push(Op::CreateTodo {
            project,
            title: title.clone(),
            due,
        });
        Ok(format!("Drafted todo \"{title}\" ({place})."))
    }

    fn update_todo(&mut self, args: &Value) -> AppResult<String> {
        let todo = self.todo(args)?.id;
        let title = match opt_str(args, "title") {
            Some(_) => Some(name_arg(args, "title")?),
            None => None,
        };
        let due = opt_date(args, "due")?;
        if title.is_none() && due.is_none() {
            return Err(bad("give a new title or due date"));
        }
        self.push(Op::UpdateTodo { todo, title, due });
        Ok(format!("Drafted changing todo {todo}."))
    }

    fn complete_todo(&mut self, args: &Value) -> AppResult<String> {
        let todo = self.todo(args)?;
        if todo.done {
            return Ok(format!("Todo {} is done already.", todo.id));
        }
        let (id, title) = (todo.id, todo.title.clone());
        self.push(Op::CompleteTodo { todo: id });
        Ok(format!("Drafted marking todo {id} ({title}) done."))
    }

    fn payment(&self, args: &Value) -> AppResult<&Payment> {
        let id = int_arg(args, "id")?;
        self.ws
            .payments
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| bad(format!("there is no payment {id}")))
    }

    fn create_payment(&mut self, args: &Value) -> AppResult<String> {
        let name = name_arg(args, "name")?;
        let amount = amount_arg(args, "amount")?.ok_or_else(|| bad("amount is required"))?;
        let currency = currency_arg(args)?;
        let cycle = str_arg(args, "cycle")?.trim().to_lowercase();
        let cycle = match cycle.as_str() {
            "monthly" | "month" | "aylık" => "monthly",
            "yearly" | "annual" | "year" | "yıllık" => "yearly",
            "once" | "one-time" | "one time" => "once",
            _ => return Err(bad("cycle must be monthly, yearly or once")),
        };
        let start = opt_date(args, "start")?.unwrap_or_else(|| self.ctx.today.to_string());
        let project = self.optional_project(args)?;
        let place = project.map_or("Personal".to_string(), |p| {
            self.ws.project_name(p).to_string()
        });
        self.push(Op::CreatePayment {
            project,
            name: name.clone(),
            amount,
            currency: currency.clone(),
            cycle: cycle.to_string(),
            start: start.clone(),
        });
        Ok(format!(
            "Drafted payment \"{name}\" ({place}): {} {cycle} from {start}.",
            spend::money(amount, &currency)
        ))
    }

    fn update_payment(&mut self, args: &Value) -> AppResult<String> {
        let payment = self.payment(args)?;
        let id = payment.id;
        let start = payment.start_date.clone();
        let name = match opt_str(args, "name") {
            Some(_) => Some(name_arg(args, "name")?),
            None => None,
        };
        let amount = amount_arg(args, "amount")?;
        if name.is_none() && amount.is_none() {
            return Err(bad("give a new amount or name"));
        }
        let from = opt_date(args, "from")?
            .unwrap_or_else(|| self.ctx.today.to_string())
            .max(start);
        self.push(Op::UpdatePayment {
            payment: id,
            name,
            amount,
            from,
        });
        Ok(format!("Drafted changing payment {id}."))
    }

    fn cancel_payment(&mut self, args: &Value) -> AppResult<String> {
        let payment = self.payment(args)?;
        let (id, start) = (payment.id, payment.start_date.clone());
        let date = opt_date(args, "date")?.unwrap_or_else(|| self.ctx.today.to_string());
        if date <= start {
            return Err(bad(format!(
                "payment {id} starts on {start}; the cancel date must be after it"
            )));
        }
        self.push(Op::CancelPayment {
            payment: id,
            date: date.clone(),
        });
        Ok(format!("Drafted cancelling payment {id} from {date}."))
    }
}

fn field_labels(list: &[Field]) -> String {
    if list.is_empty() {
        return "no fields".to_string();
    }
    list.iter()
        .map(|f| {
            let kind = if f.secret { "secret" } else { "plain" };
            format!("{} ({kind})", f.label)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// --- the diff the user reviews --------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Project,
    Folder,
    Note,
    Todo,
    Payment,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Add,
    Edit,
    Rename,
    Move,
    Delete,
    Done,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sign {
    Add,
    Change,
    Remove,
}

/// A value as the diff shows it: the user's own value (masked when secret), or a stored
/// secret's handle the user can reveal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Shown {
    Text { text: String, secret: bool },
    Handle { handle: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub sign: Sign,
    pub label: String,
    #[serde(default)]
    pub value: Option<Shown>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub kind: Kind,
    pub action: Action,
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub lines: Vec<Line>,
}

/// The changes to one project (or to personal and general todos and payments).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub project: String,
    /// The draft creates the project.
    pub new: bool,
    pub entries: Vec<Entry>,
}

fn shown(inputs: &Inputs, value: &Val, secret: bool) -> Shown {
    match value {
        Val::Secret(handle) => Shown::Handle {
            handle: handle.clone(),
        },
        other => Shown::Text {
            text: resolve_known(inputs, other).unwrap_or_default(),
            secret,
        },
    }
}

/// The draft as the user reviews it, grouped by project in the order it touches them.
pub fn view(ctx: &Ctx, ops: &[Op]) -> AppResult<Vec<Group>> {
    let before = Workspace::load(ctx, &[])?;
    let after = Workspace::load(ctx, ops)?;
    let mut groups: Vec<Group> = Vec::new();
    // Entries for field changes to one existing note gather in one place.
    let mut note_entries: HashMap<i64, (usize, usize)> = HashMap::new();
    let mut new_projects: HashSet<Ref> = HashSet::new();

    let group_index = |groups: &mut Vec<Group>, name: &str, new: bool| -> usize {
        match groups.iter().position(|g| g.project == name) {
            Some(i) => {
                groups[i].new |= new;
                i
            }
            None => {
                groups.push(Group {
                    project: name.to_string(),
                    new,
                    entries: Vec::new(),
                });
                groups.len() - 1
            }
        }
    };

    for (i, op) in ops.iter().enumerate() {
        let entry = |kind, action, title: &str, detail: Option<String>, lines| Entry {
            kind,
            action,
            title: title.to_string(),
            detail,
            lines,
        };
        let project_of_note = |id: i64| {
            before
                .notes
                .iter()
                .find(|n| n.id == Ref::Id(id))
                .map(|n| n.project)
        };
        let (project, item) = match op {
            Op::CreateProject { .. } => {
                new_projects.insert(Ref::New(i));
                let name = after.project_name(Ref::New(i)).to_string();
                group_index(&mut groups, &name, true);
                continue;
            }
            Op::CreateFolder {
                project, parent, ..
            } => (
                Some(*project),
                entry(
                    Kind::Folder,
                    Action::Add,
                    &after.folder_path(Some(Ref::New(i))),
                    parent.map(|p| format!("in {}", after.folder_path(Some(p)))),
                    Vec::new(),
                ),
            ),
            Op::CreateNote {
                project,
                folder,
                title,
                tags,
                fields: list,
            } => {
                let mut detail = after.folder_path(*folder);
                if !tags.is_empty() {
                    let tags = tags.join(", ");
                    detail = if detail.is_empty() {
                        tags
                    } else {
                        format!("{detail} · {tags}")
                    };
                }
                let lines = list
                    .iter()
                    .map(|f| Line {
                        sign: Sign::Add,
                        label: f.label.clone(),
                        value: Some(shown(ctx.inputs, &f.value, f.secret)),
                    })
                    .collect();
                (
                    Some(*project),
                    entry(
                        Kind::Note,
                        Action::Add,
                        title,
                        (!detail.is_empty()).then_some(detail),
                        lines,
                    ),
                )
            }
            Op::AddField { note, field } => {
                let line = Line {
                    sign: Sign::Add,
                    label: field.label.clone(),
                    value: Some(shown(ctx.inputs, &field.value, field.secret)),
                };
                if let Some(&(g, e)) = note_entries.get(note) {
                    groups[g].entries[e].lines.push(line);
                    continue;
                }
                (project_of_note(*note), note_edit(&before, *note, line))
            }
            Op::UpdateField {
                note,
                label,
                value,
                secret,
            } => {
                let was_secret = before
                    .note(Ref::Id(*note))
                    .and_then(|n| {
                        fields::find(&n.text, label)
                            .ok()
                            .and_then(|l| n.text.lines().nth(l))
                    })
                    .is_some_and(fields::is_secret);
                let line = Line {
                    sign: Sign::Change,
                    label: label.clone(),
                    value: Some(shown(ctx.inputs, value, secret.unwrap_or(was_secret))),
                };
                if let Some(&(g, e)) = note_entries.get(note) {
                    groups[g].entries[e].lines.push(line);
                    continue;
                }
                (project_of_note(*note), note_edit(&before, *note, line))
            }
            Op::RemoveField { note, label } => {
                let line = Line {
                    sign: Sign::Remove,
                    label: label.clone(),
                    value: None,
                };
                if let Some(&(g, e)) = note_entries.get(note) {
                    groups[g].entries[e].lines.push(line);
                    continue;
                }
                (project_of_note(*note), note_edit(&before, *note, line))
            }
            Op::RenameNote { note, title } => {
                let old = before.note(Ref::Id(*note)).map_or("", |n| n.title.as_str());
                (
                    project_of_note(*note),
                    entry(
                        Kind::Note,
                        Action::Rename,
                        title,
                        Some(old.to_string()),
                        Vec::new(),
                    ),
                )
            }
            Op::RenameFolder { folder, name } => {
                let f = before.folders.iter().find(|f| f.id == Ref::Id(*folder));
                (
                    f.map(|f| f.project),
                    entry(
                        Kind::Folder,
                        Action::Rename,
                        name,
                        f.map(|f| f.name.clone()),
                        Vec::new(),
                    ),
                )
            }
            Op::RenameProject { project, name } => (
                Some(Ref::Id(*project)),
                entry(
                    Kind::Project,
                    Action::Rename,
                    name,
                    Some(before.project_name(Ref::Id(*project)).to_string()),
                    Vec::new(),
                ),
            ),
            Op::MoveNote {
                note,
                project,
                folder,
            } => {
                let old = before.note(Ref::Id(*note));
                let from = old
                    .map(|n| before.place(n.project, n.folder))
                    .unwrap_or_default();
                let to = after.place(*project, *folder);
                (
                    old.map(|n| n.project),
                    entry(
                        Kind::Note,
                        Action::Move,
                        old.map_or("", |n| n.title.as_str()),
                        Some(format!("{from} → {to}")),
                        Vec::new(),
                    ),
                )
            }
            Op::DeleteNote { note } => {
                let old = before.note(Ref::Id(*note));
                (
                    old.map(|n| n.project),
                    entry(
                        Kind::Note,
                        Action::Delete,
                        old.map_or("", |n| n.title.as_str()),
                        old.map(|n| before.folder_path(n.folder))
                            .filter(|p| !p.is_empty()),
                        Vec::new(),
                    ),
                )
            }
            Op::CreateTodo {
                project,
                title,
                due,
            } => (
                *project,
                entry(
                    Kind::Todo,
                    Action::Add,
                    title,
                    due.as_ref().map(|d| format!("due {d}")),
                    Vec::new(),
                ),
            ),
            Op::UpdateTodo { todo, title, due } => {
                let old = before.todos.iter().find(|t| t.id == *todo);
                let mut lines = Vec::new();
                if let Some(title) = title {
                    lines.push(text_line("Title", title));
                }
                if let Some(due) = due {
                    lines.push(text_line("Due", due));
                }
                (
                    old.and_then(|t| t.project_id).map(Ref::Id),
                    entry(
                        Kind::Todo,
                        Action::Edit,
                        old.map_or("", |t| t.title.as_str()),
                        None,
                        lines,
                    ),
                )
            }
            Op::CompleteTodo { todo } => {
                let old = before.todos.iter().find(|t| t.id == *todo);
                (
                    old.and_then(|t| t.project_id).map(Ref::Id),
                    entry(
                        Kind::Todo,
                        Action::Done,
                        old.map_or("", |t| t.title.as_str()),
                        None,
                        Vec::new(),
                    ),
                )
            }
            Op::CreatePayment {
                project,
                name,
                amount,
                currency,
                cycle,
                start,
            } => (
                *project,
                entry(
                    Kind::Payment,
                    Action::Add,
                    name,
                    Some(format!(
                        "{} {cycle} from {start}",
                        spend::money(*amount, currency)
                    )),
                    Vec::new(),
                ),
            ),
            Op::UpdatePayment {
                payment,
                name,
                amount,
                from,
            } => {
                let old = before.payments.iter().find(|p| p.id == *payment);
                let mut lines = Vec::new();
                if let Some(name) = name {
                    lines.push(text_line("Name", name));
                }
                if let (Some(amount), Some(p)) = (amount, old) {
                    lines.push(text_line(
                        "Amount",
                        &format!("{} from {from}", spend::money(*amount, &p.currency)),
                    ));
                }
                (
                    old.and_then(|p| p.project_id).map(Ref::Id),
                    entry(
                        Kind::Payment,
                        Action::Edit,
                        old.map_or("", |p| p.name.as_str()),
                        None,
                        lines,
                    ),
                )
            }
            Op::CancelPayment { payment, date } => {
                let old = before.payments.iter().find(|p| p.id == *payment);
                (
                    old.and_then(|p| p.project_id).map(Ref::Id),
                    entry(
                        Kind::Payment,
                        Action::Cancel,
                        old.map_or("", |p| p.name.as_str()),
                        Some(format!("from {date}")),
                        Vec::new(),
                    ),
                )
            }
        };
        let (name, new) = match project {
            Some(id) => (
                after.project_name(id).to_string(),
                new_projects.contains(&id),
            ),
            None => ("Personal".to_string(), false),
        };
        let g = group_index(&mut groups, &name, new);
        groups[g].entries.push(item);
        let e = groups[g].entries.len() - 1;
        if let Op::AddField { note, .. }
        | Op::UpdateField { note, .. }
        | Op::RemoveField { note, .. } = op
        {
            note_entries.insert(*note, (g, e));
        }
    }
    Ok(groups)
}

fn note_edit(before: &Workspace, note: i64, line: Line) -> Entry {
    let old = before.note(Ref::Id(note));
    Entry {
        kind: Kind::Note,
        action: Action::Edit,
        title: old.map_or(String::new(), |n| n.title.clone()),
        detail: old
            .map(|n| before.folder_path(n.folder))
            .filter(|p| !p.is_empty()),
        lines: vec![line],
    }
}

fn text_line(label: &str, text: &str) -> Line {
    Line {
        sign: Sign::Change,
        label: label.to_string(),
        value: Some(Shown::Text {
            text: text.to_string(),
            secret: false,
        }),
    }
}

/// One line per change, for the model's later turns ("Drafted: …").
pub fn summary(groups: &[Group]) -> String {
    let mut parts = Vec::new();
    for g in groups {
        for e in &g.entries {
            let action = match e.action {
                Action::Add => "add",
                Action::Edit => "edit",
                Action::Rename => "rename",
                Action::Move => "move",
                Action::Delete => "delete",
                Action::Done => "complete",
                Action::Cancel => "cancel",
            };
            parts.push(format!(
                "{action} {} \"{}\" ({})",
                kind_name(&e.kind),
                e.title,
                g.project
            ));
        }
    }
    parts.join("; ")
}

fn kind_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::Project => "project",
        Kind::Folder => "folder",
        Kind::Note => "note",
        Kind::Todo => "todo",
        Kind::Payment => "payment",
    }
}

// --- applying -----------------------------------------------------------------------

fn resolve(
    conn: &Connection,
    key: &VaultKey,
    inputs: &Inputs,
    value: &Val,
) -> AppResult<Zeroizing<String>> {
    match value {
        Val::Input(n) => inputs
            .get(*n)
            .map(|v| Zeroizing::new(v.to_string()))
            .ok_or_else(|| bad(format!("[u{n}] is missing"))),
        Val::Text(text) => Ok(Zeroizing::new(text.clone())),
        Val::Secret(handle) => handles::reveal(conn, key, Handle::parse(handle)?),
    }
}

/// A note's whole text, decrypted, and whether it is a secure note.
fn note_plaintext(
    conn: &Connection,
    key: &VaultKey,
    id: i64,
) -> AppResult<(Zeroizing<String>, bool)> {
    let item = items::get(conn, id)?;
    let s = &item.summary;
    if s.secure || s.has_secrets {
        let encrypted = items::encrypted(conn, id)?;
        let text = open_with(key, encrypted.template.as_deref(), &encrypted.sealed)?;
        return Ok((text, s.secure));
    }
    Ok((Zeroizing::new(item.body.unwrap_or_default()), false))
}

fn seal_body(key: &VaultKey, text: &str, secure: bool) -> AppResult<Body> {
    if secure {
        return Ok(Body::Secure(seal_with(key, text)?));
    }
    Ok(match seal_inline_with(key, text)? {
        Some((template, secrets)) => Body::Inline { template, secrets },
        None => Body::Plain(text.to_string()),
    })
}

/// Rewrites an existing note's body through `change`, keeping everything else.
fn edit_note(
    conn: &Connection,
    key: &VaultKey,
    id: i64,
    now: i64,
    change: impl FnOnce(&str) -> AppResult<String>,
) -> AppResult<()> {
    let (text, secure) = note_plaintext(conn, key, id)?;
    let new = Zeroizing::new(change(&text)?);
    let item = items::get(conn, id)?;
    let s = item.summary;
    let body = seal_body(key, &new, secure)?;
    items::update(
        conn,
        id,
        &ItemRecord {
            project_id: s.project_id,
            folder_id: s.folder_id,
            title: &s.title,
            body: &body,
            tags: &s.tags,
        },
        now,
    )
}

/// Applies a draft in one transaction: all of it, or nothing.
pub fn apply(
    conn: &Connection,
    key: &VaultKey,
    inputs: &Inputs,
    ops: &[Op],
    now: i64,
) -> AppResult<()> {
    let change = Change::begin(conn)?;
    let mut created: Vec<Option<i64>> = vec![None; ops.len()];
    let id_of = |created: &[Option<i64>], r: Ref| -> AppResult<i64> {
        match r {
            Ref::Id(id) => Ok(id),
            Ref::New(i) => created
                .get(i)
                .copied()
                .flatten()
                .ok_or_else(|| bad("The draft refers to something it doesn't create")),
        }
    };
    let opt_id = |created: &[Option<i64>], r: Option<Ref>| r.map(|r| id_of(created, r)).transpose();
    let field_error = |e: String| bad(format!("The note changed since the draft: {e}"));

    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::CreateProject { name } => {
                created[i] = Some(projects::insert(&change, name, "", now)?);
            }
            Op::CreateFolder {
                project,
                parent,
                name,
            } => {
                let project = id_of(&created, *project)?;
                let parent = opt_id(&created, *parent)?;
                created[i] = Some(folders::insert(&change, project, parent, name, now)?);
            }
            Op::CreateNote {
                project,
                folder,
                title,
                tags,
                fields: list,
            } => {
                let mut text = Zeroizing::new(String::new());
                for field in list {
                    let value = resolve(&change, key, inputs, &field.value)?;
                    *text = fields::add(&text, &field.label, &value, field.secret);
                }
                let body = seal_body(key, &text, false)?;
                let record = ItemRecord {
                    project_id: id_of(&created, *project)?,
                    folder_id: opt_id(&created, *folder)?,
                    title,
                    body: &body,
                    tags,
                };
                created[i] = Some(items::insert(&change, &record, now)?);
            }
            Op::AddField { note, field } => {
                let value = resolve(&change, key, inputs, &field.value)?;
                edit_note(&change, key, *note, now, |text| {
                    Ok(fields::add(text, &field.label, &value, field.secret))
                })?;
            }
            Op::UpdateField {
                note,
                label,
                value,
                secret,
            } => {
                let value = resolve(&change, key, inputs, value)?;
                edit_note(&change, key, *note, now, |text| {
                    fields::update(text, label, &value, *secret).map_err(field_error)
                })?;
            }
            Op::RemoveField { note, label } => {
                edit_note(&change, key, *note, now, |text| {
                    fields::remove(text, label).map_err(field_error)
                })?;
            }
            Op::RenameNote { note, title } => items::rename(&change, *note, title, now)?,
            Op::RenameFolder { folder, name } => folders::rename(&change, *folder, name)?,
            Op::RenameProject { project, name } => {
                projects::rename(&change, *project, name, now)?;
            }
            Op::MoveNote {
                note,
                project,
                folder,
            } => {
                let project = id_of(&created, *project)?;
                let folder = opt_id(&created, *folder)?;
                if let Some(folder) = folder
                    && folders::project_of(&change, folder)? != project
                {
                    return Err(bad("Folder belongs to another project"));
                }
                items::move_to(&change, *note, project, folder, now)?;
            }
            Op::DeleteNote { note } => items::delete(&change, *note)?,
            Op::CreateTodo {
                project,
                title,
                due,
            } => {
                let record = TodoRecord {
                    project_id: opt_id(&created, *project)?,
                    item_id: None,
                    title,
                    description: "",
                    secrets: None,
                    due_date: due.as_deref(),
                };
                todos::insert(&change, &record, now)?;
            }
            Op::UpdateTodo { todo, title, due } => {
                let old = todos::get(&change, *todo)?;
                let sealed = if old.has_secrets {
                    Some(todos::description_secrets(&change, *todo)?.1)
                } else {
                    None
                };
                let record = TodoRecord {
                    project_id: old.project_id,
                    item_id: old.item_id,
                    title: title.as_deref().unwrap_or(&old.title),
                    description: &old.description,
                    secrets: sealed.as_ref(),
                    due_date: due.as_deref().or(old.due_date.as_deref()),
                };
                todos::update(&change, *todo, &record, now)?;
            }
            Op::CompleteTodo { todo } => todos::set_done(&change, *todo, true, now)?,
            Op::CreatePayment {
                project,
                name,
                amount,
                currency,
                cycle,
                start,
            } => {
                let record = PaymentRecord {
                    project_id: opt_id(&created, *project)?,
                    name,
                    currency,
                    cycle,
                    start_date: start,
                    cancel_date: None,
                };
                payments::insert(&change, &record, *amount, now)?;
            }
            Op::UpdatePayment {
                payment,
                name,
                amount,
                from,
            } => {
                let old = payments::get(&change, *payment)?;
                let record = PaymentRecord {
                    project_id: old.project_id,
                    name: name.as_deref().unwrap_or(&old.name),
                    currency: &old.currency,
                    cycle: &old.cycle,
                    start_date: &old.start_date,
                    cancel_date: old.cancel_date.as_deref(),
                };
                let price = amount.map(|a| (from.as_str(), a));
                payments::update(&change, *payment, &record, price, now)?;
            }
            Op::CancelPayment { payment, date } => {
                let old = payments::get(&change, *payment)?;
                let record = PaymentRecord {
                    project_id: old.project_id,
                    name: &old.name,
                    currency: &old.currency,
                    cycle: &old.cycle,
                    start_date: &old.start_date,
                    cancel_date: Some(date),
                };
                payments::update(&change, *payment, &record, None, now)?;
            }
        }
    }
    change.commit()?;
    Ok(())
}

/// Every secret value in the vault's inline notes, for checking a test never leaks one.
#[cfg(test)]
pub fn all_spans(conn: &Connection, key: &VaultKey) -> Vec<String> {
    let mut all = Vec::new();
    for item in items::list(conn).unwrap() {
        if item.has_secrets {
            let sealed = items::encrypted(conn, item.id).unwrap().sealed;
            let spans = crate::commands::sealing::open_spans(key, &sealed).unwrap();
            all.extend(spans.iter().cloned());
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn key() -> VaultKey {
        Zeroizing::new(vec![7; 32])
    }

    /// Acme Shop (1) with Infra (1) holding "Hosting panel" (1); Blog (2) with "Admin" (2);
    /// a todo (1) and a payment (1).
    fn vault() -> Connection {
        let conn = open_in_memory();
        let key = key();
        let acme = projects::insert(&conn, "Acme Shop", "", 0).unwrap();
        let blog = projects::insert(&conn, "Blog", "", 0).unwrap();
        let infra = folders::insert(&conn, acme, None, "Infra", 0).unwrap();
        let body = seal_body(
            &key,
            "URL: `https://panel.example`\nUser: `apo`\nPassword: `!hunter2-old!`",
            false,
        )
        .unwrap();
        items::insert(
            &conn,
            &ItemRecord {
                project_id: acme,
                folder_id: Some(infra),
                title: "Hosting panel",
                body: &body,
                tags: &[],
            },
            0,
        )
        .unwrap();
        items::insert(
            &conn,
            &ItemRecord {
                project_id: blog,
                folder_id: None,
                title: "Admin",
                body: &Body::Plain("Email: `me@blog.example`".to_string()),
                tags: &[],
            },
            0,
        )
        .unwrap();
        todos::insert(
            &conn,
            &TodoRecord {
                project_id: Some(acme),
                item_id: None,
                title: "Renew SSL",
                description: "",
                secrets: None,
                due_date: None,
            },
            0,
        )
        .unwrap();
        payments::insert(
            &conn,
            &PaymentRecord {
                project_id: Some(acme),
                name: "Hetzner",
                currency: "EUR",
                cycle: "monthly",
                start_date: "2026-01-05",
                cancel_date: None,
            },
            1249,
            0,
        )
        .unwrap();
        conn
    }

    struct Run {
        conn: Connection,
        inputs: Inputs,
        stored: HashMap<String, i64>,
        draft: Draft,
    }

    impl Run {
        fn new(message: &str) -> Self {
            let conn = vault();
            let mut inputs = Inputs::default();
            inputs.take(message);
            let stored = stored_values(&conn, Some(&key())).unwrap();
            Self {
                conn,
                inputs,
                stored,
                draft: Draft::default(),
            }
        }

        fn call(&mut self, name: &str, args: Value) -> String {
            let ctx = Ctx {
                conn: &self.conn,
                today: "2026-10-23",
                inputs: &self.inputs,
                stored: &self.stored,
            };
            run(&ctx, &mut self.draft, name, &args).unwrap()
        }

        fn view(&self) -> Vec<Group> {
            let ctx = Ctx {
                conn: &self.conn,
                today: "2026-10-23",
                inputs: &self.inputs,
                stored: &self.stored,
            };
            view(&ctx, &self.draft.ops).unwrap()
        }

        fn apply(&self) {
            apply(&self.conn, &key(), &self.inputs, &self.draft.ops, 1).unwrap();
        }

        fn body(&self, id: i64) -> String {
            note_plaintext(&self.conn, &key(), id)
                .unwrap()
                .0
                .to_string()
        }
    }

    #[test]
    fn a_new_key_lands_in_a_new_note_with_its_real_value() {
        let mut run = Run::new("add a Stripe test key sk_test_51HxYz9KqLmN0pQrStUv to acme");
        let result = run.call(
            "create_note",
            json!({
                "project": "acme",
                "folder": "Payments",
                "title": "Stripe",
                "fields": [{ "label": "Secret key", "value": "[u1]", "secret": true }],
            }),
        );
        assert!(
            result.contains("Drafted note \"Stripe\" in Acme Shop / Payments"),
            "{result}"
        );
        assert!(result.contains("Drafted new folder Acme Shop / Payments."));
        let groups = run.view();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].project, "Acme Shop");
        assert_eq!(groups[0].entries.len(), 2);
        assert_eq!(
            groups[0].entries[1].lines[0].value,
            Some(Shown::Text {
                text: "sk_test_51HxYz9KqLmN0pQrStUv".to_string(),
                secret: true
            })
        );
        run.apply();
        let note = items::list(&run.conn)
            .unwrap()
            .into_iter()
            .find(|i| i.title == "Stripe")
            .unwrap();
        assert!(note.has_secrets);
        assert_eq!(
            run.body(note.id),
            "Secret key: `!sk_test_51HxYz9KqLmN0pQrStUv!`"
        );
        assert!(all_spans(&run.conn, &key()).contains(&"sk_test_51HxYz9KqLmN0pQrStUv".to_string()));
    }

    #[test]
    fn typed_values_are_refused() {
        let mut run = Run::new("password is Kedi-Mavi-42 for the blog admin");
        let result = run.call(
            "add_field",
            json!({ "note": 2, "label": "Password", "value": "Kedi-Mavi-24", "secret": true }),
        );
        assert!(
            result.starts_with("Error: value must be a handle"),
            "{result}"
        );
        let result = run.call(
            "add_field",
            json!({ "note": 2, "label": "Password", "value": "u1", "secret": true }),
        );
        assert!(
            result.starts_with("Drafted adding Password to note 2"),
            "{result}"
        );
        assert!(run.draft.ops.len() == 1);
    }

    #[test]
    fn updates_keep_secrecy_and_apply_to_the_real_body() {
        let mut run = Run::new("new panel password: Yildiz2026!?");
        let result = run.call(
            "update_field",
            json!({ "note": "1", "label": "password", "value": "[u1: 13 chars]" }),
        );
        assert!(result.contains("Password in note 1"), "{result}");
        let missing = run.call(
            "update_field",
            json!({ "note": 1, "label": "token", "value": "u1" }),
        );
        assert!(
            missing.contains("its fields are: URL, User, Password"),
            "{missing}"
        );
        run.call("remove_field", json!({ "note": 1, "label": "User" }));
        let groups = run.view();
        assert_eq!(groups[0].entries.len(), 1);
        assert_eq!(groups[0].entries[0].lines.len(), 2);
        run.apply();
        assert_eq!(
            run.body(1),
            "URL: `https://panel.example`\nPassword: `!Yildiz2026!?!`"
        );
    }

    #[test]
    fn stored_values_are_skipped() {
        let mut run = Run::new("admin of blog: me@blog.example pass hunter2-old");
        let result = run.call(
            "create_note",
            json!({
                "project": "Blog",
                "title": "Admin",
                "fields": [
                    { "label": "Email", "value": "u1", "secret": false },
                    { "label": "Password", "value": "u2", "secret": true },
                ],
            }),
        );
        assert!(
            result.contains("Skipped Email: already stored in note 2"),
            "{result}"
        );
        assert!(
            result.contains("Skipped Password: already stored in note 1"),
            "{result}"
        );
        assert!(run.draft.ops.is_empty());
    }

    #[test]
    fn same_titles_merge_into_the_existing_note() {
        let mut run = Run::new("blog admin api token tok_9a8b7c6d5e");
        let result = run.call(
            "create_note",
            json!({
                "project": "blog projesi",
                "title": "admin",
                "fields": [{ "label": "API token", "value": "u1", "secret": true }],
            }),
        );
        assert!(
            result.contains("drafted adding API token (secret) to it"),
            "{result}"
        );
        assert!(matches!(run.draft.ops[..], [Op::AddField { note: 2, .. }]));
    }

    #[test]
    fn moves_todos_and_payments() {
        let mut run =
            Run::new("move the hosting panel to Billing, renew ssl done, 300 TRY hosting");
        run.call("move_note", json!({ "note": 1, "folder": "Billing" }));
        run.call("complete_todo", json!({ "id": 1 }));
        let result = run.call(
            "create_payment",
            json!({ "name": "Hosting", "amount": 300, "currency": "TL", "cycle": "monthly", "project": "Acme Shop" }),
        );
        assert!(
            result.contains("300.00 TRY monthly from 2026-10-23"),
            "{result}"
        );
        let bad = run.call("cancel_payment", json!({ "id": 1, "date": "2025-01-01" }));
        assert!(bad.starts_with("Error:"), "{bad}");
        run.call(
            "update_payment",
            json!({ "id": 1, "amount": 14.99, "from": "2026-11-05" }),
        );
        let groups = run.view();
        let kinds: Vec<_> = groups[0]
            .entries
            .iter()
            .map(|e| (&e.kind, &e.action))
            .collect();
        assert_eq!(
            kinds,
            [
                (&Kind::Folder, &Action::Add),
                (&Kind::Note, &Action::Move),
                (&Kind::Todo, &Action::Done),
                (&Kind::Payment, &Action::Add),
                (&Kind::Payment, &Action::Edit),
            ]
        );
        run.apply();
        let item = items::summary(&run.conn, 1).unwrap();
        assert_eq!(
            folders::list(&run.conn)
                .unwrap()
                .iter()
                .find(|f| Some(f.id) == item.folder_id)
                .unwrap()
                .name,
            "Billing"
        );
        assert!(todos::get(&run.conn, 1).unwrap().done);
        let payments = payments::list(&run.conn).unwrap();
        assert_eq!(payments.len(), 2);
        assert_eq!(payments[0].prices.last().unwrap().amount, 1499);
    }

    #[test]
    fn a_failing_op_rolls_back_the_whole_draft() {
        let mut run = Run::new("rename and delete");
        run.call(
            "rename",
            json!({ "kind": "note", "id": 1, "name": "Panel" }),
        );
        run.call("delete_note", json!({ "note": 2 }));
        items::delete(&run.conn, 2).unwrap();
        assert!(apply(&run.conn, &key(), &run.inputs, &run.draft.ops, 1).is_err());
        assert_eq!(items::summary(&run.conn, 1).unwrap().title, "Hosting panel");
    }

    #[test]
    fn project_names_match_loosely_but_not_wrongly() {
        let mut run = Run::new("x");
        let ctx = Ctx {
            conn: &run.conn,
            today: "2026-10-23",
            inputs: &run.inputs,
            stored: &run.stored,
        };
        let ws = Workspace::load(&ctx, &[]).unwrap();
        for text in ["acme", "ACME SHOP", "Acme Shop projesi", "acme işleri", "1"] {
            assert_eq!(ws.find_project(text).unwrap(), Some(Ref::Id(1)), "{text}");
        }
        for text in ["Acme Mobile", "Zumrut Veteriner", "ac"] {
            assert_eq!(ws.find_project(text).unwrap(), None, "{text}");
        }
        let result = run.call("create_project", json!({ "name": "acme" }));
        assert_eq!(result, "Project \"Acme Shop\" exists already; use it.");
    }
}
