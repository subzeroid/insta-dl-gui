# Installation

## macOS

Grab `insta-dl-gui_x.y.z_universal.dmg` from [releases](https://github.com/subzeroid/insta-dl-gui/releases) — it runs natively on both Apple Silicon and Intel Macs.

Or install via Homebrew for automatic updates on `brew upgrade`:

```sh
brew install --cask subzeroid/tap/insta-dl-gui
```

> **Note:** the app is not code-signed yet, so Gatekeeper will ask for confirmation on first launch no matter how you installed it ("Open Anyway" or right-click → Open). See [Troubleshooting → macOS blocks the app](troubleshooting.md#macos-blocks-the-app).

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
