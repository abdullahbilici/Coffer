//! Dev-only demo data. Runs after unlock when `COFFER_SEED=1` is set and the vault is empty.

use rusqlite::Connection;

use crate::{
    crypto::{self, VaultKey},
    db::{
        folders,
        items::{self, Body, ItemRecord, SecureBody},
        projects,
    },
    error::AppResult,
    secrets,
    time::now_millis,
};

struct SeedItem {
    folder: Option<&'static str>,
    title: &'static str,
    secure: bool,
    tags: &'static [&'static str],
    body: &'static str,
}

struct SeedProject {
    name: &'static str,
    readme: &'static str,
    /// (name, parent name)
    folders: &'static [(&'static str, Option<&'static str>)],
    items: &'static [SeedItem],
}

const PROJECTS: &[SeedProject] = &[
    SeedProject {
        name: "Acme Corp",
        readme: "# Acme Corp\n\nE-commerce redesign and backend migration for Acme.\n\n## Links\n\n- [Production](https://app.acme-demo.com)\n- [Staging](https://staging.acme-demo.com)\n- [Jira board](https://acme-demo.atlassian.net)\n- [Figma](https://figma.com/file/DEMO123/acme-redesign)\n\n## Contacts\n\n| Role | Name | Email |\n| --- | --- | --- |\n| PM | Mert Yılmaz | mert@acme-demo.com |\n| CTO | Elif Kaya | elif@acme-demo.com |\n\n## Notes\n\n- Deploys go through GitHub Actions, never by hand.\n- Live Stripe keys stay with the client.\n- Staging DB host is `db.staging.acme-demo.com`.\n",
        folders: &[
            ("Infrastructure", None),
            ("AWS", Some("Infrastructure")),
            ("Payments", None),
        ],
        items: &[
            SeedItem {
                folder: Some("AWS"),
                title: "AWS console",
                secure: false,
                tags: &["cloud/aws", "env/prod"],
                body: "URL:       `https://acme-demo.signin.aws.amazon.com/console`\nAccount:   `4821-9930-1174`\nUser:      `apo.deploy`\nPassword:  `!Tr0ub4dor&3-demo!`\nMFA:       Authy, entry \"acme-aws\"",
            },
            SeedItem {
                folder: Some("AWS"),
                title: "S3 deploy keys",
                secure: true,
                tags: &["cloud/aws", "env/staging"],
                body: "AWS_ACCESS_KEY_ID=`AKIADEMO1234567890`\nAWS_SECRET_ACCESS_KEY=`demo/xxxxxxxxxxxxxxxxxxxxxxxx`\nBucket: `acme-staging-assets`\nRegion: `eu-central-1`",
            },
            SeedItem {
                folder: Some("Infrastructure"),
                title: ".env (staging)",
                secure: true,
                tags: &["env/staging"],
                body: "DATABASE_URL=postgres://acme:s3cr3t-demo@db.staging.acme-demo.com:5432/acme\nREDIS_URL=redis://:demo-pass@cache.staging.acme-demo.com:6379\nJWT_SECRET=demo-7f3a9c1e5b2d8f4a",
            },
            SeedItem {
                folder: Some("Infrastructure"),
                title: "Staging access",
                secure: false,
                tags: &["env/staging"],
                body: "URL:         `https://staging.acme-demo.com`\nBasic auth:  `acme` / `!stg-demo-4471!`\nVPN profile: `acme-staging.ovpn`\nVPN code:    `!vpn-demo-9932!`\n\nAsk Mert for a new VPN profile.",
            },
            SeedItem {
                folder: Some("Payments"),
                title: "Stripe test keys",
                secure: false,
                tags: &["billing/stripe", "env/staging"],
                body: "Publishable: `pk_test_51DEMOxxxxxxxxxxxxxxxxxxxx`\nSecret:      `!sk_test_51DEMOxxxxxxxxxxxxxxxxxxxx!`\nWebhook:     `!whsec_DEMOxxxxxxxxxxxxxxxx!`\nDashboard:   `https://dashboard.stripe.com/test`",
            },
            SeedItem {
                folder: None,
                title: "Office Wi-Fi",
                secure: false,
                tags: &["personal"],
                body: "SSID:     `AcmeOffice-5G`\nPassword: `!welcome-to-acme-demo!`\nGuest:    `AcmeGuest` / `acme2026`",
            },
        ],
    },
    SeedProject {
        name: "Muhit",
        readme: "# Muhit\n\nNeighbourhood app. Self-hosted on Hetzner.\n\n## Deploy\n\n```sh\nssh deploy@203.0.113.42\ncd /srv/muhit && ./deploy.sh\n```\n\n## Renewals\n\n- Hetzner CX22: monthly on the 5th, €4.51\n- Domain `muhit-demo.com`: 2027-03-14\n",
        folders: &[("Servers", None), ("Accounts", None)],
        items: &[
            SeedItem {
                folder: Some("Accounts"),
                title: "Hetzner login",
                secure: false,
                tags: &["cloud/hetzner"],
                body: "URL:      `https://accounts.hetzner.com`\nEmail:    `ops@muhit-demo.com`\nPassword: `!correct-horse-battery-demo!`",
            },
            SeedItem {
                folder: Some("Servers"),
                title: "Production SSH",
                secure: false,
                tags: &["infra/ssh", "env/prod"],
                body: "Host:    `203.0.113.42`\nUser:    `deploy`\nConnect: `ssh deploy@203.0.113.42`\nKey:     `~/.ssh/muhit_demo_ed25519`\nSudo:    `!demo-sudo-9921!`",
            },
            SeedItem {
                folder: Some("Servers"),
                title: "DNS setup",
                secure: false,
                tags: &["infra/dns"],
                body: "Cloudflare free plan.\n\nA      @    `203.0.113.42`\nCNAME  www  `muhit-demo.com`\nAPI token: `!cf-demo-token-8812!`",
            },
        ],
    },
    SeedProject {
        name: "Personal",
        readme: "# Personal\n\nKeys and accounts that don't belong to a client.\n",
        folders: &[("AI", None)],
        items: &[
            SeedItem {
                folder: Some("AI"),
                title: "Anthropic API key",
                secure: false,
                tags: &["ai/anthropic"],
                body: "Key:         `!sk-ant-api03-DEMOxxxxxxxxxxxxxxxx!`\nConsole:     `https://console.anthropic.com`\nMonthly cap: $50",
            },
            SeedItem {
                folder: Some("AI"),
                title: "OpenAI API key",
                secure: true,
                tags: &["ai/openai"],
                body: "Key: `sk-proj-DEMOxxxxxxxxxxxxxxxx`\nOrg: `org-DEMO1234`",
            },
            SeedItem {
                folder: None,
                title: "Subscriptions",
                secure: false,
                tags: &["billing"],
                body: "Figma Pro:       yearly, 2027-01-12, $144\nApple Developer: yearly, 2027-06-02, $99\nSupport email:   `billing@apo-demo.dev`",
            },
        ],
    },
];

pub fn run_if_requested(conn: &Connection, key: &VaultKey) -> AppResult<()> {
    if std::env::var("COFFER_SEED").as_deref() != Ok("1") || !projects::list(conn)?.is_empty() {
        return Ok(());
    }
    let now = now_millis();
    for project in PROJECTS {
        let project_id = projects::insert(conn, project.name, project.readme, now)?;

        let mut folder_ids: Vec<(&str, i64)> = Vec::new();
        for (name, parent) in project.folders {
            let parent_id = parent.map(|p| lookup(&folder_ids, p));
            let id = folders::insert(conn, project_id, parent_id, name, now)?;
            folder_ids.push((name, id));
        }

        for item in project.items {
            let seal = |plaintext: &str| -> AppResult<SecureBody> {
                let (nonce, ciphertext) = crypto::encrypt(key, plaintext)?;
                Ok(SecureBody { nonce, ciphertext })
            };
            let (template, spans) = secrets::extract(item.body);
            let body = if item.secure {
                Body::Secure(seal(item.body)?)
            } else if spans.is_empty() {
                Body::Plain(item.body.to_string())
            } else {
                let json =
                    serde_json::to_string(&spans).expect("a list of strings always serializes");
                Body::Inline {
                    template,
                    secrets: seal(&json)?,
                }
            };
            let tags: Vec<String> = item.tags.iter().map(|t| t.to_string()).collect();
            let record = ItemRecord {
                project_id,
                folder_id: item.folder.map(|f| lookup(&folder_ids, f)),
                title: item.title,
                body: &body,
                tags: &tags,
            };
            items::insert(conn, &record, now)?;
        }
    }
    Ok(())
}

fn lookup(folder_ids: &[(&str, i64)], name: &str) -> i64 {
    folder_ids
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, id)| *id)
        .unwrap_or_else(|| panic!("seed folder {name} is defined after its use"))
}
