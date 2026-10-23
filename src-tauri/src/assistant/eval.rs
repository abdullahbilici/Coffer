//! An evaluation of the real agent against a local Ollama, opt-in:
//!
//! ```sh
//! COFFER_EVAL_MODEL=qwen3.8:27b cargo test --lib eval -- --ignored --nocapture
//! ```
//!
//! It builds a synthetic vault that looks like a few years of use (every value is fake),
//! renders parts of it as messy dumps, and runs the same reply path as the app: imports into an
//! empty vault, an import of new and changed values into a full one, and natural-language
//! commands. Drafts are applied to an in-memory database and scored on what ends up stored.
//! `COFFER_EVAL_TASKS` picks tasks (import_S, import_M, add, edit), `COFFER_EVAL_URL` the server;
//! results go to `target/eval/<model>.json`.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write as _,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};

use rusqlite::Connection;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    Reply, Request, Turn,
    agent::Model,
    draft, fields, import_prompt,
    ollama::Ollama,
    reply::{self, Io, Prompts},
    system_prompt,
};
use crate::{
    commands::sealing::{open_with, seal_inline_with},
    crypto::VaultKey,
    db::{
        folders,
        items::{self, Body, ItemRecord},
        open_in_memory,
        payments::{self, PaymentRecord},
        projects,
        todos::{self, TodoRecord},
    },
    error::AppResult,
};

const TODAY: &str = "2026-10-23";

// --- values ----------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 10_000) as f64 / 10_000.0 < p
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
    fn sample<T: Clone>(&mut self, items: &[T], n: usize) -> Vec<T> {
        let mut all = items.to_vec();
        self.shuffle(&mut all);
        all.truncate(n);
        all
    }
    fn chars(&mut self, n: usize, alphabet: &str) -> String {
        let chars: Vec<char> = alphabet.chars().collect();
        (0..n).map(|_| *self.pick(&chars)).collect()
    }
}

const ALNUM: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER_DIGITS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const HEX: &str = "0123456789abcdef";
const WORDS: [&str; 14] = [
    "mavi", "kedi", "deniz", "kahve", "limon", "zeytin", "yildiz", "bulut", "nehir", "kartal",
    "tarcin", "elma", "ruzgar", "orman",
];
const DOMAINS: [&str; 6] = [
    "hostfast",
    "cloudnest",
    "webbox",
    "serverpark",
    "netanadolu",
    "bulutsu",
];

fn cap(word: &str) -> String {
    let mut c = word.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn generate(r: &mut Rng, kind: &str) -> String {
    let word = |r: &mut Rng| r.pick(&WORDS).to_string();
    let domain = |r: &mut Rng| r.pick(&DOMAINS).to_string();
    match kind {
        "pw" => match r.below(3) {
            0 => format!(
                "{}{}{}",
                cap(&word(r)),
                r.range(10, 9999),
                r.chars(1, "!?#*")
            ),
            1 => {
                let n = r.range(10, 14);
                r.chars(n, &format!("{ALNUM}!@#$%&*"))
            }
            _ => format!("{}-{}-{}", cap(&word(r)), cap(&word(r)), r.range(10, 99)),
        },
        "user" => format!("{}_{}{}", word(r), r.chars(3, LOWER), r.range(1, 99)),
        "email" => format!("{}.{}@{}.example", word(r), r.chars(4, LOWER), domain(r)),
        "url" => format!(
            "https://{}.{}.example/{}",
            r.chars(5, LOWER),
            domain(r),
            r.pick(&["admin", "panel", "login", "wp-admin"])
        ),
        "host" => format!("{}.{}.example", r.chars(6, LOWER), domain(r)),
        "ip" => format!(
            "{}.{}",
            r.pick(&["203.0.113", "198.51.100", "192.0.2"]),
            r.range(2, 254)
        ),
        "dbname" => format!(
            "{}_{}{}",
            word(r),
            r.pick(&["prod", "db", "main", "app"]),
            r.range(1, 99)
        ),
        "region" => format!(
            "{}-{}",
            r.pick(&["eu-central-1", "eu-west-1", "us-east-1", "eu-north-1"]),
            r.range(1, 9)
        ),
        "akia" => format!("AKIA{}", r.chars(16, UPPER_DIGITS)),
        "awssecret" => r.chars(40, &format!("{ALNUM}/+")),
        "pk" => format!("pk_live_{}", r.chars(24, ALNUM)),
        "sk" => format!("sk_live_{}", r.chars(24, ALNUM)),
        "ghp" => format!("ghp_{}", r.chars(36, ALNUM)),
        "sg" => format!("SG.{}.{}", r.chars(22, ALNUM), r.chars(43, ALNUM)),
        "twsid" => format!("AC{}", r.chars(32, HEX)),
        "hex32" => r.chars(32, HEX),
        "skproj" => format!("sk-proj-{}", r.chars(40, ALNUM)),
        "aiza" => format!("AIza{}", r.chars(35, ALNUM)),
        "slug" => format!("{}{}", word(r), r.range(10, 999)),
        "dsn" => format!(
            "https://{}@o{}.ingest.sentry.example/{}",
            r.chars(32, HEX),
            r.range(100_000, 999_999),
            r.range(1000, 9999)
        ),
        "cftoken" => r.chars(40, &format!("{ALNUM}_-")),
        "handle" => format!("@{}{}", word(r), r.chars(3, LOWER)),
        "ga" => format!("G-{}", r.chars(10, UPPER_DIGITS)),
        "teamid" => r.chars(10, UPPER_DIGITS),
        "tgtoken" => format!(
            "{}:AA{}",
            r.range(1_000_000_000, 1_999_999_999),
            r.chars(33, &format!("{ALNUM}_-"))
        ),
        "pin" => r.range(100_000, 999_999).to_string(),
        "ssid" => format!("{}_{}", cap(&word(r)), r.chars(4, UPPER_DIGITS)),
        "jwt" => format!(
            "eyJ{}.{}.{}",
            r.chars(30, ALNUM),
            r.chars(60, ALNUM),
            r.chars(30, ALNUM)
        ),
        "backup" => (0..4)
            .map(|_| r.chars(4, "abcdefghijklmnopqrstuvwxyz0123456789"))
            .collect::<Vec<_>>()
            .join("-"),
        other => panic!("unknown kind {other}"),
    }
}

/// (label, kind, secret): `None` accepts either answer.
type Spec = (&'static str, &'static str, Option<bool>);

const SERVICES: [(&str, &[Spec]); 24] = [
    (
        "AWS",
        &[
            ("access key id", "akia", None),
            ("secret access key", "awssecret", Some(true)),
            ("region", "region", Some(false)),
        ],
    ),
    (
        "Stripe",
        &[
            ("publishable key", "pk", Some(false)),
            ("secret key", "sk", Some(true)),
        ],
    ),
    (
        "GitHub",
        &[
            ("username", "user", Some(false)),
            ("token", "ghp", Some(true)),
        ],
    ),
    (
        "Hosting panel",
        &[
            ("url", "url", Some(false)),
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "WordPress",
        &[
            ("url", "url", Some(false)),
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "Postgres",
        &[
            ("host", "host", Some(false)),
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
            ("database", "dbname", Some(false)),
        ],
    ),
    (
        "Redis",
        &[
            ("host", "host", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "SSH server",
        &[
            ("host", "ip", Some(false)),
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "SMTP",
        &[
            ("host", "host", Some(false)),
            ("email", "email", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    ("SendGrid", &[("api key", "sg", Some(true))]),
    (
        "Twilio",
        &[
            ("account sid", "twsid", Some(false)),
            ("auth token", "hex32", Some(true)),
        ],
    ),
    ("OpenAI", &[("api key", "skproj", Some(true))]),
    (
        "Firebase",
        &[
            ("api key", "aiza", None),
            ("project id", "slug", Some(false)),
        ],
    ),
    ("Sentry", &[("dsn", "dsn", None)]),
    (
        "Cloudflare",
        &[
            ("email", "email", Some(false)),
            ("password", "pw", Some(true)),
            ("api token", "cftoken", Some(true)),
        ],
    ),
    (
        "Domain registrar",
        &[
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "Instagram",
        &[
            ("handle", "handle", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    ("Google Analytics", &[("measurement id", "ga", Some(false))]),
    (
        "Apple Developer",
        &[
            ("apple id", "email", Some(false)),
            ("password", "pw", Some(true)),
            ("team id", "teamid", Some(false)),
        ],
    ),
    ("Telegram bot", &[("token", "tgtoken", Some(true))]),
    (
        "VPN",
        &[("server", "host", Some(false)), ("code", "pin", Some(true))],
    ),
    (
        "Supabase",
        &[
            ("url", "url", Some(false)),
            ("anon key", "jwt", None),
            ("service role key", "jwt", Some(true)),
        ],
    ),
    (
        "MongoDB Atlas",
        &[
            ("cluster", "host", Some(false)),
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "Admin panel",
        &[
            ("url", "url", Some(false)),
            ("email", "email", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
];
const PERSONAL: [(&str, &[Spec]); 5] = [
    (
        "Netflix",
        &[
            ("email", "email", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "Home Wi-Fi",
        &[
            ("network", "ssid", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
    (
        "Router admin",
        &[("url", "url", Some(false)), ("password", "pw", Some(true))],
    ),
    ("Gym locker", &[("code", "pin", Some(true))]),
    (
        "Steam",
        &[
            ("user", "user", Some(false)),
            ("password", "pw", Some(true)),
        ],
    ),
];

fn category(service: &str) -> Option<&'static str> {
    Some(match service {
        "AWS" | "SSH server" | "Postgres" | "Redis" | "MongoDB Atlas" | "Cloudflare"
        | "Supabase" | "Hosting panel" => "Infra",
        "Stripe" => "Payments",
        "SendGrid" | "SMTP" => "Email",
        "Instagram" | "Google Analytics" => "Marketing",
        "Apple Developer" | "Firebase" => "Mobile",
        _ => return None,
    })
}

fn tr_label(label: &str) -> &str {
    match label {
        "password" => "şifre",
        "user" => "kullanıcı",
        "email" => "mail",
        "host" => "sunucu",
        "url" => "link",
        "code" => "kod",
        "server" => "sunucu",
        "network" => "ağ adı",
        "username" => "kullanıcı adı",
        other => other,
    }
}

const CLIENTS: [&str; 16] = [
    "Acme Shop",
    "Bilici Kafe",
    "Yildiz Insaat",
    "Mavi Klinik",
    "Kuzey Lojistik",
    "Atlas Fitness",
    "Pera Otel",
    "Nova Akademi",
    "Delta Muhasebe",
    "Ege Zeytin",
    "Lale Cicekcilik",
    "Toros Otomotiv",
    "Kardelen Dis",
    "Marmara Emlak",
    "Sirius Yazilim",
    "Galata Sanat",
];
const SIDE: [&str; 6] = [
    "weatherbot",
    "habit-tracker",
    "recipe-app",
    "crypto-alert",
    "invoice-gen",
    "mini-crm",
];

// --- the synthetic vault ---------------------------------------------------------------

#[derive(Clone)]
struct TrueField {
    label: String,
    value: String,
    secret: Option<bool>,
}

#[derive(Clone)]
struct TrueNote {
    key: usize,
    service: String,
    env: Option<&'static str>,
    folder: Option<&'static str>,
    fields: Vec<TrueField>,
    /// Its id once seeded.
    id: i64,
}

impl TrueNote {
    fn title(&self) -> String {
        match self.env {
            Some(env) => format!("{} ({env})", self.service),
            None => self.service.clone(),
        }
    }
}

#[derive(Clone)]
struct TrueProject {
    name: String,
    aliases: Vec<String>,
    notes: Vec<TrueNote>,
}

impl TrueProject {
    fn folders(&self) -> Vec<&'static str> {
        let mut all: Vec<&str> = self.notes.iter().filter_map(|n| n.folder).collect();
        all.sort();
        all.dedup();
        all
    }
}

struct Gen {
    r: Rng,
    used: HashSet<String>,
    next_note: usize,
}

impl Gen {
    fn value(&mut self, kind: &str) -> String {
        loop {
            let v = generate(&mut self.r, kind);
            let clash = self
                .used
                .iter()
                .any(|u| u.len() >= 4 && v.len() >= 4 && (u.contains(&v) || v.contains(u)));
            if !clash {
                self.used.insert(v.clone());
                return v;
            }
        }
    }

    fn note(
        &mut self,
        service: &str,
        spec: &[Spec],
        env: Option<&'static str>,
        folder: Option<&'static str>,
    ) -> TrueNote {
        self.next_note += 1;
        TrueNote {
            key: self.next_note,
            service: service.to_string(),
            env,
            folder,
            fields: spec
                .iter()
                .map(|(label, kind, secret)| TrueField {
                    label: label.to_string(),
                    value: self.value(kind),
                    secret: *secret,
                })
                .collect(),
            id: 0,
        }
    }

    fn vault(&mut self, n: usize) -> Vec<TrueProject> {
        let mut names: Vec<(&str, bool)> = CLIENTS
            .iter()
            .map(|c| (*c, false))
            .chain(SIDE.iter().map(|s| (*s, true)))
            .collect();
        self.r.shuffle(&mut names);
        let mut out = Vec::new();
        for (name, side) in names.into_iter().take(n) {
            let first = name.split([' ', '-']).next().unwrap().to_lowercase();
            let aliases = if side {
                vec![
                    name.to_string(),
                    format!("side project - {name}"),
                    format!("{name} (hobi)"),
                ]
            } else {
                vec![
                    name.to_string(),
                    name.to_lowercase(),
                    name.to_uppercase(),
                    first.clone(),
                    format!("{name} projesi"),
                    format!("{first} işleri"),
                ]
            };
            let mut project = TrueProject {
                name: name.to_string(),
                aliases,
                notes: Vec::new(),
            };
            let tier = self.r.below(10);
            let names: Vec<usize> = (0..SERVICES.len()).collect();
            if tier < 2 {
                let k = self.r.range(6, 8);
                for s in self.r.sample(&names, k) {
                    let (service, spec) = SERVICES[s];
                    let infra = category(service) == Some("Infra");
                    let envs: Vec<Option<&'static str>> = if infra {
                        let n = self.r.range(1, 2);
                        self.r
                            .sample(&["prod", "staging"], n)
                            .into_iter()
                            .map(Some)
                            .collect()
                    } else {
                        vec![None]
                    };
                    for env in envs {
                        let note = self.note(service, spec, env, category(service));
                        project.notes.push(note);
                    }
                }
            } else {
                let k = if tier < 7 {
                    self.r.range(3, 5)
                } else {
                    self.r.range(1, 2)
                };
                for s in self.r.sample(&names, k) {
                    let (service, spec) = SERVICES[s];
                    let note = self.note(service, spec, None, None);
                    project.notes.push(note);
                }
            }
            out.push(project);
        }
        let mut personal = TrueProject {
            name: "Personal".to_string(),
            aliases: vec!["kişisel".into(), "benim hesaplar".into(), "Personal".into()],
            notes: Vec::new(),
        };
        let all: Vec<usize> = (0..PERSONAL.len()).collect();
        for p in self.r.sample(&all, 2 + n / 6) {
            let (service, spec) = PERSONAL[p];
            let note = self.note(service, spec, None, None);
            personal.notes.push(note);
        }
        out.push(personal);
        out
    }
}

// --- dumps -----------------------------------------------------------------------------

fn title_text(r: &mut Rng, note: &TrueNote) -> String {
    let service = if r.chance(0.5) {
        note.service.clone()
    } else {
        note.service.to_lowercase()
    };
    match note.env {
        None => service,
        Some(env) => {
            let env = if r.chance(0.5) {
                env
            } else if env == "prod" {
                "canlı"
            } else {
                "test"
            };
            format!("{service} {env}")
        }
    }
}

fn render_note(r: &mut Rng, note: &TrueNote) -> Vec<String> {
    let tr = r.chance(0.5);
    let title = title_text(r, note);
    let pairs: Vec<(String, String)> = note
        .fields
        .iter()
        .map(|f| {
            let label = if tr { tr_label(&f.label) } else { &f.label };
            (label.to_string(), f.value.clone())
        })
        .collect();
    match r.below(5) {
        0 => vec![format!(
            "{title} -> {}",
            pairs
                .iter()
                .map(|(l, v)| format!("{l} {v}"))
                .collect::<Vec<_>>()
                .join("  ")
        )],
        1 => {
            let prefix: String = note
                .service
                .to_uppercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            let env = note
                .env
                .map(|e| format!("_{}", e.to_uppercase()))
                .unwrap_or_default();
            note.fields
                .iter()
                .map(|f| {
                    let label: String = f
                        .label
                        .to_uppercase()
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '_' })
                        .collect();
                    format!("{prefix}{env}_{label}={}", f.value)
                })
                .collect()
        }
        2 if pairs.len() >= 2 => {
            let (last, head) = pairs.split_last().unwrap();
            vec![format!(
                "{title}: {} / {}",
                head.iter()
                    .map(|(_, v)| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                last.1
            )]
        }
        3 => std::iter::once("| servis | alan | değer |".to_string())
            .chain(
                pairs
                    .iter()
                    .map(|(l, v)| format!("| {title} | {l} | {v} |")),
            )
            .collect(),
        _ => std::iter::once(title)
            .chain(pairs.iter().map(|(l, v)| format!("  {l}: {v}")))
            .collect(),
    }
}

const NOISE: [&str; 6] = [
    "TODO: {svc} şifresini değiştir",
    "müşteri whatsapp'tan gönderdi",
    "bu eski olabilir kontrol et",
    "---",
    "2FA telefonda",
    "staging bazen çöküyor, restart at",
];
const HEADERS: [&str; 5] = ["## {a}", "{a}", "[{a}]", "{a}:", "=== {a} ==="];

fn render_dump(r: &mut Rng, vault: &[TrueProject]) -> String {
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut floating: Vec<Vec<String>> = Vec::new();
    for p in vault {
        let mut notes = p.notes.clone();
        r.shuffle(&mut notes);
        if p.name == "Personal" {
            let half = notes.len() / 2;
            for n in notes.drain(..half) {
                floating.push(render_note(r, &n));
            }
        }
        let parts = if notes.len() >= 6 { 2 } else { 1 };
        for part in 0..parts {
            let alias = r.pick(&p.aliases).clone();
            let mut lines = vec![r.pick(&HEADERS).replace("{a}", &alias)];
            for n in notes.iter().skip(part).step_by(parts) {
                lines.extend(render_note(r, n));
                if r.chance(0.25) {
                    lines.push(r.pick(&NOISE).replace("{svc}", &n.service.to_lowercase()));
                }
            }
            blocks.push(lines);
        }
    }
    r.shuffle(&mut blocks);
    for f in floating {
        let at = r.below(blocks.len() + 1);
        blocks.insert(at, f);
    }
    blocks
        .iter()
        .map(|b| b.join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

// --- seeding and reading back ----------------------------------------------------------

fn key() -> VaultKey {
    Zeroizing::new(vec![42; 32])
}

fn label_case(label: &str) -> String {
    cap(label)
}

/// Stores the vault; fields that may be either become secret.
fn seed(conn: &Connection, vault: &mut [TrueProject]) {
    for p in vault.iter_mut() {
        let project = projects::insert(conn, &p.name, "", 0).unwrap();
        let mut folder_ids: HashMap<&str, i64> = HashMap::new();
        for f in p.folders() {
            folder_ids.insert(f, folders::insert(conn, project, None, f, 0).unwrap());
        }
        for note in &mut p.notes {
            let body = note
                .fields
                .iter()
                .map(|f| fields::line(&label_case(&f.label), &f.value, f.secret != Some(false)))
                .collect::<Vec<_>>()
                .join("\n");
            let body = match seal_inline_with(&key(), &body).unwrap() {
                Some((template, secrets)) => Body::Inline { template, secrets },
                None => Body::Plain(body),
            };
            let tags: Vec<String> = note.env.map(|e| format!("env/{e}")).into_iter().collect();
            note.id = items::insert(
                conn,
                &ItemRecord {
                    project_id: project,
                    folder_id: note.folder.map(|f| folder_ids[f]),
                    title: &note.title(),
                    body: &body,
                    tags: &tags,
                },
                0,
            )
            .unwrap();
        }
    }
}

/// A stored field: where it is and whether it is secret.
#[derive(Clone, Debug)]
struct Stored {
    project: String,
    note: i64,
    secret: bool,
}

fn stored_fields(conn: &Connection) -> HashMap<String, Vec<Stored>> {
    let names: HashMap<i64, String> = projects::list(conn)
        .unwrap()
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let mut out: HashMap<String, Vec<Stored>> = HashMap::new();
    for item in items::list(conn).unwrap() {
        let text = if item.has_secrets || item.secure {
            let e = items::encrypted(conn, item.id).unwrap();
            open_with(&key(), e.template.as_deref(), &e.sealed)
                .unwrap()
                .to_string()
        } else {
            items::get(conn, item.id).unwrap().body.unwrap_or_default()
        };
        for line in text.lines() {
            if let Some(value) = fields::value_of(line) {
                out.entry(value.to_string()).or_default().push(Stored {
                    project: names[&item.project_id].clone(),
                    note: item.id,
                    secret: fields::is_secret(line),
                });
            }
        }
    }
    out
}

// --- running ---------------------------------------------------------------------------

struct Outcome {
    saved: Vec<Turn>,
    ops: Vec<draft::Op>,
    seconds: f64,
    error: Option<String>,
}

/// Writes every model reply to the trace file, so a bad score can be read back.
struct Traced<'a> {
    model: &'a Ollama,
    trace: &'a Mutex<std::fs::File>,
}

impl Model for Traced<'_> {
    fn chat(&self, request: &Request, on_text: &mut dyn FnMut(&str)) -> AppResult<Reply> {
        let started = Instant::now();
        let reply = self.model.chat(request, on_text);
        if let Ok(mut file) = self.trace.lock() {
            let _ = match &reply {
                Ok(r) => writeln!(
                    file,
                    "--- {:.0}s text: {}\ncalls: {}",
                    started.elapsed().as_secs_f64(),
                    r.text,
                    serde_json::to_string(&r.tool_calls).unwrap_or_default()
                ),
                Err(e) => writeln!(file, "--- error: {e}"),
            };
        }
        reply
    }
}

fn run(model: &Traced, db: &Mutex<Connection>, history: Vec<Turn>, apply: bool) -> Outcome {
    let started = Instant::now();
    let chat = system_prompt("Friday, 23 October 2026", "");
    let import = import_prompt("");
    let saved = std::cell::RefCell::new(Vec::new());
    let lock = || db.lock().unwrap();
    let mut on_text = |_: &str| {};
    let mut save = |turn: &Turn| {
        saved.borrow_mut().push(turn.clone());
        Ok(())
    };
    let result = reply::reply(
        model,
        &Prompts {
            chat: &chat,
            import: &import,
        },
        &history,
        Io {
            db: &lock,
            key: Some(key()),
            today: TODAY,
            on_text: &mut on_text,
            save: &mut save,
            cancelled: &|| false,
            check: &|| Ok(()),
        },
    );
    let saved = saved.take();
    let ops = saved
        .iter()
        .find_map(|t| match t {
            Turn::Draft { ops, .. } => Some(ops.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut error = result.err().map(|e| e.to_string());
    if apply && !ops.is_empty() {
        let (inputs, _) = reply::take_inputs(&history);
        if let Err(e) = draft::apply(&lock(), &key(), &inputs, &ops, 1) {
            error = Some(format!("apply: {e}"));
        }
    }
    Outcome {
        saved,
        ops,
        seconds: started.elapsed().as_secs_f64(),
        error,
    }
}

fn pct(a: usize, b: usize) -> String {
    if b == 0 {
        "0/0".to_string()
    } else {
        format!("{a}/{b} ({:.0}%)", 100.0 * a as f64 / b as f64)
    }
}

fn tool_errors(saved: &[Turn]) -> usize {
    saved
        .iter()
        .filter(|t| matches!(t, Turn::Tool { result, .. } if result.contains("Error:")))
        .count()
}

/// Imports a dump of `n` projects into an empty vault.
fn import_task(model: &Traced, n: usize, seed_value: u64) -> Value {
    let mut g = Gen {
        r: Rng(seed_value),
        used: HashSet::new(),
        next_note: 0,
    };
    let vault = g.vault(n);
    let dump = render_dump(&mut g.r, &vault);
    let db = Mutex::new(open_in_memory());
    let out = run(model, &db, vec![paste(&dump)], true);
    let stored = stored_fields(&db.lock().unwrap());

    // Truth: value -> (project, note key, secret).
    let truth: Vec<(&TrueProject, &TrueNote, &TrueField)> = vault
        .iter()
        .flat_map(|p| {
            p.notes
                .iter()
                .flat_map(move |n| n.fields.iter().map(move |f| (p, n, f)))
        })
        .collect();
    let found: Vec<_> = truth
        .iter()
        .filter_map(|(p, n, f)| stored.get(&f.value).map(|s| (p, n, f, &s[0])))
        .collect();
    let leaks = found
        .iter()
        .filter(|(_, _, f, s)| f.secret == Some(true) && !s.secret)
        .count();
    let over = found
        .iter()
        .filter(|(_, _, f, s)| f.secret == Some(false) && s.secret)
        .count();
    let judged = found
        .iter()
        .filter(|(_, _, f, _)| f.secret.is_some())
        .count();

    // One-to-one project assignment, greedy by size.
    let mut spread: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (p, _, _, s) in &found {
        *spread
            .entry((p.name.clone(), s.project.clone()))
            .or_default() += 1;
    }
    let mut pairs: Vec<(usize, String, String)> = spread
        .iter()
        .map(|((t, m), k)| (*k, t.clone(), m.clone()))
        .collect();
    pairs.sort_by(|a, b| b.cmp(a));
    let (mut taken_true, mut taken_model, mut right) = (HashSet::new(), HashSet::new(), 0);
    for (k, t, m) in pairs {
        if !taken_true.contains(&t) && !taken_model.contains(&m) {
            taken_true.insert(t);
            taken_model.insert(m);
            right += k;
        }
    }
    let model_projects: HashSet<&str> = found
        .iter()
        .map(|(_, _, _, s)| s.project.as_str())
        .collect();

    // Grouping: pairs of values that share a note should share one, and only those.
    let (mut tp, mut fp, mut fn_) = (0usize, 0usize, 0usize);
    for (i, a) in found.iter().enumerate() {
        for b in &found[i + 1..] {
            let same_true = a.1.key == b.1.key;
            let same_model = a.3.note == b.3.note;
            tp += usize::from(same_true && same_model);
            fp += usize::from(same_model && !same_true);
            fn_ += usize::from(same_true && !same_model);
        }
    }
    let precision = if tp + fp == 0 {
        1.0
    } else {
        tp as f64 / (tp + fp) as f64
    };
    let recall = if tp + fn_ == 0 {
        1.0
    } else {
        tp as f64 / (tp + fn_) as f64
    };
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let notes_true: usize = vault.iter().map(|p| p.notes.len()).sum();
    let notes_model = items::list(&db.lock().unwrap()).unwrap().len();
    let not_in_dump = stored.keys().filter(|v| !dump.contains(v.as_str())).count();
    json!({
        "seconds": out.seconds.round(),
        "error": out.error,
        "tool_errors": tool_errors(&out.saved),
        "values_captured": pct(found.len(), truth.len()),
        "values_not_in_dump": not_in_dump,
        "secret_flags_ok": pct(judged - leaks - over, judged),
        "secrets_stored_plain": leaks,
        "plain_stored_secret": over,
        "project_assignment": pct(right, found.len()),
        "projects_true_vs_model": format!("{} vs {}", vault.len(), model_projects.len()),
        "grouping_f1": (f1 * 1000.0).round() / 1000.0,
        "notes_true_vs_model": format!("{notes_true} vs {notes_model}"),
    })
}

fn paste(text: &str) -> Turn {
    Turn::User {
        text: String::new(),
        paste: Some(text.to_string()),
    }
}

/// A new dump against a full vault: new notes, new fields, changed values, a new project and
/// values stored already.
fn add_task(model: &Traced) -> Value {
    let mut g = Gen {
        r: Rng(0x05ee_dadd),
        used: HashSet::new(),
        next_note: 0,
    };
    let mut vault = g.vault(12);
    let db = Mutex::new(open_in_memory());
    seed(&db.lock().unwrap(), &mut vault);
    let clients: Vec<TrueProject> = vault
        .iter()
        .filter(|p| p.name != "Personal")
        .cloned()
        .collect();
    let alias = |r: &mut Rng, p: &TrueProject| r.pick(&p.aliases).clone();

    // (line, [(value, expectation)])
    let mut lines: Vec<(String, Vec<(String, Value)>)> = Vec::new();
    for p in g.r.sample(&clients, 6) {
        let have: HashSet<&str> = p.notes.iter().map(|n| n.service.as_str()).collect();
        let options: Vec<usize> = (0..SERVICES.len())
            .filter(|&s| !have.contains(SERVICES[s].0))
            .collect();
        let (service, spec) = SERVICES[*g.r.pick(&options)];
        let note = g.note(service, spec, None, None);
        let a = alias(&mut g.r, &p);
        let title = title_text(&mut g.r, &note);
        let text = format!(
            "{a} için yeni {title}: {}",
            note.fields
                .iter()
                .map(|f| format!("{} {}", tr_label(&f.label), f.value))
                .collect::<Vec<_>>()
                .join("  ")
        );
        let expect = note
            .fields
            .iter()
            .map(|f| {
                (
                    f.value.clone(),
                    json!({ "type": "new_note", "project": p.name, "secret": f.secret }),
                )
            })
            .collect();
        lines.push((text, expect));
    }
    let logins: Vec<(TrueProject, TrueNote)> = clients
        .iter()
        .flat_map(|p| {
            p.notes
                .iter()
                .filter(|n| n.fields.iter().any(|f| f.label == "password"))
                .map(move |n| (p.clone(), n.clone()))
        })
        .collect();
    for (p, n) in g.r.sample(&logins, 4) {
        let v = g.value("backup");
        let a = alias(&mut g.r, &p);
        let title = title_text(&mut g.r, &n);
        lines.push((
            format!("{a} {title} 2FA yedek kodları: {v}"),
            vec![(
                v,
                json!({ "type": "add_field", "note": n.id, "secret": true }),
            )],
        ));
    }
    let secrets: Vec<(TrueProject, TrueNote, TrueField)> = clients
        .iter()
        .flat_map(|p| {
            p.notes.iter().flat_map(move |n| {
                n.fields
                    .iter()
                    .filter(|f| f.secret == Some(true))
                    .map(move |f| (p.clone(), n.clone(), f.clone()))
            })
        })
        .collect();
    for (p, n, f) in g.r.sample(&secrets, 4) {
        let kind = SERVICES
            .iter()
            .find(|(s, _)| *s == n.service)
            .and_then(|(_, spec)| spec.iter().find(|(l, _, _)| *l == f.label))
            .map(|(_, k, _)| *k)
            .unwrap();
        let v = g.value(kind);
        let a = alias(&mut g.r, &p);
        let title = title_text(&mut g.r, &n);
        lines.push((
            format!("{a} {title} {} değişti, yenisi: {v}", tr_label(&f.label)),
            vec![(
                v,
                json!({ "type": "update", "note": n.id, "old": f.value, "secret": true }),
            )],
        ));
    }
    let new_name = "Zumrut Veteriner";
    let mut block = vec![format!("## {new_name} (yeni müşteri)")];
    let mut values = Vec::new();
    let all: Vec<usize> = (0..SERVICES.len()).collect();
    for s in g.r.sample(&all, 2) {
        let (service, spec) = SERVICES[s];
        let note = g.note(service, spec, None, None);
        block.push(format!(
            "{}: {}",
            title_text(&mut g.r, &note),
            note.fields
                .iter()
                .map(|f| format!("{} {}", f.label, f.value))
                .collect::<Vec<_>>()
                .join("  ")
        ));
        values.extend(note.fields.iter().map(|f| {
            (
                f.value.clone(),
                json!({ "type": "new_project", "secret": f.secret }),
            )
        }));
    }
    lines.push((block.join("\n"), values));
    let plain: Vec<(TrueProject, TrueNote, TrueField)> = clients
        .iter()
        .flat_map(|p| {
            p.notes.iter().flat_map(move |n| {
                n.fields
                    .iter()
                    .filter(|f| f.secret == Some(false))
                    .map(move |f| (p.clone(), n.clone(), f.clone()))
            })
        })
        .collect();
    for (p, n, f) in g.r.sample(&plain, 3) {
        let a = alias(&mut g.r, &p);
        let title = title_text(&mut g.r, &n);
        lines.push((
            format!("{a} {title} {}: {}", f.label, f.value),
            vec![(f.value.clone(), json!({ "type": "skip" }))],
        ));
    }
    g.r.shuffle(&mut lines);
    let dump = lines
        .iter()
        .map(|(l, _)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let out = run(model, &db, vec![paste(&dump)], true);
    let stored = stored_fields(&db.lock().unwrap());
    let mut totals: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut leaks = 0;
    let mut new_project_names = HashSet::new();
    for (_, values) in &lines {
        for (v, e) in values {
            let kind = e["type"].as_str().unwrap();
            let at = stored.get(v);
            let entry = totals.entry(kind).or_default();
            entry.1 += 1;
            if let (Some(at), Some(true)) = (at, e["secret"].as_bool())
                && !at[0].secret
            {
                leaks += 1;
            }
            let ok = match kind {
                "new_note" => at.is_some_and(|s| s[0].project == e["project"]),
                "add_field" => at.is_some_and(|s| s[0].note == e["note"].as_i64().unwrap()),
                "update" => {
                    at.is_some_and(|s| s[0].note == e["note"].as_i64().unwrap())
                        && !stored.contains_key(e["old"].as_str().unwrap())
                }
                "new_project" => at.is_some_and(|s| {
                    new_project_names.insert(s[0].project.clone());
                    !vault.iter().any(|p| p.name == s[0].project)
                }),
                "skip" => at.is_some_and(|s| s.len() == 1),
                _ => false,
            };
            entry.0 += usize::from(ok);
        }
    }
    let mut result = json!({
        "seconds": out.seconds.round(),
        "error": out.error,
        "tool_errors": tool_errors(&out.saved),
        "secrets_stored_plain": leaks,
        "new_project_split_into": new_project_names.len(),
    });
    let names = [
        ("new_note", "new_note_right_project"),
        ("add_field", "field_added_to_right_note"),
        ("update", "update_replaced_right_value"),
        ("new_project", "new_project_values"),
        ("skip", "duplicates_skipped"),
    ];
    for (kind, name) in names {
        let (ok, all) = totals.get(kind).copied().unwrap_or_default();
        result[name] = json!(pct(ok, all));
    }
    result
}

/// Natural-language commands, one conversation each, scored on the draft they produce.
fn edit_task(model: &Traced) -> Value {
    let mut g = Gen {
        r: Rng(0xed17),
        used: HashSet::new(),
        next_note: 0,
    };
    let mut vault = g.vault(10);
    let base = open_in_memory();
    seed(&base, &mut vault);
    let clients: Vec<TrueProject> = vault
        .iter()
        .filter(|p| p.name != "Personal")
        .cloned()
        .collect();
    // A todo and a payment to act on.
    let todo_project = projects::list(&base).unwrap().remove(0);
    let todo = todos::insert(
        &base,
        &TodoRecord {
            project_id: Some(todo_project.id),
            item_id: None,
            title: "Renew the SSL certificate",
            description: "",
            secrets: None,
            due_date: Some("2026-10-30"),
        },
        0,
    )
    .unwrap();
    payments::insert(
        &base,
        &PaymentRecord {
            project_id: Some(todo_project.id),
            name: "Domain",
            currency: "USD",
            cycle: "yearly",
            start_date: "2025-03-01",
            cancel_date: None,
        },
        1200,
        0,
    )
    .unwrap();
    let base_dump = crate::db::vault::dump(&base).unwrap();

    let reference = |r: &mut Rng, p: &TrueProject, n: &TrueNote| {
        let same = p.notes.iter().filter(|m| m.service == n.service).count();
        let t = if same > 1 {
            title_text(r, n)
        } else {
            n.service.to_lowercase()
        };
        format!("{} {t}", r.pick(&p.aliases))
    };
    let notes: Vec<(TrueProject, TrueNote)> = clients
        .iter()
        .flat_map(|p| p.notes.iter().map(move |n| (p.clone(), n.clone())))
        .collect();
    // (command, expectation)
    let mut commands: Vec<(String, Value)> = Vec::new();
    let with_secret: Vec<_> = notes
        .iter()
        .filter(|(_, n)| n.fields.iter().any(|f| f.secret == Some(true)))
        .cloned()
        .collect();
    for (p, n) in g.r.sample(&with_secret, 3) {
        let f = n
            .fields
            .iter()
            .find(|f| f.secret == Some(true))
            .unwrap()
            .clone();
        let kind = SERVICES
            .iter()
            .find(|(s, _)| *s == n.service)
            .and_then(|(_, spec)| spec.iter().find(|(l, _, _)| *l == f.label))
            .map(|(_, k, _)| *k)
            .unwrap();
        let v = g.value(kind);
        let r = reference(&mut g.r, &p, &n);
        commands.push((
            format!("{r} notundaki {} artık {v} olsun", tr_label(&f.label)),
            json!({ "op": "updateField", "note": n.id, "label": label_case(&f.label), "value": v }),
        ));
    }
    let multi: Vec<_> = notes
        .iter()
        .filter(|(_, n)| n.fields.len() >= 2)
        .cloned()
        .collect();
    for (p, n) in g.r.sample(&multi, 2) {
        let f = g.r.pick(&n.fields).clone();
        let r = reference(&mut g.r, &p, &n);
        commands.push((
            format!(
                "{r} içindeki {} alanını sil, artık kullanmıyoruz",
                tr_label(&f.label)
            ),
            json!({ "op": "removeField", "note": n.id, "label": label_case(&f.label) }),
        ));
    }
    let movable: Vec<_> = notes
        .iter()
        .filter(|(p, _)| p.folders().len() >= 2)
        .cloned()
        .collect();
    if !movable.is_empty() {
        for (p, n) in g.r.sample(&movable, 2) {
            let options: Vec<&str> = p
                .folders()
                .into_iter()
                .filter(|f| Some(*f) != n.folder)
                .collect();
            let target = *g.r.pick(&options);
            let r = reference(&mut g.r, &p, &n);
            commands.push((
                format!("move the {r} note into {target}"),
                json!({ "op": "moveNote", "note": n.id, "folder": target }),
            ));
        }
    }
    let (p, n) = g.r.pick(&notes).clone();
    let title = format!("{} (old account)", n.service);
    let r = reference(&mut g.r, &p, &n);
    commands.push((
        format!("rename the {r} note to '{title}'"),
        json!({ "op": "renameNote", "note": n.id, "title": title }),
    ));
    let (p, n) = g.r.pick(&notes).clone();
    let r = reference(&mut g.r, &p, &n);
    commands.push((
        format!("{r} notunu tamamen sil, o servisi kapattık"),
        json!({ "op": "deleteNote", "note": n.id }),
    ));
    commands.push((
        format!("mark the {} renew SSL todo done", todo_project.name),
        json!({ "op": "completeTodo", "todo": todo }),
    ));
    let p = g.r.pick(&clients).clone();
    commands.push((
        format!("add a monthly 300 TRY hosting payment to {}", p.name.to_lowercase()),
        json!({ "op": "createPayment", "project": p.name, "amount": 30000, "currency": "TRY", "cycle": "monthly" }),
    ));
    let p = g.r.pick(&clients).clone();
    let key = format!("sk_test_{}", g.r.chars(24, ALNUM));
    commands.push((
        format!("add a Stripe test key {key} to {}", g.r.pick(&p.aliases)),
        json!({ "op": "storeValue", "project": p.name, "value": key, "secret": true }),
    ));

    let mut right = 0;
    let mut details = Vec::new();
    let mut seconds = 0.0;
    let mut errors = 0;
    for (command, expect) in &commands {
        let db = Mutex::new(open_in_memory());
        crate::db::vault::replace(&db.lock().unwrap(), &base_dump).unwrap();
        let history = vec![Turn::user(command.clone())];
        let out = run(model, &db, history.clone(), true);
        seconds += out.seconds;
        errors += tool_errors(&out.saved);
        let ok = check_edit(&db.lock().unwrap(), &out.ops, expect, &history);
        right += usize::from(ok);
        let answer = out.saved.iter().rev().find_map(|t| match t {
            Turn::Assistant { text, .. } if !text.is_empty() => Some(text.clone()),
            _ => None,
        });
        details.push(json!({
            "command": command,
            "ok": ok,
            "expected": expect,
            "ops": serde_json::to_value(&out.ops).unwrap(),
            "error": out.error,
            "answer": answer,
        }));
    }
    json!({
        "seconds": seconds.round(),
        "tool_errors": errors,
        "commands_right": pct(right, commands.len()),
        "commands": details,
    })
}

/// Whether a command's draft did exactly what was asked (checked after applying it).
fn check_edit(conn: &Connection, ops: &[draft::Op], expect: &Value, history: &[Turn]) -> bool {
    use draft::Op;
    let (inputs, _) = reply::take_inputs(history);
    let value_of = |v: &draft::Val| match v {
        draft::Val::Input(n) => inputs.get(*n).map(str::to_string),
        draft::Val::Text(t) => Some(t.clone()),
        draft::Val::Secret(_) => None,
    };
    let note = expect["note"].as_i64().unwrap_or(-1);
    let label = expect["label"].as_str().unwrap_or("");
    match expect["op"].as_str().unwrap() {
        "updateField" => {
            ops.len() == 1
                && matches!(&ops[0], Op::UpdateField { note: n, label: l, value, .. }
                    if *n == note && fields::normalize(l) == fields::normalize(label)
                        && value_of(value).as_deref() == expect["value"].as_str())
        }
        "removeField" => {
            ops.len() == 1
                && matches!(&ops[0], Op::RemoveField { note: n, label: l }
                    if *n == note && fields::normalize(l) == fields::normalize(label))
        }
        "moveNote" => {
            ops.iter()
                .any(|op| matches!(op, Op::MoveNote { note: n, .. } if *n == note))
                && ops.iter().all(|op| matches!(op, Op::MoveNote { .. }))
                && items::summary(conn, note).is_ok_and(|s| {
                    folders::list(conn).unwrap().iter().any(|f| {
                        Some(f.id) == s.folder_id && f.name == expect["folder"].as_str().unwrap()
                    })
                })
        }
        "renameNote" => {
            ops.len() == 1
                && matches!(&ops[0], Op::RenameNote { note: n, title } if *n == note && title == expect["title"].as_str().unwrap())
        }
        "deleteNote" => {
            ops.len() == 1 && matches!(&ops[0], Op::DeleteNote { note: n } if *n == note)
        }
        "completeTodo" => {
            ops.len() == 1
                && matches!(&ops[0], Op::CompleteTodo { todo } if *todo == expect["todo"].as_i64().unwrap())
        }
        "createPayment" => {
            ops.len() == 1
                && payments::list(conn).unwrap().iter().any(|p| {
                    p.cycle == "monthly"
                        && p.currency == "TRY"
                        && p.prices.last().is_some_and(|x| x.amount == 30000)
                        && p.project_id.is_some_and(|id| {
                            projects::get(conn, id).is_ok_and(|x| x.name == expect["project"])
                        })
                })
        }
        "storeValue" => {
            let stored = stored_fields(conn);
            stored
                .get(expect["value"].as_str().unwrap())
                .is_some_and(|s| s.len() == 1 && s[0].project == expect["project"] && s[0].secret)
                && !ops.iter().any(|op| matches!(op, Op::CreateProject { .. }))
        }
        _ => false,
    }
}

#[test]
#[ignore = "runs a local model; see the module docs"]
fn eval() {
    let model_name = std::env::var("COFFER_EVAL_MODEL").unwrap_or_else(|_| "qwen3.8:27b".into());
    let url = std::env::var("COFFER_EVAL_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    let tasks =
        std::env::var("COFFER_EVAL_TASKS").unwrap_or_else(|_| "edit,import_S,add,import_M".into());
    let ollama = Ollama::new(&url, &model_name, None, Arc::new(AtomicBool::new(false))).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/eval");
    std::fs::create_dir_all(&dir).unwrap();
    let tag = model_name.replace([':', '/'], "_");
    let path = dir.join(format!("{tag}.json"));
    let trace = Mutex::new(std::fs::File::create(dir.join(format!("{tag}.trace.txt"))).unwrap());
    let model = Traced {
        model: &ollama,
        trace: &trace,
    };
    let mut report =
        serde_json::from_str::<Value>(&std::fs::read_to_string(&path).unwrap_or_default())
            .unwrap_or_else(|_| json!({}));
    for task in tasks.split(',').map(str::trim) {
        if let Ok(mut file) = trace.lock() {
            let _ = writeln!(file, "===== {task}");
        }
        let result = match task {
            "import_S" => import_task(&model, 5, 0x5eed_0005),
            "import_M" => import_task(&model, 12, 0x5eed_0012),
            "add" => add_task(&model),
            "edit" => edit_task(&model),
            other => panic!("unknown task {other}"),
        };
        println!(
            "=== {model_name} {task}\n{}",
            serde_json::to_string_pretty(&result).unwrap()
        );
        report[task] = result;
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}

#[test]
fn dumps_are_deterministic_and_hold_every_value() {
    let make = || {
        let mut g = Gen {
            r: Rng(0x5eed_0005),
            used: HashSet::new(),
            next_note: 0,
        };
        let vault = g.vault(5);
        let dump = render_dump(&mut g.r, &vault);
        (vault, dump)
    };
    let (vault, dump) = make();
    assert_eq!(dump, make().1);
    for p in &vault {
        for n in &p.notes {
            for f in &n.fields {
                assert!(dump.contains(&f.value), "{} missing", f.value);
            }
        }
    }
    let conn = open_in_memory();
    let mut vault = vault;
    seed(&conn, &mut vault);
    let stored = stored_fields(&conn);
    let secret = &vault[0].notes[0]
        .fields
        .iter()
        .find(|f| f.secret == Some(true));
    if let Some(f) = secret {
        assert!(stored[&f.value][0].secret);
    }
}
