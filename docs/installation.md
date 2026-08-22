# Installation

## macOS

**Homebrew (recommended)** — no Gatekeeper prompts at all, since Homebrew strips the quarantine flag:

```sh
brew install --cask subzeroid/tap/insta-dl-gui
```

The cask ships a **universal binary** (Apple Silicon + Intel).

**Manual download** — grab `insta-dl-gui_x.y.z_aarch64.dmg` (Apple Silicon) or `_x64.dmg` (Intel) from [releases](https://github.com/subzeroid/insta-dl-gui/releases). Unsigned builds trigger Gatekeeper on first launch; see [Troubleshooting → macOS blocks the app](troubleshooting.md#macos-blocks-the-app) for the two-click fix.

## Windows

Download `insta-dl-gui_x.y.z_x64-setup.exe` from [releases](https://github.com/subzeroid/insta-dl-gui/releases).

SmartScreen may warn about unsigned binaries — click **More info → Run anyway**. Code signing is planned; until then the warning is expected for every new release.

## Linux

Download from [releases](https://github.com/subzeroid/insta-dl-gui/releases):

- `.AppImage` — chmod +x and run, no installation
- `.deb` — `sudo dpkg -i insta-dl-gui_*.deb`
- `.rpm` — `sudo rpm -i insta-dl-gui_*.rpm`

## Build from source

Requires Node 22+ and a Rust toolchain.

```sh
git clone https://github.com/subzeroid/insta-dl-gui.git
cd insta-dl-gui
npm install
npm run tauri dev      # development
npm run tauri build    # installers into src-tauri/target/release/bundle/
```
