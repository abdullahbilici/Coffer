//! Read-only tools the model calls. They read stored rows and templates only, never decrypt
//! anything, and show every secret as a handle. Results are compact text for a small model.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{Value, json};

use super::{
    handles::{Handle, Source, mask},
    spend::{self, Totals},
};
use crate::{
    db::{
        folders::{self, Folder},
        items::{self, ItemSummary},
        payments::{self, Payment},
        projects::{self, ProjectSummary},
        todos::{self, Todo},
    },
    error::{AppError, AppResult},
    secrets,
};

/// Longest text a single tool returns; the model's context is small.
const MAX_RESULT: usize = 6_000;
const MAX_SEARCH_RESULTS: usize = 20;

/// A read tool: its name, what it does and a JSON schema of its arguments. The assistant and
/// the MCP server describe the same tools from this one list.
pub struct Spec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// The tool definitions in Ollama's format.
pub fn specs() -> Value {
    catalog()
        .into_iter()
        .map(|spec| {
            json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": spec.parameters,
                },
            })
        })
        .collect()
}

pub fn catalog() -> Vec<Spec> {
    let project = json!({
        "type": "string",
        "description": "Project name or id. \"personal\" means payments and todos without a project.",
    });
    let tool = |name, description, properties: Value, required: &[&str]| Spec {
        name,
        description,
        parameters: json!({ "type": "object", "properties": properties, "required": required }),
    };
    vec![
        tool(
            "list_projects",
            "All projects with what they contain.",
            json!({}),
            &[],
        ),
        tool(
            "get_project",
            "A project's overview, folders and notes.",
            json!({ "project": project }),
            &["project"],
        ),
        tool(
            "list_folder",
            "The subfolders and notes of a folder.",
            json!({ "folder_id": { "type": "integer" } }),
            &["folder_id"],
        ),
        tool(
            "search",
            "Finds notes, projects, folders, tags, todos and payments by words in their names, tags or note text.",
            json!({ "query": { "type": "string" } }),
            &["query"],
        ),
        tool(
            "get_note",
            "A note's full text, with secrets as handles.",
            json!({ "id": { "type": "integer" } }),
            &["id"],
        ),
        tool(
            "list_todos",
            "Todos, open ones by default.",
            json!({ "project": project, "include_done": { "type": "boolean" } }),
            &[],
        ),
        tool(
            "list_payments",
            "Payments with their prices, cycles and next due dates.",
            json!({ "project": project }),
            &[],
        ),
        tool(
            "project_spend",
            "What payments cost: per month now, this month, this year so far, or between two dates (YYYY-MM-DD, inclusive). Without a project, every project.",
            json!({
                "project": project,
                "from": { "type": "string" },
                "to": { "type": "string" },
            }),
            &[],
        ),
    ]
}

/// Runs a tool. Failures come back as text, so the model can correct itself.
pub fn run(conn: &Connection, today: &str, name: &str, args: &Value) -> String {
    let result = Vault::load(conn).and_then(|vault| {
        let tools = Tools { vault, today };
        match name {
            "list_projects" => Ok(tools.list_projects()),
            "get_project" => tools.get_project(conn, args),
            "list_folder" => tools.list_folder(args),
            "search" => tools.search(args),
            "get_note" => tools.get_note(conn, args),
            "list_todos" => tools.list_todos(args),
            "list_payments" => tools.list_payments(args),
            "project_spend" => tools.project_spend(args),
            _ => Err(AppError::Invalid(format!("Unknown tool {name}"))),
        }
    });
    match result {
        Ok(text) => clip(&text, MAX_RESULT),
        Err(AppError::NotFound) => "Error: not found".to_string(),
        Err(e) => format!("Error: {e}"),
    }
}

/// Everything readable at once; vaults are small.
struct Vault {
    projects: Vec<ProjectSummary>,
    folders: Vec<Folder>,
    items: Vec<ItemSummary>,
    todos: Vec<Todo>,
    payments: Vec<Payment>,
    /// Readable bodies by note id, secrets already shown as handles.
    bodies: HashMap<i64, String>,
}

impl Vault {
    fn load(conn: &Connection) -> AppResult<Self> {
        Ok(Self {
            projects: projects::list(conn)?,
            folders: folders::list(conn)?,
            items: items::list(conn)?,
            todos: todos::list(conn)?,
            payments: payments::list(conn)?,
            bodies: items::templates(conn)?
                .into_iter()
                .map(|(id, template)| (id, mask(&template, Source::Note, id)))
                .collect(),
        })
    }

    fn project_name(&self, id: Option<i64>) -> &str {
        id.and_then(|id| self.projects.iter().find(|p| p.id == id))
            .map_or("Personal", |p| p.name.as_str())
    }

    /// `Infra/AWS`, or empty at the project root.
    fn folder_path(&self, mut id: Option<i64>) -> String {
        let by_id: HashMap<i64, &Folder> = self.folders.iter().map(|f| (f.id, f)).collect();
        let mut names = Vec::new();
        while let Some(folder) = id.and_then(|id| by_id.get(&id)) {
            names.push(folder.name.as_str());
            id = folder.parent_id;
        }
        names.reverse();
        names.join("/")
    }

    /// `Acme Corp / Infra/AWS`.
    fn place(&self, project_id: i64, folder_id: Option<i64>) -> String {
        let project = self.project_name(Some(project_id));
        match self.folder_path(folder_id) {
            path if path.is_empty() => project.to_string(),
            path => format!("{project} / {path}"),
        }
    }

    fn write_contents(
        &self,
        out: &mut String,
        project_id: i64,
        folder_id: Option<i64>,
        depth: usize,
        deep: bool,
    ) {
        let indent = "  ".repeat(depth);
        for f in self
            .folders
            .iter()
            .filter(|f| f.project_id == project_id && f.parent_id == folder_id)
        {
            out.push_str(&format!("\n{indent}- folder {}: {}", f.id, f.name));
            if deep {
                self.write_contents(out, project_id, Some(f.id), depth + 1, deep);
            }
        }
        for i in self
            .items
            .iter()
            .filter(|i| i.project_id == project_id && i.folder_id == folder_id)
        {
            out.push_str(&format!("\n{indent}- {}", note_line(i)));
        }
    }

    /// A project by id or name: exact first, then a unique partial match. `Ok(None)` means
    /// personal (no project).
    fn find_project(&self, text: &str) -> AppResult<Option<&ProjectSummary>> {
        let text = text.trim().trim_start_matches('#');
        let lower = text.to_lowercase();
        if matches!(lower.as_str(), "personal" | "none" | "general") {
            return Ok(None);
        }
        if let Ok(id) = text.parse::<i64>()
            && let Some(project) = self.projects.iter().find(|p| p.id == id)
        {
            return Ok(Some(project));
        }
        if let Some(project) = self
            .projects
            .iter()
            .find(|p| p.name.to_lowercase() == lower)
        {
            return Ok(Some(project));
        }
        let partial: Vec<&ProjectSummary> = self
            .projects
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .collect();
        match partial[..] {
            [project] => Ok(Some(project)),
            _ => Err(AppError::Invalid(format!(
                "no single project matches \"{text}\". Projects: {}",
                self.projects
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// The project filter a tool was given: `None` for all, `Some(None)` for personal.
    fn project_filter(&self, args: &Value) -> AppResult<Option<Option<i64>>> {
        match args.get("project").and_then(Value::as_str) {
            Some(text) if !text.trim().is_empty() => {
                Ok(Some(self.find_project(text)?.map(|p| p.id)))
            }
            _ => Ok(None),
        }
    }
}

struct Tools<'a> {
    vault: Vault,
    today: &'a str,
}

impl Tools<'_> {
    fn list_projects(&self) -> String {
        let v = &self.vault;
        if v.projects.is_empty() {
            return "No projects.".to_string();
        }
        let mut out = format!("{} projects:", v.projects.len());
        for p in &v.projects {
            let notes = v.items.iter().filter(|i| i.project_id == p.id).count();
            let folders = v.folders.iter().filter(|f| f.project_id == p.id).count();
            let todos = v
                .todos
                .iter()
                .filter(|t| t.project_id == Some(p.id) && !t.done)
                .count();
            let payments = v
                .payments
                .iter()
                .filter(|x| x.project_id == Some(p.id))
                .count();
            out.push_str(&format!(
                "\n- {} (project {}): {notes} notes, {folders} folders, {todos} open todos, {payments} payments",
                p.name, p.id
            ));
        }
        out
    }

    fn get_project(&self, conn: &Connection, args: &Value) -> AppResult<String> {
        let v = &self.vault;
        let Some(summary) = v.find_project(str_arg(args, "project")?)? else {
            return Ok("Personal is not a project; use list_todos or list_payments.".to_string());
        };
        let project = projects::get(conn, summary.id)?;
        let mut out = format!("project {}: {}", project.id, project.name);
        let overview = mask(&project.readme, Source::Overview, project.id);
        if overview.trim().is_empty() {
            out.push_str("\nOverview: empty");
        } else {
            out.push_str(&format!("\nOverview:\n{}", clip(overview.trim(), 2_500)));
        }
        out.push_str(&self.folder_contents(project.id, None, true));
        let todos = v
            .todos
            .iter()
            .filter(|t| t.project_id == Some(project.id) && !t.done)
            .count();
        let payments: Vec<&Payment> = v
            .payments
            .iter()
            .filter(|p| p.project_id == Some(project.id))
            .collect();
        out.push_str(&format!(
            "\nOpen todos: {todos}. Payments: {}, per month now {}.",
            payments.len(),
            spend::totals_text(&spend::monthly_rate(payments, self.today))
        ));
        Ok(out)
    }

    /// Subfolders and notes under `folder_id` (or the project root), recursively when `deep`.
    fn folder_contents(&self, project_id: i64, folder_id: Option<i64>, deep: bool) -> String {
        let mut out = String::new();
        self.vault
            .write_contents(&mut out, project_id, folder_id, 0, deep);
        if out.is_empty() {
            "\nContents: empty".to_string()
        } else {
            format!("\nContents:{out}")
        }
    }

    fn list_folder(&self, args: &Value) -> AppResult<String> {
        let id = int_arg(args, "folder_id")?;
        let folder = self
            .vault
            .folders
            .iter()
            .find(|f| f.id == id)
            .ok_or(AppError::NotFound)?;
        Ok(format!(
            "folder {}: {}{}",
            folder.id,
            self.vault.place(folder.project_id, Some(folder.id)),
            self.folder_contents(folder.project_id, Some(folder.id), false)
        ))
    }

    fn search(&self, args: &Value) -> AppResult<String> {
        let v = &self.vault;
        let query = str_arg(args, "query")?.trim().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return Err(AppError::Invalid("query is empty".to_string()));
        }
        // (words matched, name matched, line)
        let mut hits: Vec<(usize, bool, String)> = Vec::new();
        let mut consider = |name: &str, other: &str, line: String| {
            let name = name.to_lowercase();
            let text = format!("{name}\n{}", other.to_lowercase());
            let matched = words.iter().filter(|w| text.contains(*w)).count();
            if matched > 0 {
                hits.push((matched, words.iter().any(|w| name.contains(w)), line));
            }
        };

        for p in &v.projects {
            consider(&p.name, "", format!("project {}: {}", p.id, p.name));
        }
        for f in &v.folders {
            let place = v.place(f.project_id, f.parent_id);
            consider(
                &f.name,
                &place,
                format!("folder {}: {} (in {place})", f.id, f.name),
            );
        }
        for i in &v.items {
            let place = v.place(i.project_id, i.folder_id);
            let body = v.bodies.get(&i.id).map_or("", String::as_str);
            let mut line = format!("{} in {place}", note_line(i));
            if let Some(snippet) = snippet(body, &words) {
                line.push_str(&format!(" — text: \"{snippet}\""));
            }
            consider(
                &i.title,
                &format!("{}\n{place}\n{body}", i.tags.join(" ")),
                line,
            );
        }
        for t in &v.todos {
            let description = secrets::strip_markers(&t.description);
            consider(&t.title, &description, todo_line(t, v, self.today));
        }
        for p in &v.payments {
            consider(&p.name, v.project_name(p.project_id), self.payment_line(p));
        }

        if hits.is_empty() {
            return Ok(format!("Nothing matches \"{query}\"."));
        }
        let best = hits.iter().map(|h| h.0).max().unwrap_or(0);
        hits.retain(|h| h.0 == best);
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.1));
        let total = hits.len();
        let noun = if total == 1 { "result" } else { "results" };
        let mut out = format!("{total} {noun} for \"{query}\":");
        for (_, _, line) in hits.iter().take(MAX_SEARCH_RESULTS) {
            out.push_str(&format!("\n- {line}"));
        }
        if total > MAX_SEARCH_RESULTS {
            out.push_str(&format!(
                "\n({} more; search more precisely)",
                total - MAX_SEARCH_RESULTS
            ));
        }
        Ok(out)
    }

    fn get_note(&self, conn: &Connection, args: &Value) -> AppResult<String> {
        let id = int_arg(args, "id")?;
        let item = items::get(conn, id)?;
        let s = &item.summary;
        let mut out = format!(
            "note {}: {}\nIn: {}",
            s.id,
            s.title,
            self.vault.place(s.project_id, s.folder_id)
        );
        if !s.tags.is_empty() {
            out.push_str(&format!("\nTags: {}", s.tags.join(", ")));
        }
        let todos: Vec<String> = self
            .vault
            .todos
            .iter()
            .filter(|t| t.item_id == Some(s.id))
            .map(|t| todo_line(t, &self.vault, self.today))
            .collect();
        if !todos.is_empty() {
            out.push_str(&format!("\nLinked todos:\n- {}", todos.join("\n- ")));
        }
        match item.body {
            _ if s.secure => {
                let handle = Handle {
                    source: Source::Note,
                    id: s.id,
                    index: None,
                };
                out.push_str(&format!(
                    "\nThe whole note is secret: {handle}. Its text is hidden from you."
                ));
            }
            Some(body) if !body.trim().is_empty() => {
                out.push_str(&format!("\n\n{}", mask(&body, Source::Note, s.id)));
            }
            _ => out.push_str("\nEmpty."),
        }
        Ok(out)
    }

    fn list_todos(&self, args: &Value) -> AppResult<String> {
        let v = &self.vault;
        let filter = v.project_filter(args)?;
        let include_done = args
            .get("include_done")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let todos: Vec<&Todo> = v
            .todos
            .iter()
            .filter(|t| filter.is_none_or(|p| t.project_id == p))
            .filter(|t| include_done || !t.done)
            .collect();
        let kind = if include_done { "todos" } else { "open todos" };
        if todos.is_empty() {
            return Ok(format!("No {kind}."));
        }
        let mut out = format!("{} {kind}:", todos.len());
        for t in todos {
            out.push_str(&format!("\n- {}", todo_line(t, v, self.today)));
            let description = mask(&t.description, Source::Todo, t.id);
            let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
            if !description.is_empty() {
                out.push_str(&format!("\n  {}", clip(&description, 300)));
            }
        }
        Ok(out)
    }

    fn payment_line(&self, p: &Payment) -> String {
        let price = spend::price_on(p, self.today.max(p.start_date.as_str()));
        let mut line = format!(
            "payment {}: {} ({}), {} {}",
            p.id,
            p.name,
            self.vault.project_name(p.project_id),
            spend::money(price, &p.currency),
            p.cycle
        );
        if p.cycle == "once" {
            line.push_str(&format!(" on {}", p.start_date));
        } else {
            line.push_str(&format!(" from {}", p.start_date));
        }
        if p.prices.len() > 1 {
            let history: Vec<String> = p
                .prices
                .iter()
                .map(|x| format!("{} from {}", spend::money(x.amount, &p.currency), x.from))
                .collect();
            line.push_str(&format!(", prices: {}", history.join(", ")));
        }
        match (&p.cancel_date, spend::next_due(p, self.today)) {
            (Some(cancel), _) if cancel.as_str() <= self.today => {
                line.push_str(&format!(", cancelled from {cancel}"));
            }
            (cancel, next) => {
                if let Some(next) = next {
                    line.push_str(&format!(", next due {next}"));
                }
                if let Some(cancel) = cancel {
                    line.push_str(&format!(", ends {cancel}"));
                }
            }
        }
        line
    }

    fn list_payments(&self, args: &Value) -> AppResult<String> {
        let v = &self.vault;
        let filter = v.project_filter(args)?;
        let payments: Vec<&Payment> = v
            .payments
            .iter()
            .filter(|p| filter.is_none_or(|id| p.project_id == id))
            .collect();
        if payments.is_empty() {
            return Ok("No payments.".to_string());
        }
        let mut out = format!("{} payments:", payments.len());
        for p in payments {
            out.push_str(&format!("\n- {}", self.payment_line(p)));
        }
        Ok(out)
    }

    fn project_spend(&self, args: &Value) -> AppResult<String> {
        let v = &self.vault;
        let filter = v.project_filter(args)?;
        let from = opt_date(args, "from")?;
        let to = opt_date(args, "to")?;
        // Groups: one per project with payments, then personal ones.
        let mut groups: Vec<(Option<i64>, Vec<&Payment>)> = Vec::new();
        for p in &v.payments {
            if filter.is_some_and(|id| p.project_id != id) {
                continue;
            }
            match groups.iter_mut().find(|(id, _)| *id == p.project_id) {
                Some((_, list)) => list.push(p),
                None => groups.push((p.project_id, vec![p])),
            }
        }
        groups.sort_by_key(|(id, _)| (id.is_none(), v.project_name(*id).to_lowercase()));
        if groups.is_empty() {
            return Ok("No payments.".to_string());
        }

        let today = self.today;
        let month_start = format!("{}-01", &today[..7]);
        let year_start = format!("{}-01-01", &today[..4]);
        let tomorrow = spend::add_days(today, 1);
        let mut out = String::new();
        let mut all: Vec<&Payment> = Vec::new();
        for (id, payments) in &groups {
            out.push_str(&format!("{}:\n", v.project_name(*id)));
            match (from, to) {
                (None, None) => {
                    let rate = spend::monthly_rate(payments.iter().copied(), today);
                    let month = spend::total_due(
                        payments.iter().copied(),
                        &month_start,
                        &spend::add_months(&month_start, 1),
                    );
                    let year = spend::total_due(payments.iter().copied(), &year_start, &tomorrow);
                    out.push_str(&format!(
                        "- per month now (monthly plus a twelfth of yearly): {}\n- due this month ({} to {}): {}\n- paid this year so far ({year_start} to {today}): {}\n",
                        spend::totals_text(&rate),
                        month_start,
                        spend::add_days(&spend::add_months(&month_start, 1), -1),
                        spend::totals_text(&month),
                        spend::totals_text(&year),
                    ));
                }
                (from, to) => {
                    let from = from.unwrap_or("0000-01-01");
                    let to = to.unwrap_or("9999-12-31");
                    let end = spend::add_days(to, 1);
                    let total = spend::total_due(payments.iter().copied(), from, &end);
                    out.push_str(&format!(
                        "- due {from} to {to}: {}\n",
                        spend::totals_text(&total)
                    ));
                }
            }
            for p in payments {
                out.push_str(&format!("  - {}\n", self.payment_line(p)));
            }
            all.extend(payments);
        }
        if groups.len() > 1 {
            let rate: Totals = spend::monthly_rate(all.iter().copied(), today);
            out.push_str(&format!(
                "All of these, per month now: {}",
                spend::totals_text(&rate)
            ));
        }
        Ok(out.trim_end().to_string())
    }
}

fn note_line(item: &ItemSummary) -> String {
    let mut line = format!("note {}: {}", item.id, item.title);
    if item.secure {
        line.push_str(" (fully secret)");
    } else if item.has_secrets {
        line.push_str(" (has secrets)");
    }
    if !item.tags.is_empty() {
        line.push_str(&format!(" [{}]", item.tags.join(", ")));
    }
    line
}

fn todo_line(todo: &Todo, vault: &Vault, today: &str) -> String {
    let mut line = format!(
        "todo {}: {} ({})",
        todo.id,
        todo.title,
        vault.project_name(todo.project_id)
    );
    if todo.done {
        line.push_str(", done");
    }
    if let Some(due) = &todo.due_date {
        line.push_str(&format!(", due {due}"));
        if !todo.done {
            match due.as_str().cmp(today) {
                std::cmp::Ordering::Less => line.push_str(" (overdue)"),
                std::cmp::Ordering::Equal => line.push_str(" (today)"),
                std::cmp::Ordering::Greater => {}
            }
        }
    }
    if let Some(item) = todo.item_id {
        line.push_str(&format!(", linked to note {item}"));
    }
    line
}

/// The line of `body` around the first matched word, shortened.
fn snippet(body: &str, words: &[&str]) -> Option<String> {
    let line = body.lines().find(|line| {
        let lower = line.to_lowercase();
        words.iter().any(|w| lower.contains(w))
    })?;
    Some(clip(line.trim(), 120))
}

/// Cuts `text` to `max` bytes at a char boundary, marking the cut.
pub(super) fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (cut)", &text[..end])
}

pub(super) fn str_arg<'a>(args: &'a Value, name: &str) -> AppResult<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Invalid(format!("{name} is required")))
}

/// Accepts a number or a numeric string, since small models send either.
pub(super) fn int_arg(args: &Value, name: &str) -> AppResult<i64> {
    let value = args.get(name);
    value
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .and_then(Value::as_str)
                .and_then(|s| s.trim().trim_start_matches('#').parse().ok())
        })
        .ok_or_else(|| AppError::Invalid(format!("{name} must be a number")))
}

fn opt_date<'a>(args: &'a Value, name: &str) -> AppResult<Option<&'a str>> {
    match args.get(name).and_then(Value::as_str).map(str::trim) {
        None | Some("") => Ok(None),
        Some(date) => crate::commands::valid_date(date)
            .map(Some)
            .map_err(|_| AppError::Invalid(format!("{name} must be YYYY-MM-DD"))),
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;
    use crate::{
        commands::sealing::{seal_inline_with, seal_with},
        db::{
            items::{Body, ItemRecord},
            open_in_memory,
            payments::PaymentRecord,
            todos::TodoRecord,
        },
    };

    const TODAY: &str = "2026-10-23";
    const SECRETS: [&str; 4] = [
        "hunter2-panel",
        "AKIAOVERVIEW",
        "whole-body-secret",
        "todo-pin-4242",
    ];

    /// A vault whose every secret is sealed for real, so a leak would show up in the text.
    fn vault() -> Connection {
        let conn = open_in_memory();
        let key = Zeroizing::new(vec![9; 32]);
        let acme = projects::insert(&conn, "Acme Corp", "", 0).unwrap();
        let (readme, sealed) = seal_inline_with(&key, "AWS root: `!AKIAOVERVIEW!`")
            .unwrap()
            .unwrap();
        projects::update_readme(&conn, acme, &readme, Some(&sealed), 0).unwrap();
        let infra = folders::insert(&conn, acme, None, "Infra", 0).unwrap();
        let hosting = folders::insert(&conn, acme, Some(infra), "Hosting", 0).unwrap();
        let (template, secrets) = seal_inline_with(
            &key,
            "URL: `https://panel.example.com`\nUser: `apo`\nPassword: `!hunter2-panel!`",
        )
        .unwrap()
        .unwrap();
        let panel = Body::Inline { template, secrets };
        let tags = ["env/prod".to_string()];
        items::insert(
            &conn,
            &ItemRecord {
                project_id: acme,
                folder_id: Some(hosting),
                title: "Hosting panel",
                body: &panel,
                tags: &tags,
            },
            0,
        )
        .unwrap();
        let secure = Body::Secure(seal_with(&key, "whole-body-secret").unwrap());
        items::insert(
            &conn,
            &ItemRecord {
                project_id: acme,
                folder_id: None,
                title: "Recovery codes",
                body: &secure,
                tags: &[],
            },
            0,
        )
        .unwrap();
        let (description, sealed) = seal_inline_with(&key, "PIN `!todo-pin-4242!`")
            .unwrap()
            .unwrap();
        todos::insert(
            &conn,
            &TodoRecord {
                project_id: Some(acme),
                item_id: Some(1),
                title: "Renew SSL",
                description: &description,
                secrets: Some(&sealed),
                due_date: Some("2026-10-20"),
            },
            0,
        )
        .unwrap();
        let hosting_payment = PaymentRecord {
            project_id: Some(acme),
            name: "Hetzner",
            currency: "EUR",
            cycle: "monthly",
            start_date: "2026-01-05",
            cancel_date: None,
        };
        payments::insert(&conn, &hosting_payment, 1249, 0).unwrap();
        let domain = PaymentRecord {
            name: "Domain",
            currency: "USD",
            cycle: "yearly",
            start_date: "2025-03-01",
            ..hosting_payment
        };
        payments::insert(&conn, &domain, 1200, 0).unwrap();
        let personal = PaymentRecord {
            project_id: None,
            name: "Spotify",
            ..hosting_payment
        };
        payments::insert(&conn, &personal, 1099, 0).unwrap();
        conn
    }

    fn call(conn: &Connection, name: &str, args: Value) -> String {
        let text = run(conn, TODAY, name, &args);
        for secret in SECRETS {
            assert!(!text.contains(secret), "{name} leaked {secret}:\n{text}");
        }
        text
    }

    #[test]
    fn no_tool_ever_returns_a_secret() {
        let conn = vault();
        let calls = [
            ("list_projects", json!({})),
            ("get_project", json!({ "project": "acme" })),
            ("list_folder", json!({ "folder_id": 1 })),
            ("list_folder", json!({ "folder_id": 2 })),
            ("search", json!({ "query": "password" })),
            ("search", json!({ "query": "hunter2" })),
            ("search", json!({ "query": "whole" })),
            ("get_note", json!({ "id": 1 })),
            ("get_note", json!({ "id": "2" })),
            ("list_todos", json!({ "include_done": true })),
            ("list_payments", json!({})),
            ("project_spend", json!({})),
        ];
        for (name, args) in calls {
            assert!(!call(&conn, name, args).starts_with("Error"), "{name}");
        }
    }

    #[test]
    fn secrets_show_as_handles() {
        let conn = vault();
        let note = call(&conn, "get_note", json!({ "id": 1 }));
        assert!(note.contains("Password: [n1.0]"), "{note}");
        assert!(note.contains("User: `apo`"));
        assert!(note.contains("In: Acme Corp / Infra/Hosting"));
        assert!(note.contains("todo 1: Renew SSL"));
        let secure = call(&conn, "get_note", json!({ "id": 2 }));
        assert!(secure.contains("[n2]") && !secure.contains("[n2.0]"));
        let project = call(&conn, "get_project", json!({ "project": "Acme Corp" }));
        assert!(project.contains("AWS root: [p1.0]"), "{project}");
        assert!(project.contains("    - note 1: Hosting panel (has secrets) [env/prod]"));
        let todos = call(&conn, "list_todos", json!({}));
        assert!(
            todos.contains("PIN [t1.0]") && todos.contains("(overdue)"),
            "{todos}"
        );
    }

    #[test]
    fn search_ranks_full_matches_and_quotes_text() {
        let conn = vault();
        let found = call(&conn, "search", json!({ "query": "panel url" }));
        assert!(found.starts_with("1 result for"), "{found}");
        assert!(found.contains("text: \"URL: `https://panel.example.com`\""));
        let partial = call(&conn, "search", json!({ "query": "hosting zebra" }));
        assert!(partial.contains("folder 2: Hosting") && partial.contains("note 1"));
        assert!(call(&conn, "search", json!({ "query": "zebra" })).starts_with("Nothing"));
    }

    #[test]
    fn spend_is_computed_per_currency() {
        let conn = vault();
        let acme = call(&conn, "project_spend", json!({ "project": "acme" }));
        assert!(
            acme.contains("per month now (monthly plus a twelfth of yearly): 12.49 EUR + 1.00 USD"),
            "{acme}"
        );
        assert!(
            acme.contains(
                "paid this year so far (2026-01-01 to 2026-10-23): 124.90 EUR + 12.00 USD"
            )
        );
        assert!(!acme.contains("Spotify"));
        let all = call(&conn, "project_spend", json!({}));
        assert!(
            all.contains("Personal:") && all.ends_with("23.48 EUR + 1.00 USD"),
            "{all}"
        );
        let range = call(
            &conn,
            "project_spend",
            json!({ "project": "personal", "from": "2026-01-01", "to": "2026-03-05" }),
        );
        assert!(
            range.contains("due 2026-01-01 to 2026-03-05: 32.97 EUR"),
            "{range}"
        );
        let payments = call(&conn, "list_payments", json!({ "project": "Acme" }));
        assert!(
            payments.contains(
                "Domain (Acme Corp), 12.00 USD yearly from 2025-03-01, next due 2027-03-01"
            ),
            "{payments}"
        );
    }

    #[test]
    fn bad_input_comes_back_as_text() {
        let conn = vault();
        assert_eq!(
            call(&conn, "get_note", json!({ "id": 99 })),
            "Error: not found"
        );
        assert!(
            call(&conn, "get_project", json!({ "project": "zzz" })).contains("Projects: Acme Corp")
        );
        assert!(call(&conn, "nope", json!({})).starts_with("Error: Unknown tool"));
        assert!(call(&conn, "project_spend", json!({ "from": "soon" })).contains("YYYY-MM-DD"));
        assert!(call(&conn, "get_note", json!({})).contains("id must be a number"));
    }

    #[test]
    fn specs_name_every_tool() {
        let names: Vec<String> = specs()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect();
        let conn = vault();
        for name in &names {
            assert!(!run(&conn, TODAY, name, &json!({})).starts_with("Error: Unknown"));
        }
        assert_eq!(names.len(), 8);
    }
}
