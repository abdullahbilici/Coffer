//! Dev-only demo data. Runs after unlock when `COFFER_SEED=1` is set and the vault is empty
//! (`npm run reseed` backs up the current vault and starts one). All values are fake.

use rusqlite::Connection;

use crate::{
    crypto::{self, VaultKey},
    db::{
        SecureBody, folders,
        items::{self, Body, ItemRecord},
        payments::{self, PaymentRecord},
        projects,
        todos::{self, TodoRecord},
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
    /// (name, parent name). Parents come first.
    folders: &'static [(&'static str, Option<&'static str>)],
    items: &'static [SeedItem],
}

/// A note with plain text, copyable `` `chips` `` and inline `` `!secrets!` ``.
const fn note(
    folder: Option<&'static str>,
    title: &'static str,
    tags: &'static [&'static str],
    body: &'static str,
) -> SeedItem {
    SeedItem {
        folder,
        title,
        secure: false,
        tags,
        body,
    }
}

/// A note whose whole body is encrypted.
const fn secure(
    folder: Option<&'static str>,
    title: &'static str,
    tags: &'static [&'static str],
    body: &'static str,
) -> SeedItem {
    SeedItem {
        folder,
        title,
        secure: true,
        tags,
        body,
    }
}

const PROJECTS: &[SeedProject] = &[
    SeedProject {
        name: "Acme Corp",
        readme: "# Acme Corp\n\nE-commerce redesign and backend migration for Acme.\n\n## Links\n\n- [Production](https://app.acme-demo.com)\n- [Staging](https://staging.acme-demo.com)\n- [Jira board](https://acme-demo.atlassian.net)\n- [Figma](https://figma.com/file/DEMO123/acme-redesign)\n\n## Contacts\n\n| Role | Name | Email |\n| --- | --- | --- |\n| PM | Mert Yılmaz | `mert@acme-demo.com` |\n| CTO | Elif Kaya | `elif@acme-demo.com` |\n\n## Access\n\n- Office door code: `!4471#!`\n- Shared 1Password vault invite: `!acme-vault-demo-88!`\n- Staging DB host is `db.staging.acme-demo.com`.\n\n## Notes\n\n- Deploys go through GitHub Actions, never by hand.\n- Live Stripe keys stay with the client.\n",
        folders: &[
            ("Infrastructure", None),
            ("AWS", Some("Infrastructure")),
            ("Databases", Some("Infrastructure")),
            ("Payments", None),
            ("Third-party", None),
            ("Analytics", Some("Third-party")),
        ],
        items: &[
            note(
                Some("AWS"),
                "AWS console",
                &["cloud/aws", "env/prod"],
                "URL:       `https://acme-demo.signin.aws.amazon.com/console`\nAccount:   `4821-9930-1174`\nUser:      `apo.deploy`\nPassword:  `!Tr0ub4dor&3-demo!`\nMFA:       Authy, entry \"acme-aws\"",
            ),
            secure(
                Some("AWS"),
                "S3 deploy keys",
                &["cloud/aws", "env/staging"],
                "AWS_ACCESS_KEY_ID=`AKIADEMO1234567890`\nAWS_SECRET_ACCESS_KEY=`demo/xxxxxxxxxxxxxxxxxxxxxxxx`\nBucket: `acme-staging-assets`\nRegion: `eu-central-1`",
            ),
            note(
                Some("AWS"),
                "CloudFront distribution",
                &["cloud/aws", "env/prod"],
                "ID:     `E2DEMO7QK3XJ1A`\nDomain: `d1demo4x9.cloudfront.net`\nInvalidate: `aws cloudfront create-invalidation --distribution-id E2DEMO7QK3XJ1A --paths \"/*\"`",
            ),
            note(
                Some("Databases"),
                "Postgres (prod)",
                &["infra/db", "env/prod"],
                "Host:     `db.acme-demo.com`\nPort:     `5432`\nDatabase: `acme`\nUser:     `acme_app`\nPassword: `!pg-prod-demo-9f2c!`\nRead replica: `db-ro.acme-demo.com`",
            ),
            note(
                Some("Databases"),
                "Postgres (staging)",
                &["infra/db", "env/staging"],
                "Host:     `db.staging.acme-demo.com`\nUser:     `acme`\nPassword: `!s3cr3t-demo!`\nConnect:  `psql postgres://acme@db.staging.acme-demo.com:5432/acme`",
            ),
            note(
                Some("Databases"),
                "Redis",
                &["infra/cache", "env/staging"],
                "URL:      `redis://cache.staging.acme-demo.com:6379`\nPassword: `!demo-pass!`",
            ),
            secure(
                Some("Infrastructure"),
                ".env (staging)",
                &["env/staging"],
                "DATABASE_URL=postgres://acme:s3cr3t-demo@db.staging.acme-demo.com:5432/acme\nREDIS_URL=redis://:demo-pass@cache.staging.acme-demo.com:6379\nJWT_SECRET=demo-7f3a9c1e5b2d8f4a\nSENTRY_DSN=`https://demo@o0.ingest.sentry.io/1`",
            ),
            note(
                Some("Infrastructure"),
                "Staging access",
                &["env/staging"],
                "URL:         `https://staging.acme-demo.com`\nBasic auth:  `acme` / `!stg-demo-4471!`\nVPN profile: `acme-staging.ovpn`\nVPN code:    `!vpn-demo-9932!`\n\nAsk Mert for a new VPN profile.",
            ),
            note(
                Some("Infrastructure"),
                "GitHub deploy",
                &["ci/github", "env/prod"],
                "Repo:     `github.com/acme-demo/storefront`\nWorkflow: `.github/workflows/deploy.yml`\nDeploy token: `!ghp_DEMOxxxxxxxxxxxxxxxxxxxxxxxx!`\nEnvironments need a reviewer from the Acme side.",
            ),
            note(
                Some("Infrastructure"),
                "Deploy runbook",
                &["runbook", "env/prod"],
                "# Deploy runbook\n\nProduction deploys run from GitHub Actions. Do this by hand **only** if CI is down.\n\n## Steps\n\n1. Announce in `#acme-deploys`\n2. Tag the release: `git tag v$(date +%Y.%m.%d) && git push --tags`\n3. Run the workflow manually\n4. Watch the error rate in Sentry for 10 minutes\n\n## Rollback\n\n```sh\naws ecs update-service --cluster acme-prod --service storefront --task-definition storefront:PREVIOUS\n```\n\nBreak-glass AWS password: `!breakglass-demo-7731!`\n\n> If the database migration already ran, ask Elif before rolling back.\n\n## Checklist\n\n- [x] CI green on `main`\n- [x] Staging smoke test\n- [ ] Changelog posted",
            ),
            note(
                Some("Payments"),
                "Stripe test keys",
                &["billing/stripe", "env/staging"],
                "Publishable: `pk_test_51DEMOxxxxxxxxxxxxxxxxxxxx`\nSecret:      `!sk_test_51DEMOxxxxxxxxxxxxxxxxxxxx!`\nWebhook:     `!whsec_DEMOxxxxxxxxxxxxxxxx!`\nDashboard:   `https://dashboard.stripe.com/test`",
            ),
            note(
                Some("Payments"),
                "Stripe webhook endpoints",
                &["billing/stripe"],
                "Prod:    `https://api.acme-demo.com/webhooks/stripe`\nStaging: `https://api.staging.acme-demo.com/webhooks/stripe`\nEvents:  checkout.session.completed, invoice.paid, customer.subscription.deleted",
            ),
            note(
                Some("Analytics"),
                "Google Analytics",
                &["analytics/ga"],
                "Property:  `G-DEMO12345`\nLogin:     `analytics@acme-demo.com`\nPassword:  `!ga-demo-2026!`",
            ),
            note(
                Some("Analytics"),
                "Hotjar",
                &["analytics/hotjar"],
                "Site ID: `3456789`\nLogin:   `apo@acme-demo.com` / `!hj-demo-71!`",
            ),
            note(
                Some("Third-party"),
                "SendGrid",
                &["email/sendgrid", "env/prod"],
                "API key:  `!SG.DEMO.xxxxxxxxxxxxxxxxxxxxxxxx!`\nSender:   `no-reply@acme-demo.com`\nTemplate: `d-demo0a1b2c3d4e5f`",
            ),
            note(
                None,
                "Office Wi-Fi",
                &["personal"],
                "SSID:     `AcmeOffice-5G`\nPassword: `!welcome-to-acme-demo!`\nGuest:    `AcmeGuest` / `acme2026`",
            ),
        ],
    },
    SeedProject {
        name: "Muhit",
        readme: "# Muhit\n\nNeighbourhood app. Self-hosted on Hetzner.\n\n## Deploy\n\n```sh\nssh deploy@203.0.113.42\ncd /srv/muhit && ./deploy.sh\n```\n\n## Renewals\n\n- Hetzner CX22: monthly on the 5th, €4.51\n- Domain `muhit-demo.com`: 2027-03-14\n- Apple Developer: 2027-06-02\n",
        folders: &[
            ("Servers", None),
            ("Accounts", None),
            ("Mobile", None),
            ("iOS", Some("Mobile")),
            ("Android", Some("Mobile")),
        ],
        items: &[
            note(
                Some("Accounts"),
                "Hetzner login",
                &["cloud/hetzner"],
                "URL:      `https://accounts.hetzner.com`\nEmail:    `ops@muhit-demo.com`\nPassword: `!correct-horse-battery-demo!`",
            ),
            note(
                Some("Accounts"),
                "Namecheap",
                &["infra/dns", "billing"],
                "User:     `muhitdemo`\nPassword: `!nc-demo-5521!`\nDomain:   `muhit-demo.com`, renews 2027-03-14",
            ),
            note(
                Some("Servers"),
                "Production SSH",
                &["infra/ssh", "env/prod"],
                "Host:    `203.0.113.42`\nUser:    `deploy`\nConnect: `ssh deploy@203.0.113.42`\nKey:     `~/.ssh/muhit_demo_ed25519`\nSudo:    `!demo-sudo-9921!`",
            ),
            note(
                Some("Servers"),
                "DNS setup",
                &["infra/dns"],
                "Cloudflare free plan.\n\nA      @    `203.0.113.42`\nCNAME  www  `muhit-demo.com`\nAPI token: `!cf-demo-token-8812!`",
            ),
            note(
                Some("Servers"),
                "Incident checklist",
                &["runbook", "env/prod"],
                "# Incident checklist\n\n- [ ] Check `https://status.muhit-demo.com`\n- [ ] SSH in: `ssh deploy@203.0.113.42`\n- [ ] Tail logs: `journalctl -u muhit -f`\n- [ ] Restart if stuck: `sudo systemctl restart muhit`\n- [ ] Post an update in the app banner\n\n## Who to call\n\n| Area | Person | Phone |\n| --- | --- | --- |\n| Server | Apo | `+90 555 010 0000` |\n| App | Deniz | `+90 555 010 0001` |\n\nHetzner console rescue password: `!rescue-demo-1122!`",
            ),
            secure(
                Some("Servers"),
                "Backup encryption key",
                &["infra/backup"],
                "restic repo: `s3:s3.eu-central-1.amazonaws.com/muhit-backups`\nRESTIC_PASSWORD=demo-restic-4f8a2b\nRuns nightly at 03:00 via systemd timer.",
            ),
            note(
                Some("iOS"),
                "App Store Connect",
                &["mobile/ios"],
                "Apple ID:  `dev@muhit-demo.com`\nPassword:  `!asc-demo-3391!`\nTeam ID:   `QDEMO5L4QA`\nBundle ID: `com.muhit.app`",
            ),
            secure(
                Some("iOS"),
                "APNs auth key",
                &["mobile/ios", "env/prod"],
                "Key ID: `DEMO8K2L9Q`\nTeam ID: `QDEMO5L4QA`\n-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgDEMO\n-----END PRIVATE KEY-----",
            ),
            note(
                Some("Android"),
                "Play Console",
                &["mobile/android"],
                "Account:  `dev@muhit-demo.com`\nPackage:  `com.muhit.app`\nKeystore password: `!ks-demo-7782!`\nKey alias: `muhit-upload`",
            ),
            note(
                Some("Android"),
                "Firebase",
                &["mobile/android", "mobile/ios"],
                "Project:    `muhit-demo`\nServer key: `!AAAA-demo-fcm-xxxxxxxx!`\nConsole:    `https://console.firebase.google.com/project/muhit-demo`",
            ),
        ],
    },
    SeedProject {
        name: "Northwind Logistics",
        readme: "# Northwind Logistics\n\nFleet tracking dashboard. Retainer, 20h/month.\n\n## Environments\n\n| Env | URL |\n| --- | --- |\n| Prod | `https://track.northwind-demo.com` |\n| Staging | `https://track-stg.northwind-demo.com` |\n\n## Client VPN\n\nGateway `vpn.northwind-demo.com`, shared secret `!nw-ipsec-demo-11!`.\n",
        folders: &[
            ("Cloud", None),
            ("GCP", Some("Cloud")),
            ("Integrations", None),
        ],
        items: &[
            note(
                Some("GCP"),
                "GCP project",
                &["cloud/gcp", "env/prod"],
                "Project ID: `northwind-track-prod`\nConsole: `https://console.cloud.google.com/home/dashboard?project=northwind-track-prod`\nBilling account: `01DEMO-ABCDEF-123456`",
            ),
            secure(
                Some("GCP"),
                "Service account JSON",
                &["cloud/gcp", "env/prod"],
                "{\n  \"type\": \"service_account\",\n  \"project_id\": \"northwind-track-prod\",\n  \"client_email\": \"deployer@northwind-track-prod.iam.gserviceaccount.com\",\n  \"private_key_id\": \"demo0123456789abcdef\"\n}",
            ),
            note(
                Some("GCP"),
                "Cloud SQL",
                &["infra/db", "cloud/gcp"],
                "Instance: `northwind-track-prod:europe-west1:fleet`\nUser:     `fleet_app`\nPassword: `!cloudsql-demo-6620!`\nProxy:    `cloud-sql-proxy northwind-track-prod:europe-west1:fleet`",
            ),
            note(
                Some("Integrations"),
                "Mapbox",
                &["maps/mapbox"],
                "Public token: `pk.eyJ1IjoibndkZW1vIiwiYSI6ImRlbW8ifQ.DEMO`\nSecret token: `!sk.eyJ1IjoibndkZW1vIiwiYSI6ImRlbW8ifQ.SECRETDEMO!`",
            ),
            note(
                Some("Integrations"),
                "Twilio SMS",
                &["sms/twilio"],
                "Account SID: `ACDEMO1234567890abcdef1234567890`\nAuth token:  `!twilio-demo-0f9e8d7c!`\nFrom number: `+1 555 010 4477`",
            ),
            note(
                None,
                "Client contacts",
                &["contacts"],
                "Ops lead: Sarah Lin, `sarah.lin@northwind-demo.com`\nIT:       Tom Becker, `it@northwind-demo.com`\nOn-call:  `+49 30 5550 1234`",
            ),
            note(
                None,
                "Meeting notes 2026-10",
                &["meetings"],
                "## Kickoff, Oct 14\n\n*Attendees:* Sarah, Tom, Apo\n\n- Map tiles move from Mapbox to self-hosted by **Q1**\n- SMS alerts stay on Twilio for now\n- Tom sends the VPN profile this week\n\n### Action items\n\n- [ ] Apo: cost estimate for tile hosting\n- [x] Sarah: share fleet CSV export\n- [ ] Tom: VPN profile for staging",
            ),
        ],
    },
    SeedProject {
        name: "Blueprint Studio",
        readme: "# Blueprint Studio\n\nAgency partner. We do the backend, they do design.\n\n- Shared drive: `https://drive.google.com/drive/folders/DEMOblueprint`\n- Slack: `blueprint-studio.slack.com`, channel `#coffer-backend`\n",
        folders: &[
            ("Design", None),
            ("Hosting", None),
            ("Vercel", Some("Hosting")),
        ],
        items: &[
            note(
                Some("Design"),
                "Figma team",
                &["design/figma"],
                "Team:     `Blueprint Studio`\nLogin:    `apo@blueprint-demo.com`\nPassword: `!figma-demo-2210!`",
            ),
            note(
                Some("Design"),
                "Font licenses",
                &["design/fonts", "billing"],
                "Inter: free (OFL)\nGT America: web license `GTA-DEMO-4412`, 250k pageviews\nLicense portal password: `!fonts-demo-77!`",
            ),
            note(
                Some("Vercel"),
                "Vercel project",
                &["hosting/vercel", "env/prod"],
                "Team:    `blueprint-studio`\nProject: `client-portal`\nDeploy hook: `!https://api.vercel.com/v1/integrations/deploy/prj_DEMO/hook!`",
            ),
            secure(
                Some("Vercel"),
                "Vercel env (prod)",
                &["hosting/vercel", "env/prod"],
                "NEXT_PUBLIC_API_URL=`https://api.blueprint-demo.com`\nAUTH_SECRET=demo-auth-secret-91b2\nDATABASE_URL=postgres://portal:demo@db.blueprint-demo.com/portal",
            ),
            note(
                Some("Hosting"),
                "Domain registrar",
                &["infra/dns"],
                "Registrar: Porkbun\nLogin:     `ops@blueprint-demo.com` / `!pb-demo-3388!`\nDomains:   `blueprint-demo.com`, `blueprint-demo.studio`",
            ),
        ],
    },
    SeedProject {
        name: "Side Projects",
        readme: "# Side Projects\n\nSmall things that may or may not ship.\n",
        folders: &[
            ("Coffer", None),
            ("Release", Some("Coffer")),
            ("Signing", Some("Release")),
            ("Bots", None),
        ],
        items: &[
            note(
                Some("Signing"),
                "Apple notarization",
                &["mobile/ios", "release"],
                "Apple ID:     `dev@apo-demo.dev`\nApp password: `!abcd-efgh-ijkl-mnop!`\nTeam ID:      `QDEMO5L4QA`\nCommand: `xcrun notarytool submit Coffer.zip --keychain-profile coffer`",
            ),
            secure(
                Some("Signing"),
                "Developer ID certificate",
                &["release"],
                "Common name: Developer ID Application: Apo Demo (QDEMO5L4QA)\n.p12 password: demo-p12-5531\nExpires: 2031-02-11",
            ),
            note(
                Some("Release"),
                "GitHub release token",
                &["ci/github", "release"],
                "Repo:  `github.com/apo-demo/coffer`\nToken: `!github_pat_DEMOxxxxxxxxxxxxxxxx!`\nScope: contents:write",
            ),
            note(
                Some("Bots"),
                "Telegram bot",
                &["bots/telegram"],
                "Bot:     `@coffer_demo_bot`\nToken:   `!123456789:AAH-demo-telegram-token!`\nWebhook: `https://bots.apo-demo.dev/telegram`",
            ),
            note(
                Some("Bots"),
                "Discord bot",
                &["bots/discord"],
                "App ID: `1100000000000000001`\nToken:  `!MTEw.demo.discord-token!`\nInvite: `https://discord.com/oauth2/authorize?client_id=1100000000000000001`",
            ),
        ],
    },
    SeedProject {
        name: "Personal",
        readme: "# Personal\n\nKeys and accounts that don't belong to a client.\n\nHome router admin: `192.168.1.1`, password `!router-demo-55!`\n",
        folders: &[("AI", None), ("Finance", None), ("Home", None)],
        items: &[
            note(
                Some("AI"),
                "Anthropic API key",
                &["ai/anthropic"],
                "Key:         `!sk-ant-api03-DEMOxxxxxxxxxxxxxxxx!`\nConsole:     `https://console.anthropic.com`\nMonthly cap: $50",
            ),
            secure(
                Some("AI"),
                "OpenAI API key",
                &["ai/openai"],
                "Key: `sk-proj-DEMOxxxxxxxxxxxxxxxx`\nOrg: `org-DEMO1234`",
            ),
            note(
                Some("AI"),
                "Hugging Face",
                &["ai/huggingface"],
                "User:  `apo-demo`\nToken: `!hf_DEMOxxxxxxxxxxxxxxxxxxxx!`",
            ),
            note(
                Some("Finance"),
                "Subscriptions",
                &["billing"],
                "Figma Pro:       yearly, 2027-01-12, $144\nApple Developer: yearly, 2027-06-02, $99\nGitHub Pro:      monthly, $4\nSupport email:   `billing@apo-demo.dev`",
            ),
            note(
                None,
                "Markdown cheatsheet",
                &["reference"],
                "# Markdown in notes\n\nEvery note is markdown. Each new line stays a new line, and spaces are kept.\n\n## Text\n\n**Bold**, *italic*, ~~struck~~ and [links](https://example.com).\n\n## Values\n\n- Copyable: `api.example.com`\n- Secret: `!hunter2-demo!`\n- Not a secret (spaces): ` !just text! `\n\n## Lists\n\n1. First\n2. Second\n   - Nested\n\n- [x] Done\n- [ ] To do\n\n## Table\n\n| Key | Value |\n| --- | --- |\n| Region | `eu-central-1` |\n| Password | `!table-secret-demo!` |\n\n## Code\n\n```sh\ncurl -H \"Authorization: Bearer $TOKEN\" https://api.example.com/me\n```\n\n> Quotes work too.",
            ),
            secure(
                Some("Finance"),
                "Bank (business)",
                &["finance"],
                "IBAN: `TR00 0000 0000 0000 0000 0000 00`\nCustomer no: 12345678\nInternet banking password: demo-bank-0000",
            ),
            note(
                Some("Home"),
                "Home Wi-Fi",
                &["personal"],
                "SSID:     `apo-home`\nPassword: `!home-wifi-demo-321!`\nGuest:    `apo-guest` / `guest-demo`",
            ),
            note(
                Some("Home"),
                "NAS",
                &["personal", "infra/backup"],
                "Address:  `http://nas.local:5000`\nUser:     `admin`\nPassword: `!nas-demo-8080!`",
            ),
        ],
    },
];

struct SeedTodo {
    /// `None` for a general todo.
    project: Option<&'static str>,
    title: &'static str,
    description: &'static str,
    /// Days from today; negative is overdue.
    due_in: Option<i64>,
    done: bool,
    /// Title of a note in the same project (or anywhere, for general todos).
    note: Option<&'static str>,
}

const fn todo(
    project: Option<&'static str>,
    title: &'static str,
    due_in: Option<i64>,
    description: &'static str,
) -> SeedTodo {
    SeedTodo {
        project,
        title,
        description,
        due_in,
        done: false,
        note: None,
    }
}

const TODOS: &[SeedTodo] = &[
    todo(
        None,
        "Renew passport",
        Some(18),
        "Appointment at the consulate, bring two photos.",
    ),
    todo(
        None,
        "Call the accountant",
        Some(0),
        "Ask about the Q3 VAT filing.",
    ),
    todo(None, "Back up the NAS to the external disk", Some(-3), ""),
    todo(None, "Read the Postgres 18 release notes", None, ""),
    SeedTodo {
        done: true,
        ..todo(None, "Pay the electricity bill", Some(-6), "")
    },
    SeedTodo {
        note: Some("Postgres (staging)"),
        ..todo(
            Some("Acme Corp"),
            "Rotate the staging DB password",
            Some(2),
            "New password: `!stg-rotated-demo-5512!`\nUpdate `.env (staging)` afterwards.",
        )
    },
    todo(
        Some("Acme Corp"),
        "Send the October invoice",
        Some(-1),
        "To `billing@acme-demo.com`, net 30.",
    ),
    todo(
        Some("Acme Corp"),
        "Review the checkout redesign in Figma",
        Some(6),
        "",
    ),
    SeedTodo {
        note: Some("Namecheap"),
        ..todo(
            Some("Muhit"),
            "Move the domain to Cloudflare",
            None,
            "Unlock code from Namecheap: `!EPP-demo-77AC!`",
        )
    },
    todo(
        Some("Muhit"),
        "Submit 2.4 to App Review",
        Some(0),
        "- [ ] Screenshots\n- [ ] Release notes",
    ),
    todo(
        Some("Northwind Logistics"),
        "Cost estimate for tile hosting",
        Some(9),
        "",
    ),
    SeedTodo {
        done: true,
        ..todo(
            Some("Blueprint Studio"),
            "Renew the GT America web license",
            Some(-12),
            "",
        )
    },
];

struct SeedPayment {
    /// `None` for a personal payment.
    project: Option<&'static str>,
    name: &'static str,
    currency: &'static str,
    cycle: &'static str,
    start_date: &'static str,
    cancel_date: Option<&'static str>,
    /// (from, amount in hundredths). The first applies from `start_date`.
    prices: &'static [(&'static str, i64)],
}

const PAYMENTS: &[SeedPayment] = &[
    SeedPayment {
        project: Some("Acme Corp"),
        name: "AWS",
        currency: "USD",
        cycle: "monthly",
        start_date: "2025-03-04",
        cancel_date: None,
        prices: &[("2025-03-04", 8_400), ("2026-02-04", 11_250)],
    },
    SeedPayment {
        project: Some("Acme Corp"),
        name: "Sentry Team",
        currency: "USD",
        cycle: "monthly",
        start_date: "2025-05-12",
        cancel_date: None,
        prices: &[("2025-05-12", 2_600)],
    },
    SeedPayment {
        project: Some("Muhit"),
        name: "Hetzner CX32",
        currency: "EUR",
        cycle: "monthly",
        start_date: "2024-11-01",
        cancel_date: None,
        prices: &[("2024-11-01", 680), ("2026-04-01", 759)],
    },
    SeedPayment {
        project: Some("Muhit"),
        name: "Apple Developer Program",
        currency: "USD",
        cycle: "yearly",
        start_date: "2024-12-20",
        cancel_date: None,
        prices: &[("2024-12-20", 9_900)],
    },
    SeedPayment {
        project: Some("Muhit"),
        name: "muhit-demo.com",
        currency: "USD",
        cycle: "yearly",
        start_date: "2025-03-14",
        cancel_date: None,
        prices: &[("2025-03-14", 1_098)],
    },
    SeedPayment {
        project: Some("Northwind Logistics"),
        name: "Mapbox",
        currency: "USD",
        cycle: "monthly",
        start_date: "2025-09-01",
        cancel_date: Some("2026-03-01"),
        prices: &[("2025-09-01", 5_000)],
    },
    SeedPayment {
        project: Some("Blueprint Studio"),
        name: "Figma Professional",
        currency: "USD",
        cycle: "monthly",
        start_date: "2025-01-08",
        cancel_date: None,
        prices: &[("2025-01-08", 1_500), ("2025-07-08", 1_600)],
    },
    SeedPayment {
        project: Some("Blueprint Studio"),
        name: "GT America web license",
        currency: "USD",
        cycle: "once",
        start_date: "2026-10-11",
        cancel_date: None,
        prices: &[("2026-10-11", 32_000)],
    },
    SeedPayment {
        project: None,
        name: "iCloud+ 2 TB",
        currency: "TRY",
        cycle: "monthly",
        start_date: "2024-06-15",
        cancel_date: None,
        prices: &[("2024-06-15", 21_999), ("2025-08-15", 34_999)],
    },
    SeedPayment {
        project: None,
        name: "Home internet",
        currency: "TRY",
        cycle: "monthly",
        start_date: "2025-02-27",
        cancel_date: None,
        prices: &[("2025-02-27", 64_900)],
    },
    SeedPayment {
        project: None,
        name: "Spotify",
        currency: "TRY",
        cycle: "monthly",
        start_date: "2024-01-03",
        cancel_date: Some("2026-06-03"),
        prices: &[("2024-01-03", 5_999)],
    },
];

pub fn run_if_requested(conn: &Connection, key: &VaultKey) -> AppResult<()> {
    if std::env::var("COFFER_SEED").as_deref() != Ok("1") || !projects::list(conn)?.is_empty() {
        return Ok(());
    }
    seed(conn, key)
}

fn seed(conn: &Connection, key: &VaultKey) -> AppResult<()> {
    let seal = |plaintext: &str| -> AppResult<SecureBody> {
        let (nonce, ciphertext) = crypto::encrypt(key, plaintext)?;
        Ok(SecureBody { nonce, ciphertext })
    };
    let seal_inline = |text: &str| -> AppResult<Option<(String, SecureBody)>> {
        let (template, spans) = secrets::extract(text);
        if spans.is_empty() {
            return Ok(None);
        }
        let json = serde_json::to_string(&spans).expect("a list of strings always serializes");
        Ok(Some((template, seal(&json)?)))
    };

    let now = now_millis();
    for project in PROJECTS {
        let project_id = projects::insert(conn, project.name, "", now)?;
        match seal_inline(project.readme)? {
            Some((template, sealed)) => {
                projects::update_readme(conn, project_id, &template, Some(&sealed), now)?
            }
            None => projects::update_readme(conn, project_id, project.readme, None, now)?,
        }

        let mut folder_ids: Vec<(&str, i64)> = Vec::new();
        for (name, parent) in project.folders {
            let parent_id = parent.map(|p| lookup(&folder_ids, p));
            let id = folders::insert(conn, project_id, parent_id, name, now)?;
            folder_ids.push((name, id));
        }

        for item in project.items {
            let body = if item.secure {
                Body::Secure(seal(item.body)?)
            } else {
                match seal_inline(item.body)? {
                    Some((template, secrets)) => Body::Inline { template, secrets },
                    None => Body::Plain(item.body.to_string()),
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

    let project_id = |name: Option<&str>| -> AppResult<Option<i64>> {
        let Some(name) = name else { return Ok(None) };
        let all = projects::list(conn)?;
        Ok(all.into_iter().find(|p| p.name == name).map(|p| p.id))
    };
    let all_items = items::list(conn)?;
    let today = now / DAY_MS;
    for todo in TODOS {
        let project = project_id(todo.project)?;
        let item_id = todo.note.and_then(|title| {
            all_items
                .iter()
                .find(|i| i.title == title && project.is_none_or(|p| i.project_id == p))
                .map(|i| i.id)
        });
        let sealed = seal_inline(todo.description)?;
        let (description, secrets) = match &sealed {
            Some((template, secrets)) => (template.as_str(), Some(secrets)),
            None => (todo.description, None),
        };
        let due_date = todo.due_in.map(|days| date_of_day(today + days));
        let record = TodoRecord {
            project_id: project,
            item_id,
            title: todo.title,
            description,
            secrets,
            due_date: due_date.as_deref(),
        };
        let id = todos::insert(conn, &record, now)?;
        if todo.done {
            todos::set_done(conn, id, true, now)?;
        }
    }

    for payment in PAYMENTS {
        let record = PaymentRecord {
            project_id: project_id(payment.project)?,
            name: payment.name,
            currency: payment.currency,
            cycle: payment.cycle,
            start_date: payment.start_date,
            cancel_date: payment.cancel_date,
        };
        let [(_, first), changes @ ..] = payment.prices else {
            panic!("seed payment {} has no price", payment.name);
        };
        let id = payments::insert(conn, &record, *first, now)?;
        for (from, amount) in changes {
            payments::update(conn, id, &record, Some((from, *amount)), now)?;
        }
    }
    Ok(())
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// `YYYY-MM-DD` for a count of days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn date_of_day(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn lookup(folder_ids: &[(&str, i64)], name: &str) -> i64 {
    folder_ids
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, id)| *id)
        .unwrap_or_else(|| panic!("seed folder {name} is defined after its use"))
}

#[cfg(test)]
mod tests {
    use aes_gcm::{
        Aes256Gcm,
        aead::{Generate, Key},
    };
    use zeroize::Zeroizing;

    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn seeds_every_project_and_round_trips_secrets() {
        let conn = open_in_memory();
        let key = Zeroizing::new(Key::<Aes256Gcm>::generate().to_vec());
        seed(&conn, &key).unwrap();

        assert_eq!(projects::list(&conn).unwrap().len(), PROJECTS.len());
        let expected: usize = PROJECTS.iter().map(|p| p.items.len()).sum();
        let all = items::list(&conn).unwrap();
        assert_eq!(all.len(), expected);
        assert!(all.iter().any(|i| i.secure) && all.iter().any(|i| i.has_secrets));

        let login = all.iter().find(|i| i.title == "AWS console").unwrap();
        let encrypted = items::encrypted(&conn, login.id).unwrap();
        let spans =
            crypto::decrypt(&key, &encrypted.sealed.nonce, &encrypted.sealed.ciphertext).unwrap();
        assert!(spans.contains("Tr0ub4dor&3-demo"));

        let todos = todos::list(&conn).unwrap();
        assert_eq!(todos.len(), TODOS.len());
        assert!(todos.iter().any(|t| t.has_secrets && t.item_id.is_some()));
        let payments = payments::list(&conn).unwrap();
        assert_eq!(payments.len(), PAYMENTS.len());
        assert!(payments.iter().any(|p| p.prices.len() > 1));
    }

    #[test]
    fn formats_days_as_dates() {
        assert_eq!(date_of_day(0), "1970-01-01");
        assert_eq!(date_of_day(20_513), "2026-03-01");
        assert_eq!(date_of_day(-1), "1969-12-31");
    }
}
