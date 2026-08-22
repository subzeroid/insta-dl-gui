# Contributing

Bug reports, fixes and features are welcome.

## Development setup

Requires Node 22+ and Rust (stable).

```sh
git clone https://github.com/subzeroid/insta-dl-gui.git
cd insta-dl-gui
npm install
npm run tauri dev
```

## Layout

```
src/                  # Vue 3 + TypeScript UI
  views/              # onboarding, download, queue, settings screens
  stores/             # pinia: app config + job queue state
  lib/ipc.ts          # typed wrappers over Tauri commands/events
src-tauri/src/
  hiker.rs            # HikerAPI REST client + typed error taxonomy + mappers
  cdn.rs              # CDN streamer — every safety rule lives here, one place
  commands.rs         # tauri commands: fetch/enqueue/cancel, progress events
  jobs.rs             # cancel registry for running downloads
  targets.rs          # input parser (@username / post URLs)
  config.rs           # 0600 config file with the token
```

Ground rules:

- **All HikerAPI and CDN traffic stays in Rust.** The webview never sees the token.
- **`cdn.rs` is the only place that touches media URLs.** Don't bypass its checks.
- Backend shapes are mapped to DTOs in `hiker.rs` mappers; raw JSON never leaks to the frontend.

## Tests

```sh
cd src-tauri
cargo test            # offline unit tests — must pass, CI enforces clippy+fmt too
SMOKE_TOKEN=... cargo test --test live_download   # ~6 real API calls, run before releases
```

UI can be developed without the backend: `npx vite` then open `http://localhost:1420/download?mock=1`.

## Docs

This site is MkDocs Material. Edit `docs/*.md`, preview locally with `mkdocs serve`, CI deploys on push to `main`.
