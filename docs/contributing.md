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
  views/              # onboarding, download, explore, queue, settings screens
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

- **All HikerAPI and CDN traffic stays in Rust.** During setup, the webview handles the token only while it is entered and sends it to Rust via Tauri IPC. Rust validates and stores it; the stored token is never returned to the webview.
- **`cdn.rs` is the only place that touches media URLs.** Don't bypass its checks.
- Backend shapes are mapped to DTOs in `hiker.rs` mappers; raw JSON never leaks to the frontend.

## Tests

```sh
npm test              # frontend unit and mounted component tests

cd src-tauri
cargo test --locked   # offline unit tests — must pass, CI enforces clippy+fmt too
SMOKE_TOKEN=... cargo test --locked --test live_download   # ~6 real API calls, run before releases
```

UI can be developed without the backend: run `npx vite`, then open `http://localhost:1420/download?mock=1` or `http://localhost:1420/explore?mock=1&demo=explore`.

## Docs

This site is MkDocs Material. With [`uv`](https://docs.astral.sh/uv/) installed, create a Python 3.12 environment with the same hash-locked dependencies as CI:

```sh
uv venv --python 3.12 .venv-docs
uv pip install --python .venv-docs/bin/python --require-hashes -r docs/requirements.txt
.venv-docs/bin/mkdocs serve
```

Edit `docs/*.md`; CI builds strictly and deploys on push to `main`.
