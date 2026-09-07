# Third-party notices

This application is distributed as a bundle: it ships compiled Rust
dependencies alongside its own code. Their licences require their text and
copyright notices to travel with the binary. This file lists every dependency
the lockfiles pin, with the licence each one declares.

Regenerate with `python3 -B -m scripts.third_party_notices`.


## What these obligations amount to

Nearly every entry below is MIT, Apache-2.0, BSD or ISC: permissive licences
that ask for the licence text and the copyright notice to be distributed with
the binary, which is what this file and the upstream archives provide. Two
groups need a further word:

- **MPL-2.0** (the CSS parsing crates Tauri pulls in) is file-level copyleft.
  Shipping them unmodified only requires this notice and a way to obtain their
  source, which crates.io provides at the pinned versions. Modifying one of
  those files would oblige us to publish the modified files under MPL-2.0.
- **Apache-2.0** carries a patent grant and requires that its NOTICE file, where
  a project ships one, be reproduced. The upstream archives carry theirs.

No dependency here is GPL or AGPL, so nothing obliges the application itself to
adopt a copyleft licence.

## Rust dependencies (application)

580 packages, 34 distinct licence expressions.

### (Apache-2.0 OR MIT) AND BSD-3-Clause

encoding_rs 0.8.35

### (MIT OR Apache-2.0) AND Unicode-3.0

unicode-ident 1.0.24

### 0BSD OR MIT OR Apache-2.0

adler2 2.0.1

### Apache-2.0

gethostname 1.1.0, openssl 0.10.81, sync_wrapper 1.0.2, tao 0.35.3

### Apache-2.0 / MIT

fnv 1.0.7

### Apache-2.0 AND ISC

ring 0.17.14

### Apache-2.0 AND MIT

dpi 0.1.2

### Apache-2.0 OR BSL-1.0

ryu 1.0.23

### Apache-2.0 OR ISC OR MIT

hyper-rustls 0.27.9, rustls 0.23.43

### Apache-2.0 OR MIT

async-channel 2.5.0, async-executor 1.14.0, async-io 2.6.0, async-lock 3.4.2, async-process 2.5.0, async-signal 0.2.14, async-task 4.7.1, atomic-waker 1.1.2, autocfg 1.5.1, bit-set 0.8.0, bit-vec 0.8.0, blocking 1.6.2, cargo_toml 0.22.3, concurrent-queue 2.5.0, ctor 0.8.0, ctor-proc-macro 0.0.7, dtor 0.3.0, dtor-proc-macro 0.0.6, equivalent 1.0.2, event-listener 5.4.2, event-listener-strategy 0.5.4, fastrand 2.5.0, futures-lite 2.6.1, idna_adapter 1.2.2, indexmap 1.9.3, indexmap 2.14.0, libappindicator 0.9.0, libappindicator-sys 0.9.0, muda 0.19.3, parking 2.2.1, pin-project-lite 0.2.17, polling 3.11.0, portable-atomic 1.15.0, portable-atomic-util 0.2.7, rustc-hash 2.1.3, tauri 2.11.5, tauri-build 2.6.3, tauri-codegen 2.6.3, tauri-macros 2.6.3, tauri-plugin 2.6.3, tauri-plugin-clipboard-manager 2.3.2, tauri-plugin-dialog 2.7.2, tauri-plugin-fs 2.5.1, tauri-plugin-opener 2.5.4, tauri-runtime 2.11.3, tauri-runtime-wry 2.11.4, tauri-utils 2.9.3, utf8_iter 1.0.4, uuid 1.24.1, window-vibrancy 0.6.0, wry 0.55.1, zeroize 1.9.0

### Apache-2.0 WITH LLVM-exception

target-lexicon 0.12.16, winx 0.36.4

### Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT

ambient-authority 0.0.2, cap-primitives 4.0.3, cap-std 4.0.3, fs-set-times 0.20.3, io-extras 0.19.0, io-lifetimes 2.0.4, io-lifetimes 3.0.1, linux-raw-sys 0.12.1, rustix 1.1.4, rustix-linux-procfs 0.1.1, wasi 0.11.1+wasi-snapshot-preview1, wasip2 1.0.4+wasi-0.2.12, wit-bindgen 0.57.1

### Apache-2.0/MIT

cesu8 1.1.0, dbus 0.9.12, libdbus-sys 0.2.7

### BSD-2-Clause OR Apache-2.0 OR MIT

zerocopy 0.8.56, zerocopy-derive 0.8.56

### BSD-3-Clause

alloc-no-stdlib 2.0.4, alloc-stdlib 0.2.4, subtle 2.6.1

### BSD-3-Clause AND MIT

brotli 8.0.4

### BSD-3-Clause OR Apache-2.0

moxcms 0.8.1, pxfm 0.1.30

### BSD-3-Clause OR MIT OR Apache-2.0

num_enum 0.7.6, num_enum_derive 0.7.6

### BSD-3-Clause/MIT

brotli-decompressor 5.0.3

### BSL-1.0

clipboard-win 5.4.1, error-code 3.4.0

### CC0-1.0 OR MIT-0 OR Apache-2.0

dunce 1.0.5

### ISC

libloading 0.7.4, rustls-webpki 0.103.15, untrusted 0.9.0

### MIT

assert-json-diff 2.0.2, atk 0.18.2, atk-sys 0.18.2, block2 0.6.2, bytes 1.12.1, cairo-rs 0.18.5, cairo-sys-rs 0.18.2, cargo_metadata 0.19.2, cfb 0.7.3, combine 4.6.7, crunchy 0.2.4, darling 0.23.0, darling_core 0.23.0, darling_macro 0.23.0, derive_more 2.1.1, derive_more-impl 2.1.1, dlopen2 0.8.2, dlopen2_derive 0.4.3, dom_query 0.27.0, embed-resource 3.0.11, endi 1.1.1, fax 0.2.7, gdk 0.18.2, gdk-pixbuf 0.18.5, gdk-pixbuf-sys 0.18.0, gdk-sys 0.18.2, gdkwayland-sys 0.18.2, gdkx11 0.18.2, gdkx11-sys 0.18.2, generic-array 0.14.7, gio 0.18.4, gio-sys 0.18.1, glib 0.18.5, glib-macros 0.18.5, glib-sys 0.18.1, gobject-sys 0.18.0, gtk 0.18.2, gtk-sys 0.18.2, gtk3-macros 0.18.2, h2 0.4.18, http-body 1.1.0, http-body-util 0.1.5, hyper 1.11.0, hyper-util 0.1.20, ico 0.5.0, infer 0.19.0, is-docker 0.2.0, is-wsl 0.4.0, javascriptcore-rs 1.1.2, javascriptcore-rs-sys 1.1.1, libredox 0.1.20, libsqlite3-sys 0.35.0, memoffset 0.9.1, mime_guess 2.0.5, mio 1.2.2, new_debug_unreachable 1.0.6, nom 8.0.0, objc2 0.6.4, objc2-encode 4.1.0, objc2-foundation 0.3.2, open 5.4.1, openssl-sys 0.9.117, os_pipe 1.2.3, pango 0.18.3, pango-sys 0.18.0, phf 0.13.1, phf_codegen 0.13.1, phf_generator 0.13.1, phf_macros 0.13.1, phf_shared 0.13.1, plist 1.10.0, precomputed-hash 0.1.1, quick-xml 0.41.0, redox_syscall 0.5.18, redox_users 0.5.2, rfd 0.16.0, rusqlite 0.37.0, schannel 0.1.29, schemars 0.8.22, schemars 0.9.0, schemars 1.2.2, schemars_derive 0.8.22, simd-adler32 0.3.10, slab 0.4.12, soup3 0.5.0, soup3-sys 0.5.0, strsim 0.11.1, synstructure 0.13.2, tauri-winres 0.3.6, tiff 0.11.3, tokio 1.53.1, tokio-macros 2.7.2, tokio-native-tls 0.3.1, tokio-util 0.7.19, tower 0.5.3, tower-http 0.6.11, tower-layer 0.3.3, tower-service 0.3.3, tracing 0.1.44, tracing-attributes 0.1.31, tracing-core 0.1.36, tree_magic_mini 3.2.2, try-lock 0.2.5, uds_windows 1.2.1, urlpattern 0.3.0, version-compare 0.2.1, vswhom 0.1.0, vswhom-sys 0.1.3, want 0.3.1, wayland-backend 0.3.17, wayland-client 0.31.15, wayland-protocols 0.32.13, wayland-protocols-wlr 0.3.12, wayland-scanner 0.31.11, wayland-sys 0.31.11, webkit2gtk 2.0.2, webkit2gtk-sys 2.0.2, webview2-com 0.38.2, webview2-com-macros 0.8.1, webview2-com-sys 0.38.2, winnow 0.5.40, winnow 0.7.15, winnow 1.0.4, winreg 0.55.0, x11 2.21.0, x11-dl 2.21.0, zbus 5.19.0, zbus_macros 5.19.0, zbus_names 4.3.4, zcheapstr 1.1.0, zmij 1.0.23, zvariant 5.15.0, zvariant_derive 5.15.0, zvariant_utils 4.2.0

### MIT OR Apache-2.0

android_system_properties 0.1.6, anyhow 1.0.104, arboard 3.6.1, async-broadcast 0.7.2, async-recursion 1.1.1, async-trait 0.1.92, base64 0.21.7, base64 0.22.1, bitflags 2.13.1, block-buffer 0.10.4, bumpalo 3.20.3, camino 1.2.5, cargo-platform 0.1.9, cc 1.4.4, cfg-expr 0.15.8, cfg-if 1.0.4, chrono 0.4.45, cookie 0.18.2, core-foundation 0.10.1, core-foundation 0.9.4, core-foundation-sys 0.8.7, core-graphics 0.25.0, core-graphics-types 0.2.0, cpufeatures 0.2.17, crc32fast 1.5.0, crossbeam-channel 0.5.16, crossbeam-utils 0.8.22, crypto-common 0.1.7, deadpool 0.12.3, deadpool-runtime 0.1.4, defmt 1.1.1, defmt-macros 1.1.1, defmt-parser 1.0.0, deranged 0.5.8, digest 0.10.7, dirs 6.0.0, dirs-sys 0.5.0, displaydoc 0.2.7, dtoa 1.0.11, dyn-clone 1.0.20, embed_plist 1.2.2, enumflags2 0.7.12, enumflags2_derive 0.7.12, erased-serde 0.4.10, errno 0.3.14, fdeflate 0.3.7, field-offset 0.3.6, find-msvc-tools 0.1.11, fixedbitset 0.5.7, flate2 1.1.9, form_urlencoded 1.2.2, futures 0.3.34, futures-channel 0.3.34, futures-core 0.3.34, futures-executor 0.3.34, futures-io 0.3.34, futures-macro 0.3.34, futures-sink 0.3.34, futures-task 0.3.34, futures-util 0.3.34, getrandom 0.2.17, getrandom 0.3.4, getrandom 0.4.3, glob 0.3.4, half 2.7.1, hashbrown 0.12.3, hashbrown 0.15.5, hashbrown 0.17.1, hashlink 0.10.0, heck 0.4.1, heck 0.5.0, hermit-abi 0.5.2, hex 0.4.3, html5ever 0.38.0, http 1.5.0, httparse 1.10.1, httpdate 1.0.3, iana-time-zone 0.1.65, iana-time-zone-haiku 0.1.2, idna 1.1.0, image 0.25.10, ipnet 2.12.1, itoa 1.0.18, jni-sys 0.3.1, jni-sys 0.4.1, jni-sys-macros 0.4.1, js-sys 0.3.104, jsonptr 0.6.3, keyboard-types 0.7.0, lazy_static 1.5.0, libc 0.2.189, lock_api 0.4.14, log 0.4.33, markup5ever 0.38.0, maybe-owned 0.3.4, mime 0.3.17, native-tls 0.2.18, ndk 0.9.0, ndk-sys 0.6.0+11769913, num-conv 0.2.2, num-traits 0.2.19, num_cpus 1.17.0, once_cell 1.21.4, openssl-probe 0.2.1, ordered-stream 0.2.0, parking_lot 0.12.5, parking_lot_core 0.9.12, percent-encoding 2.3.2, petgraph 0.8.3, piper 0.2.5, pkg-config 0.3.34, png 0.17.16, png 0.18.1, powerfmt 0.2.0, proc-macro-crate 1.3.1, proc-macro-crate 2.0.2, proc-macro-crate 3.5.0, proc-macro-error 1.0.4, proc-macro-error-attr 1.0.4, proc-macro2 1.0.107, quote 1.0.47, ref-cast 1.0.27, ref-cast-impl 1.0.27, regex 1.13.1, regex-automata 0.4.18, regex-syntax 0.8.11, reqwest 0.12.28, reqwest 0.13.4, rustc_version 0.4.1, rustls-pki-types 1.15.1, rustversion 1.0.23, scopeguard 1.2.0, security-framework 3.7.0, security-framework-sys 2.17.0, semver 1.0.28, serde 1.0.229, serde-untagged 0.1.9, serde_core 1.0.229, serde_derive 1.0.229, serde_derive_internals 0.29.1, serde_json 1.0.151, serde_repr 0.1.21, serde_spanned 0.6.9, serde_spanned 1.1.1, serde_with 3.22.0, serde_with_macros 3.22.0, serialize-to-javascript 0.1.2, serialize-to-javascript-impl 0.1.2, servo_arc 0.4.3, sha2 0.10.9, shlex 2.0.1, signal-hook-registry 1.4.8, smallvec 1.15.2, socket2 0.6.5, softbuffer 0.4.8, stable_deref_trait 1.2.1, string_cache 0.9.0, string_cache_codegen 0.6.1, swift-rs 1.0.8, syn 1.0.109, syn 2.0.119, syn 3.0.3, system-configuration 0.7.0, system-configuration-sys 0.6.0, system-deps 6.2.2, tao-macros 0.1.4, tempfile 3.27.0, tendril 0.5.1, thiserror 1.0.69, thiserror 2.0.20, thiserror-impl 1.0.69, thiserror-impl 2.0.20, time 0.3.55, time-core 0.1.9, time-macros 0.2.32, tokio-rustls 0.26.4, toml 0.8.2, toml 0.9.12+spec-1.1.0, toml 1.1.4+spec-1.1.0, toml_datetime 0.6.3, toml_datetime 0.7.5+spec-1.1.0, toml_datetime 1.1.1+spec-1.1.0, toml_edit 0.19.15, toml_edit 0.20.2, toml_edit 0.25.13+spec-1.1.0, toml_parser 1.1.3+spec-1.1.0, toml_writer 1.1.2+spec-1.1.0, tray-icon 0.24.2, typeid 1.0.3, typenum 1.20.1, unicase 2.9.0, unicode-segmentation 1.13.3, url 2.5.8, wasm-bindgen 0.2.127, wasm-bindgen-futures 0.4.77, wasm-bindgen-macro 0.2.127, wasm-bindgen-macro-support 0.2.127, wasm-bindgen-shared 0.2.127, wasm-streams 0.4.2, wasm-streams 0.5.0, web-sys 0.3.104, web_atoms 0.2.6, weezl 0.1.12, windows 0.61.3, windows-collections 0.2.0, windows-core 0.61.2, windows-core 0.62.2, windows-future 0.2.1, windows-implement 0.60.2, windows-interface 0.59.3, windows-link 0.1.3, windows-link 0.2.1, windows-numerics 0.2.0, windows-registry 0.6.1, windows-result 0.3.4, windows-result 0.4.1, windows-strings 0.4.2, windows-strings 0.5.1, windows-sys 0.45.0, windows-sys 0.52.0, windows-sys 0.59.0, windows-sys 0.60.2, windows-sys 0.61.2, windows-targets 0.42.2, windows-targets 0.52.6, windows-targets 0.53.5, windows-threading 0.1.0, windows-version 0.1.7, windows_aarch64_gnullvm 0.42.2, windows_aarch64_gnullvm 0.52.6, windows_aarch64_gnullvm 0.53.1, windows_aarch64_msvc 0.42.2, windows_aarch64_msvc 0.52.6, windows_aarch64_msvc 0.53.1, windows_i686_gnu 0.42.2, windows_i686_gnu 0.52.6, windows_i686_gnu 0.53.1, windows_i686_gnullvm 0.52.6, windows_i686_gnullvm 0.53.1, windows_i686_msvc 0.42.2, windows_i686_msvc 0.52.6, windows_i686_msvc 0.53.1, windows_x86_64_gnu 0.42.2, windows_x86_64_gnu 0.52.6, windows_x86_64_gnu 0.53.1, windows_x86_64_gnullvm 0.42.2, windows_x86_64_gnullvm 0.52.6, windows_x86_64_gnullvm 0.53.1, windows_x86_64_msvc 0.42.2, windows_x86_64_msvc 0.52.6, windows_x86_64_msvc 0.53.1, x11rb 0.13.2, x11rb-protocol 0.13.2

### MIT OR Apache-2.0 OR LGPL-2.1-or-later

r-efi 5.3.0, r-efi 6.0.0

### MIT OR Apache-2.0 OR Zlib

raw-window-handle 0.6.2, tinyvec_macros 0.1.1, zune-core 0.5.3, zune-jpeg 0.5.15

### MIT OR Zlib OR Apache-2.0

miniz_oxide 0.8.9

### MIT/Apache-2.0

bitflags 1.3.2, bs58 0.5.1, downcast-rs 1.2.1, fallible-iterator 0.3.0, fallible-streaming-iterator 0.1.9, filetime 0.2.29, foreign-types 0.3.2, foreign-types 0.5.0, foreign-types-macros 0.2.4, foreign-types-shared 0.1.1, foreign-types-shared 0.3.1, fs2 0.4.3, hyper-tls 0.6.0, ident_case 1.0.1, jni 0.21.1, json-patch 3.0.1, openssl-macros 0.1.1, quick-error 2.0.1, serde_urlencoded 0.7.1, siphasher 1.0.3, unic-char-property 0.9.0, unic-char-range 0.9.0, unic-common 0.9.0, unic-ucd-ident 0.9.0, unic-ucd-version 0.9.0, vcpkg 0.2.15, version_check 0.9.5, winapi 0.3.9, winapi-i686-pc-windows-gnu 0.4.0, winapi-x86_64-pc-windows-gnu 0.4.0, wiremock 0.6.5, wl-clipboard-rs 0.9.3

### MPL-2.0

cssparser 0.36.0, cssparser-macros 0.6.1, dtoa-short 0.3.5, option-ext 0.2.0, selectors 0.36.1

### Unicode-3.0

icu_collections 2.3.0, icu_locale_core 2.3.0, icu_normalizer 2.3.0, icu_normalizer_data 2.3.0, icu_properties 2.3.0, icu_properties_data 2.3.0, icu_provider 2.3.1, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.4, writeable 0.6.4, yoke 0.8.3, yoke-derive 0.8.2, zerofrom 0.1.8, zerofrom-derive 0.1.7, zerotrie 0.2.5, zerovec 0.11.8, zerovec-derive 0.11.6

### Unlicense OR MIT

aho-corasick 1.1.5, byteorder 1.5.0, byteorder-lite 0.1.0, jiff 0.2.35, jiff-core 0.1.0, jiff-static 0.2.35, jiff-tzdb 0.1.8, jiff-tzdb-platform 0.1.3, memchr 2.8.3, winapi-util 0.1.11

### Unlicense/MIT

same-file 1.0.6, walkdir 2.5.0

### Zlib

foldhash 0.1.5, foldhash 0.2.0

### Zlib OR Apache-2.0 OR MIT

bytemuck 1.25.2, dispatch2 0.3.1, objc2-app-kit 0.3.2, objc2-cloud-kit 0.3.2, objc2-core-data 0.3.2, objc2-core-foundation 0.3.2, objc2-core-graphics 0.3.2, objc2-core-image 0.3.2, objc2-core-location 0.3.2, objc2-core-text 0.3.2, objc2-exception-helper 0.1.1, objc2-io-surface 0.3.2, objc2-quartz-core 0.3.2, objc2-ui-kit 0.3.2, objc2-user-notifications 0.3.2, objc2-web-kit 0.3.2, tinyvec 1.12.0


## JavaScript dependencies

255 packages, 10 distinct licence expressions.

### 0BSD

tslib 2.8.1

### Apache-2.0

detect-libc 2.1.2, expect-type 1.4.0, playwright 1.62.1, playwright-core 1.62.1, typescript 5.6.3

### Apache-2.0 OR MIT

@tauri-apps/api 2.11.1, @tauri-apps/cli 2.11.4, @tauri-apps/cli-darwin-arm64 2.11.4, @tauri-apps/cli-darwin-x64 2.11.4, @tauri-apps/cli-linux-arm-gnueabihf 2.11.4, @tauri-apps/cli-linux-arm64-gnu 2.11.4, @tauri-apps/cli-linux-arm64-musl 2.11.4, @tauri-apps/cli-linux-riscv64-gnu 2.11.4, @tauri-apps/cli-linux-x64-gnu 2.11.4, @tauri-apps/cli-linux-x64-musl 2.11.4, @tauri-apps/cli-win32-arm64-msvc 2.11.4, @tauri-apps/cli-win32-ia32-msvc 2.11.4, @tauri-apps/cli-win32-x64-msvc 2.11.4

### BSD-2-Clause

entities 7.0.1

### BSD-3-Clause

source-map-js 1.2.1

### BlueOak-1.0.0

jackspeak 3.4.3, minipass 7.1.3, package-json-from-dist 1.0.1, path-scurry 1.11.1

### ISC

@isaacs/cliui 8.0.2, abbrev 2.0.0, foreground-child 3.3.1, glob 10.5.0, graceful-fs 4.2.11, ini 1.3.8, isexe 2.0.0, lru-cache 10.4.3, minimatch 9.0.9, nopt 7.2.1, picocolors 1.1.1, proto-list 1.2.4, semver 7.8.5, siginfo 2.0.0, signal-exit 4.1.0, which 2.0.2

### MIT

@babel/helper-string-parser 7.29.7, @babel/helper-validator-identifier 7.29.7, @babel/parser 7.29.8, @babel/types 7.29.8, @emnapi/wasi-threads 1.2.2, @esbuild/aix-ppc64 0.25.12, @esbuild/android-arm 0.25.12, @esbuild/android-arm64 0.25.12, @esbuild/android-x64 0.25.12, @esbuild/darwin-arm64 0.25.12, @esbuild/darwin-x64 0.25.12, @esbuild/freebsd-arm64 0.25.12, @esbuild/freebsd-x64 0.25.12, @esbuild/linux-arm 0.25.12, @esbuild/linux-arm64 0.25.12, @esbuild/linux-ia32 0.25.12, @esbuild/linux-loong64 0.25.12, @esbuild/linux-mips64el 0.25.12, @esbuild/linux-ppc64 0.25.12, @esbuild/linux-riscv64 0.25.12, @esbuild/linux-s390x 0.25.12, @esbuild/linux-x64 0.25.12, @esbuild/netbsd-arm64 0.25.12, @esbuild/netbsd-x64 0.25.12, @esbuild/openbsd-arm64 0.25.12, @esbuild/openbsd-x64 0.25.12, @esbuild/openharmony-arm64 0.25.12, @esbuild/sunos-x64 0.25.12, @esbuild/win32-arm64 0.25.12, @esbuild/win32-ia32 0.25.12, @esbuild/win32-x64 0.25.12, @jridgewell/gen-mapping 0.3.13, @jridgewell/remapping 2.3.5, @jridgewell/resolve-uri 3.1.2, @jridgewell/sourcemap-codec 1.5.5, @jridgewell/trace-mapping 0.3.31, @napi-rs/lzma-linux-x64-gnu 1.5.1, @napi-rs/wasm-runtime 1.1.4, @one-ini/wasm 0.1.1, @pkgjs/parseargs 0.11.0, @rollup/rollup-android-arm-eabi 4.62.5, @rollup/rollup-android-arm64 4.62.5, @rollup/rollup-darwin-arm64 4.62.5, @rollup/rollup-darwin-x64 4.62.5, @rollup/rollup-freebsd-arm64 4.62.5, @rollup/rollup-freebsd-x64 4.62.5, @rollup/rollup-linux-arm-gnueabihf 4.62.5, @rollup/rollup-linux-arm-musleabihf 4.62.5, @rollup/rollup-linux-arm64-gnu 4.62.5, @rollup/rollup-linux-arm64-musl 4.62.5, @rollup/rollup-linux-loong64-gnu 4.62.5, @rollup/rollup-linux-loong64-musl 4.62.5, @rollup/rollup-linux-ppc64-gnu 4.62.5, @rollup/rollup-linux-ppc64-musl 4.62.5, @rollup/rollup-linux-riscv64-gnu 4.62.5, @rollup/rollup-linux-riscv64-musl 4.62.5, @rollup/rollup-linux-s390x-gnu 4.62.5, @rollup/rollup-linux-x64-gnu 4.62.5, @rollup/rollup-linux-x64-musl 4.62.5, @rollup/rollup-openbsd-x64 4.62.5, @rollup/rollup-openharmony-arm64 4.62.5, @rollup/rollup-win32-arm64-msvc 4.62.5, @rollup/rollup-win32-ia32-msvc 4.62.5, @rollup/rollup-win32-x64-gnu 4.62.5, @rollup/rollup-win32-x64-msvc 4.62.5, @standard-schema/spec 1.1.0, @tailwindcss/node 4.3.3, @tailwindcss/oxide 4.3.3, @tailwindcss/oxide-android-arm64 4.3.3, @tailwindcss/oxide-darwin-arm64 4.3.3, @tailwindcss/oxide-darwin-x64 4.3.3, @tailwindcss/oxide-freebsd-x64 4.3.3, @tailwindcss/oxide-linux-arm-gnueabihf 4.3.3, @tailwindcss/oxide-linux-arm64-gnu 4.3.3, @tailwindcss/oxide-linux-arm64-musl 4.3.3, @tailwindcss/oxide-linux-x64-gnu 4.3.3, @tailwindcss/oxide-linux-x64-musl 4.3.3, @tailwindcss/oxide-wasm32-wasi 4.3.3, @tailwindcss/oxide-win32-arm64-msvc 4.3.3, @tailwindcss/oxide-win32-x64-msvc 4.3.3, @tailwindcss/vite 4.3.3, @tybys/wasm-util 0.10.2, @types/chai 5.2.3, @types/deep-eql 4.0.2, @types/estree 1.0.9, @types/node 20.19.43, @types/whatwg-mimetype 3.0.2, @types/ws 8.18.1, @vitejs/plugin-vue 5.2.4, @vitest/expect 4.1.11, @vitest/mocker 4.1.11, @vitest/pretty-format 4.1.11, @vitest/runner 4.1.11, @vitest/snapshot 4.1.11, @vitest/spy 4.1.11, @vitest/utils 4.1.11, @volar/language-core 2.4.15, @volar/source-map 2.4.15, @volar/typescript 2.4.15, @vue/compiler-core 3.5.41, @vue/compiler-dom 3.5.41, @vue/compiler-sfc 3.5.41, @vue/compiler-ssr 3.5.41, @vue/compiler-vue2 2.7.16, @vue/devtools-api 6.6.4, @vue/devtools-api 8.2.1, @vue/devtools-kit 8.2.1, @vue/devtools-shared 8.2.1, @vue/language-core 2.2.12, @vue/reactivity 3.5.41, @vue/runtime-core 3.5.41, @vue/runtime-dom 3.5.41, @vue/server-renderer 3.5.41, @vue/shared 3.5.41, @vue/test-utils 2.4.6, alien-signals 1.0.13, ansi-regex 5.0.1, ansi-regex 5.0.1, ansi-regex 5.0.1, ansi-regex 6.3.0, ansi-styles 4.3.0, ansi-styles 6.2.3, assertion-error 2.0.1, balanced-match 1.0.2, birpc 2.9.0, brace-expansion 2.1.4, buffer-image-size 0.6.4, chai 6.2.2, color-convert 2.0.1, color-name 1.1.4, commander 10.0.1, config-chain 1.1.13, convert-source-map 2.0.0, cross-spawn 7.0.6, csstype 3.2.3, de-indent 1.0.2, eastasianwidth 0.2.0, editorconfig 1.0.7, emoji-regex 8.0.0, emoji-regex 8.0.0, emoji-regex 9.2.2, enhanced-resolve 5.24.5, es-module-lexer 2.3.2, esbuild 0.25.12, estree-walker 2.0.2, estree-walker 3.0.3, fdir 6.5.0, fsevents 2.3.2, fsevents 2.3.3, happy-dom 20.11.6, he 1.2.0, hookable 5.5.3, is-fullwidth-code-point 3.0.0, jiti 2.7.0, js-beautify 1.15.4, js-cookie 3.0.8, magic-string 0.30.21, muggle-string 0.4.1, nanoid 3.3.18, nostics 1.2.0, obug 2.1.4, path-browserify 1.0.1, path-key 3.1.1, pathe 2.0.3, perfect-debounce 2.1.0, picomatch 4.0.5, pinia 4.0.3, postcss 8.5.26, rollup 4.62.5, shebang-command 2.0.0, shebang-regex 3.0.0, stackback 0.0.2, std-env 4.2.0, string-width 4.2.3, string-width 5.1.2, string-width-cjs 4.2.3, strip-ansi 6.0.1, strip-ansi 6.0.1, strip-ansi 7.2.0, strip-ansi-cjs 6.0.1, tailwindcss 4.3.3, tapable 2.3.3, tinybench 2.9.0, tinyexec 1.3.0, tinyglobby 0.2.17, tinyrainbow 3.1.1, undici-types 6.21.0, vite 6.4.3, vitest 4.1.11, vscode-uri 3.1.0, vue 3.5.41, vue-component-type-helpers 2.2.12, vue-router 4.6.4, vue-tsc 2.2.12, whatwg-mimetype 3.0.0, why-is-node-running 2.3.0, wrap-ansi 8.1.0, wrap-ansi-cjs 7.0.0, ws 8.21.3

### MIT OR Apache-2.0

@tauri-apps/plugin-clipboard-manager 2.3.2, @tauri-apps/plugin-dialog 2.7.2, @tauri-apps/plugin-opener 2.5.4

### MPL-2.0

lightningcss 1.32.0, lightningcss-android-arm64 1.32.0, lightningcss-darwin-arm64 1.32.0, lightningcss-darwin-x64 1.32.0, lightningcss-freebsd-x64 1.32.0, lightningcss-linux-arm-gnueabihf 1.32.0, lightningcss-linux-arm64-gnu 1.32.0, lightningcss-linux-arm64-musl 1.32.0, lightningcss-linux-x64-gnu 1.32.0, lightningcss-linux-x64-musl 1.32.0, lightningcss-win32-arm64-msvc 1.32.0, lightningcss-win32-x64-msvc 1.32.0

