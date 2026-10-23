//! `coffer --mcp`: a Model Context Protocol server on stdin/stdout, so an MCP client (Claude Code,
//! Claude Desktop) can ask about the user's projects. It runs the assistant's read tools on a
//! read-only connection and never touches the Keychain, Touch ID or the session, so it holds no
//! key and cannot decrypt anything: secrets only ever appear as handles. It answers only while
//! "Allow MCP access" is on, checked on every request so switching it off takes effect at once.

use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::{
    assistant::tools,
    db::{self, settings},
    error::{AppError, AppResult},
    time::local_today,
};

/// Must match `identifier` in `tauri.conf.json`: the app keeps its vault in this folder of
/// `~/Library/Application Support`.
const IDENTIFIER: &str = "com.apo.coffer";

/// Newest first; a client asking for another version gets the newest.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// The vault can't be read: access is off, there is no vault, or it needs the app first.
const UNAVAILABLE: i64 = -32001;

const ACCESS_OFF: &str =
    "MCP access is off. Turn on Allow MCP access in Coffer's Settings, under Integrations.";

const INSTRUCTIONS: &str = "Coffer is the user's local vault of projects, notes, todos and \
payments. Access is read-only. Secret values are never available here: they appear as handles \
such as [n12.0] (a secret in note 12), [p3.0] (in project 3's overview), [t4.0] (in todo 4) or \
[n12] (a whole secret note). To use one, the user reveals or copies it in Coffer.";

/// Runs the server until stdin closes. Returns the process exit code.
pub fn serve() -> i32 {
    let mut server = Server::new(|| db::open_read_only(&vault_path()?));
    match run(&mut server, io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("coffer --mcp: {e}");
            1
        }
    }
}

/// Newline-delimited JSON-RPC: one message per line each way.
fn run(server: &mut Server, reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle(&line) {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn vault_path() -> AppResult<PathBuf> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| AppError::File("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join("Library/Application Support")
        .join(IDENTIFIER)
        .join(crate::db_file()))
}

/// The `claude mcp add` command for this binary. Dev builds register under another name and
/// pass on the dev vault they read.
pub fn register_command(exe: &Path) -> String {
    #[cfg(debug_assertions)]
    let (name, env) = (
        "coffer-dev",
        format!(" -e COFFER_DEV_DB={}", shell_quote(&crate::dev::db_file())),
    );
    #[cfg(not(debug_assertions))]
    let (name, env) = ("coffer", String::new());
    format!(
        "claude mcp add {name}{env} -- {} --mcp",
        shell_quote(&exe.to_string_lossy())
    )
}

fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%=,".contains(c));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

struct Server {
    open: Box<dyn Fn() -> AppResult<Connection>>,
    /// Opened on the first request that needs it, and again after a failure.
    conn: Option<Connection>,
}

impl Server {
    fn new(open: impl Fn() -> AppResult<Connection> + 'static) -> Self {
        Self {
            open: Box::new(open),
            conn: None,
        }
    }

    /// The response to one message, or `None` for a notification.
    fn handle(&mut self, line: &str) -> Option<Value> {
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(_) => return Some(error(&Value::Null, PARSE_ERROR, "Parse error")),
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str);
        let valid = message.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
        let (Some(method), true) = (method, valid) else {
            // A response from the client, or something malformed. Only a request gets a reply.
            return id.map(|id| error(&id, INVALID_REQUEST, "Invalid request"));
        };
        // Notifications (`notifications/initialized`, cancellations) need no answer.
        let id = id?;
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        Some(match self.call(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => error(&id, e.code, &e.message),
        })
    }

    fn call(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({})),
            "initialize" => {
                self.vault()?;
                let asked = params.get("protocolVersion").and_then(Value::as_str);
                let version = PROTOCOL_VERSIONS
                    .iter()
                    .find(|v| Some(**v) == asked)
                    .unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "coffer", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "tools/list" => {
                self.vault()?;
                let list: Vec<Value> = tools::catalog()
                    .into_iter()
                    .map(|spec| {
                        json!({
                            "name": spec.name,
                            "description": spec.description,
                            "inputSchema": spec.parameters,
                            "annotations": { "readOnlyHint": true, "openWorldHint": false },
                        })
                    })
                    .collect();
                Ok(json!({ "tools": list }))
            }
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::new(INVALID_PARAMS, "name is required"))?;
                if !tools::catalog().iter().any(|spec| spec.name == name) {
                    return Err(RpcError::new(
                        INVALID_PARAMS,
                        format!("Unknown tool {name}"),
                    ));
                }
                let args = match params.get("arguments") {
                    None | Some(Value::Null) => json!({}),
                    Some(args) => args.clone(),
                };
                let conn = self.vault()?;
                let text = read(conn, |conn| {
                    tools::run(conn, &local_today("yyyy-MM-dd"), name, &args)
                })
                .map_err(|e| RpcError::new(UNAVAILABLE, e.to_string()))?;
                Ok(json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": text.starts_with("Error:"),
                }))
            }
            _ => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("Method not found: {method}"),
            )),
        }
    }

    /// The open vault, if access is on.
    fn vault(&mut self) -> Result<&mut Connection, RpcError> {
        let unavailable = |e: AppError| RpcError::new(UNAVAILABLE, e.to_string());
        if self.conn.is_none() {
            self.conn = Some((self.open)().map_err(unavailable)?);
        }
        let conn = self.conn.as_ref().expect("connection was just opened");
        match settings::mcp_access(conn) {
            Ok(true) => Ok(self.conn.as_mut().expect("connection was just opened")),
            Ok(false) => Err(RpcError::new(UNAVAILABLE, ACCESS_OFF)),
            Err(e) => {
                // Reopen next time: the file may have been replaced (a restore).
                self.conn = None;
                Err(unavailable(e))
            }
        }
    }
}

/// Runs `f` in one read transaction, so a tool sees the vault as it was at a single moment even
/// while the app writes.
fn read<T>(conn: &mut Connection, f: impl FnOnce(&Connection) -> T) -> AppResult<T> {
    let tx = conn.transaction()?;
    Ok(f(&tx))
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{Arc, Barrier},
        thread,
        time::Duration,
    };

    use zeroize::Zeroizing;

    use super::*;
    use crate::{
        commands::sealing::{seal_inline_with, seal_with},
        db::{
            folders,
            items::{self, Body, ItemRecord},
            payments::{self, PaymentRecord},
            projects,
            todos::{self, TodoRecord},
        },
    };

    const SECRETS: [&str; 5] = [
        "hunter2-panel",
        "AKIAOVERVIEW",
        "whole-body-secret",
        "todo-pin-4242",
        "second-secret",
    ];

    /// A vault file on disk with real sealed secrets, removed when dropped.
    struct TempVault(PathBuf);

    impl TempVault {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("coffer-mcp-{}-{name}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let conn = db::open(&path).unwrap();
            seed(&conn);
            Self(path)
        }

        fn writer(&self) -> Connection {
            db::open(&self.0).unwrap()
        }

        fn server(&self) -> Server {
            let path = self.0.clone();
            Server::new(move || db::open_read_only(&path))
        }

        fn allow(&self, on: bool) {
            settings::set_mcp_access(&self.writer(), on).unwrap();
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn seed(conn: &Connection) {
        let key = Zeroizing::new(vec![9; 32]);
        let acme = projects::insert(conn, "Acme Corp", "", 0).unwrap();
        let (readme, sealed) = seal_inline_with(&key, "AWS root: `!AKIAOVERVIEW!`")
            .unwrap()
            .unwrap();
        projects::update_readme(conn, acme, &readme, Some(&sealed), 0).unwrap();
        let infra = folders::insert(conn, acme, None, "Infra", 0).unwrap();
        let (template, secrets) = seal_inline_with(
            &key,
            "URL: `https://panel.example.com`\nPassword: `!hunter2-panel!` `!second-secret!`",
        )
        .unwrap()
        .unwrap();
        fn note<'a>(
            project_id: i64,
            folder_id: Option<i64>,
            title: &'a str,
            body: &'a Body,
        ) -> ItemRecord<'a> {
            ItemRecord {
                project_id,
                folder_id,
                title,
                body,
                tags: &[],
            }
        }
        let panel = Body::Inline { template, secrets };
        items::insert(conn, &note(acme, Some(infra), "Hosting panel", &panel), 0).unwrap();
        let secure = Body::Secure(seal_with(&key, "whole-body-secret").unwrap());
        items::insert(conn, &note(acme, None, "Recovery codes", &secure), 0).unwrap();
        let (description, sealed) = seal_inline_with(&key, "Plain part, PIN `!todo-pin-4242!`")
            .unwrap()
            .unwrap();
        todos::insert(
            conn,
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
        let payment = PaymentRecord {
            project_id: Some(acme),
            name: "Hetzner",
            currency: "EUR",
            cycle: "monthly",
            start_date: "2026-01-05",
            cancel_date: None,
        };
        payments::insert(conn, &payment, 1249, 0).unwrap();
    }

    fn request(id: i64, method: &str, params: Value) -> String {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn call(id: i64, name: &str, arguments: Value) -> String {
        request(
            id,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    /// Feeds `lines` through the stdio loop and returns every response, checking that none of
    /// them carries a secret.
    fn exchange(server: &mut Server, lines: &[String]) -> Vec<Value> {
        let input = lines.join("\n") + "\n";
        let mut output = Vec::new();
        run(server, Cursor::new(input), &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        for secret in SECRETS {
            assert!(!output.contains(secret), "leaked {secret}:\n{output}");
        }
        output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn text(response: &Value) -> &str {
        response["result"]["content"][0]["text"].as_str().unwrap()
    }

    #[test]
    fn round_trip_over_stdio() {
        let vault = TempVault::new("round-trip");
        vault.allow(true);
        let mut server = vault.server();
        let responses = exchange(
            &mut server,
            &[
                request(
                    1,
                    "initialize",
                    json!({ "protocolVersion": "2025-06-18", "capabilities": {},
                            "clientInfo": { "name": "test", "version": "0" } }),
                ),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
                request(2, "tools/list", json!({})),
                call(3, "get_note", json!({ "id": 1 })),
                request(4, "ping", json!({})),
                "{not json".to_string(),
                request(5, "resources/list", json!({})),
                call(6, "delete_note", json!({ "id": 1 })),
                call(7, "get_note", json!({ "id": 99 })),
            ],
        );
        // The notification got no answer, everything else did, in order.
        let ids: Vec<Value> = responses.iter().map(|r| r["id"].clone()).collect();
        let expected = [
            json!(1),
            json!(2),
            json!(3),
            json!(4),
            Value::Null,
            json!(5),
            json!(6),
        ];
        assert_eq!(ids[..7], expected);
        assert_eq!(ids[7], json!(7));
        let init = &responses[0]["result"];
        assert_eq!(init["protocolVersion"], "2025-06-18");
        assert_eq!(init["serverInfo"]["name"], "coffer");
        let tools = responses[1]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 8);
        assert!(
            tools
                .iter()
                .all(|t| t["annotations"]["readOnlyHint"] == true)
        );
        assert_eq!(tools[1]["inputSchema"]["required"], json!(["project"]));
        let note = text(&responses[2]);
        assert!(note.contains("Password: [n1.0] [n1.1]"), "{note}");
        assert_eq!(responses[2]["result"]["isError"], false);
        assert_eq!(responses[3]["result"], json!({}));
        assert_eq!(responses[4]["error"]["code"], PARSE_ERROR);
        assert_eq!(responses[5]["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(responses[6]["error"]["code"], INVALID_PARAMS);
        assert_eq!(text(&responses[7]), "Error: not found");
        assert_eq!(responses[7]["result"]["isError"], true);
    }

    #[test]
    fn no_tool_returns_a_secret() {
        let vault = TempVault::new("masking");
        vault.allow(true);
        let calls = [
            call(1, "list_projects", json!({})),
            call(2, "get_project", json!({ "project": "acme" })),
            call(3, "list_folder", json!({ "folder_id": 1 })),
            call(4, "search", json!({ "query": "password" })),
            call(5, "search", json!({ "query": "hunter2" })),
            call(6, "search", json!({ "query": "pin" })),
            call(7, "get_note", json!({ "id": 1 })),
            call(8, "get_note", json!({ "id": 2 })),
            call(9, "list_todos", json!({ "include_done": true })),
            call(10, "list_payments", json!({})),
            call(11, "project_spend", json!({ "project": "Acme Corp" })),
        ];
        // `exchange` fails on any secret in the output.
        let responses = exchange(&mut vault.server(), &calls);
        assert!(responses.iter().all(|r| r["result"]["isError"] == false));
        assert!(text(&responses[1]).contains("AWS root: [p1.0]"));
        assert!(text(&responses[7]).contains("[n2]"));
        assert!(text(&responses[8]).contains("Plain part, PIN [t1.0]"));
        assert!(text(&responses[10]).contains("12.49 EUR"));
    }

    #[test]
    fn refuses_while_access_is_off() {
        let vault = TempVault::new("switch");
        let mut server = vault.server();
        let refused = |responses: &[Value]| {
            responses
                .iter()
                .all(|r| r["error"]["code"] == UNAVAILABLE && r["error"]["message"] == ACCESS_OFF)
        };
        let off = exchange(
            &mut server,
            &[
                request(1, "initialize", json!({})),
                request(2, "tools/list", json!({})),
                call(3, "list_projects", json!({})),
            ],
        );
        assert!(refused(&off), "{off:?}");
        // Checked on every request: the switch works on a server that's already running.
        vault.allow(true);
        let on = exchange(&mut server, &[call(4, "list_projects", json!({}))]);
        assert!(text(&on[0]).contains("Acme Corp"));
        vault.allow(false);
        let off = exchange(&mut server, &[call(5, "list_projects", json!({}))]);
        assert!(refused(&off));
    }

    #[test]
    fn missing_or_old_vault_is_an_error() {
        let path = std::env::temp_dir().join(format!("coffer-mcp-{}-none.db", std::process::id()));
        let mut server = Server::new(move || db::open_read_only(&path));
        let responses = exchange(&mut server, &[request(1, "initialize", json!({}))]);
        assert_eq!(responses[0]["error"]["code"], UNAVAILABLE);
        assert!(
            responses[0]["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("No vault at")
        );

        let vault = TempVault::new("old");
        vault
            .writer()
            .pragma_update(None, "user_version", 3)
            .unwrap();
        assert!(matches!(
            db::open_read_only(&vault.0),
            Err(AppError::Invalid(message)) if message.contains("Open Coffer once")
        ));
    }

    #[test]
    fn the_read_connection_cannot_write() {
        let vault = TempVault::new("read-only");
        let conn = db::open_read_only(&vault.0).unwrap();
        assert!(settings::set_mcp_access(&conn, true).is_err());
        assert!(!settings::mcp_access(&vault.writer()).unwrap());
    }

    #[test]
    fn the_app_waits_for_a_read_instead_of_failing() {
        let vault = TempVault::new("busy");
        let mut reader = db::open_read_only(&vault.0).unwrap();
        let writer = vault.writer();
        let started = Arc::new(Barrier::new(2));
        let barrier = Arc::clone(&started);
        let handle = thread::spawn(move || {
            read(&mut reader, |conn| {
                // Hold the read lock while the app tries to commit.
                projects::list(conn).unwrap();
                barrier.wait();
                thread::sleep(Duration::from_millis(300));
            })
            .unwrap();
        });
        started.wait();
        projects::insert(&writer, "Written meanwhile", "", 1).unwrap();
        handle.join().unwrap();
        assert_eq!(projects::list(&writer).unwrap().len(), 2);
    }

    #[test]
    fn register_command_quotes_odd_paths() {
        let command = register_command(Path::new("/Applications/Coffer.app/Contents/MacOS/coffer"));
        assert!(command.starts_with("claude mcp add coffer"));
        assert!(command.ends_with(" -- /Applications/Coffer.app/Contents/MacOS/coffer --mcp"));
        let odd = register_command(Path::new("/Users/a b/it's/coffer"));
        assert!(
            odd.ends_with(r" -- '/Users/a b/it'\''s/coffer' --mcp"),
            "{odd}"
        );
    }

    #[test]
    fn identifier_matches_the_app() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
    }
}
