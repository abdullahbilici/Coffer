# Coffer

A local vault for the credentials, keys and notes that pile up across projects. Everything lives on your Mac, organised by project, and the sensitive parts stay encrypted and masked until you unlock them with Touch ID.

## Features

- **Projects and folders.** Each project has its own folder tree and a markdown overview for links, contacts and notes. Move notes and folders around with drag and drop.
- **Notes with secrets inside.** Write a normal note and mark only the sensitive parts:
  - `` `value` `` shows as a chip you click to copy.
  - `` `!value!` `` is a secret: encrypted at rest, masked on screen, with its own unhide and copy buttons.
  - Or make the whole note secure, so the entire body is encrypted.
- **Tags.** Tag notes with paths like `env/prod` or `cloud/aws`; nested tags are grouped automatically.
- **Search.** Find notes, projects and folders, and preview notes right in the results.
- **Touch ID.** Opening the vault asks for Touch ID (or your Mac password) and starts a 10-minute unlock window. When it ends, or your screen locks, Coffer becomes read-only: secrets stay masked and changes ask for Touch ID again.
- **Local only.** No account, no server, no network. The app opens no ports.

## Security

- Secure notes and inline secrets are encrypted with AES-256-GCM. The key is generated on first launch and stored in your macOS Keychain.
- Titles, folder names, tags and the non-secret parts of notes are stored unencrypted, so they can be browsed and searched without unlocking.
- There is no master password or export yet. If the Keychain item is lost (for example on a new Mac), encrypted content can't be recovered. Keep that in mind before storing anything you can't recreate.

## Requirements

- macOS 12 or later, on a Mac with Touch ID or a login password
- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 20 or later
- Xcode Command Line Tools: `xcode-select --install`

## Build from source

```sh
git clone <this repo>
cd coffer
npm install
npm run tauri build
```

The app is written to `src-tauri/target/release/bundle/macos/Coffer.app`. Move it to `/Applications` and open it.

To sign the build with your own Apple certificate, pass its name (`security find-identity -v -p codesigning` lists them):

```sh
APPLE_SIGNING_IDENTITY="Apple Development: Your Name (TEAMID)" npm run tauri build
```

A stable signature means macOS only asks once for Keychain access.

## Using it

| Action | Shortcut |
| --- | --- |
| Search | Cmd+K or Cmd+F |
| Save | Cmd+S |
| Make selection copyable | Cmd+E |
| Make selection secret | Cmd+P |
| Jump out of a chip while typing | Tab |
| Close the current view | Esc |

Right-click any project, folder or note for more actions.

## Built with

[Tauri](https://tauri.app), [React](https://react.dev), Rust and SQLite.
