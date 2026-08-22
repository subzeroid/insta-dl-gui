<div align="center">

# insta-dl-gui

A simple desktop app to download Instagram posts, reels, stories and highlights — **no Instagram login required**.

Powered by [HikerAPI](https://hikerapi.com) · built with [Tauri 2](https://tauri.app) + Vue 3

📖 **[Documentation](https://subzeroid.github.io/insta-dl-gui/)** — installation, token setup, usage, troubleshooting.

</div>

---

Paste your HikerAPI token once, then download any public profile's content with a couple of clicks. Because downloads go through the HikerAPI cloud instead of a logged-in Instagram session, there is **no account ban risk** — unlike instaloader or gallery-dl.

![insta-dl-gui — profile download screen](docs/screenshot.png)

## Features

- **Single posts & reels** — paste an `instagram.com/p/…` or `/reel/…` link
- **Full profiles** — posts, reels-only, active stories, highlights, and HD avatar, each selectable
- **No login, no ban risk** — HikerAPI token only; your Instagram account never touches the flow
- **Live progress** — per-job byte counts, file counters and cancel support
- **Incremental archives** — already-downloaded files are skipped on re-runs (like `--fast-update`)
- **Original timestamps** — file mtime is set from `taken_at`, so Photos/Finder sort correctly
- **JSON metadata sidecars** — caption, like/comment counts and owner saved next to every post (toggleable)
- **Balance indicator** — remaining HikerAPI quota always visible in the header

## Download

Grab an installer from [GitHub Releases](https://github.com/subzeroid/insta-dl-gui/releases):

| OS | File |
|---|---|
| Windows | `insta-dl-gui_x.y.z_x64-setup.exe` |
| macOS (Apple Silicon) | `insta-dl-gui_aarch64.dmg` |
| macOS (Intel) | `insta-dl-gui_x64.dmg` |
| Linux | `.AppImage` / `.deb` / `.rpm` |

> Windows SmartScreen may warn about unsigned binaries — click "More info" → "Run anyway". Code signing is planned.

## Getting a token

1. Sign up at [hikerapi.com](https://hikerapi.com) — **the first 100 requests are free**, no card needed.
2. Copy the token from <https://hikerapi.com/tokens>.
3. Paste it into the app on first launch.

One request ≈ one API call: fetching a post costs 1, stories 2, highlights 2 + 1 per highlight reel. A typical "download everything from a profile" run costs a handful of requests plus one per feed page.

## Building from source

Requires Node 22+ and Rust.

```sh
npm install
npm run tauri dev      # development
npm run tauri build    # installers into src-tauri/target/release/bundle/
```

### Tests

```sh
cd src-tauri
cargo test             # offline unit tests (CDN safety rules, error mapping, parsers)

SMOKE_TOKEN=... cargo test --test live_download   # live smoke (~6 real API calls)
```

The CDN downloader enforces defense-in-depth rules ported from [insto](https://github.com/subzeroid/insto): HTTPS-only, Instagram-CDN host allowlist, manual redirects (≤5), MIME magic-byte cross-checks, per-file byte budget, disk guard, atomic writes and collision suffixes — all covered by tests.

### UI without the backend

The frontend can run in a plain browser with a mocked Tauri IPC — handy for UI work and screenshots:

```sh
npx vite &
node scripts/screenshot.mjs "http://localhost:1420/download?mock=1&demo=profile" docs/screenshot.png
```

## Related projects

- [insta-dl](https://github.com/subzeroid/insta-dl) — the CLI sibling of this app (same output layout)
- [insto](https://github.com/subzeroid/insto) — interactive Instagram OSINT REPL
- [instagrapi](https://github.com/subzeroid/instagrapi) / [aiograpi](https://github.com/subzeroid/aiograpi) — private-API libraries for logged-in automation
- [Open Video Downloader](https://github.com/jely2002/youtube-dl-gui) — inspiration for this project's UX (for YouTube)

## Roadmap

- [ ] Hashtag downloads
- [ ] Comments export
- [ ] Auto-update channel (requires code-signing keys)
- [ ] Retry queue with backoff
- [ ] Interface translations

## Disclaimer

This app is not affiliated with, authorized, maintained, or endorsed by Instagram or Meta. It downloads only publicly accessible content via a third-party API service. Respect creators' rights and local law; you are responsible for how you use it.

## License

[MIT](LICENSE)
