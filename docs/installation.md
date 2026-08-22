# Installation

Download an installer from the [releases page](https://github.com/subzeroid/insta-dl-gui/releases):

| OS | File |
|---|---|
| Windows | `insta-dl-gui_x.y.z_x64-setup.exe` (NSIS) or `.msi` |
| macOS Apple Silicon | `insta-dl-gui_aarch64.dmg` |
| macOS Intel | `insta-dl-gui_x64.dmg` |
| Linux | `.AppImage`, `.deb` or `.rpm` |

## Windows

SmartScreen may warn about unsigned binaries — click **More info → Run anyway**. Code signing is planned; until then the warning is expected for every new release.

## macOS

The app is ad-hoc signed. On first launch, if Gatekeeper complains: right-click the app → **Open** → **Open**. On newer macOS versions, allow it in **System Settings → Privacy & Security**.

## Linux

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
