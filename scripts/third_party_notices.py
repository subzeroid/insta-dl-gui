"""Developer-only: regenerate THIRD-PARTY-NOTICES.md from the lockfiles.

Usage:
    python3 -B -m scripts.third_party_notices > THIRD-PARTY-NOTICES.md

The bundled application ships third-party code, and almost every licence it
carries (MIT and Apache-2.0 included) requires the licence text and the
copyright notice to travel with the binary. This reads the licence each crate
declares in the registry copy cargo already downloaded, so it describes exactly
the versions the lockfile pins — no network, no guessing.
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = Path.home() / ".cargo" / "registry" / "src"


def locked_crates(lockfile: Path) -> list[tuple[str, str]]:
    """(name, version) for every registry crate the lockfile pins."""
    data = tomllib.loads(lockfile.read_text(encoding="utf-8"))
    return sorted(
        (p["name"], p["version"])
        for p in data.get("package", [])
        # A package without a source is a local path dependency: our own code.
        if p.get("source", "").startswith("registry+")
    )


def declared_licence(name: str, version: str) -> str:
    for index in sorted(REGISTRY.glob("*")):
        manifest = index / f"{name}-{version}" / "Cargo.toml"
        if not manifest.is_file():
            continue
        try:
            package = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
        except (tomllib.TOMLDecodeError, KeyError, OSError):
            return "UNREADABLE"
        if isinstance(package.get("license"), str):
            return package["license"]
        if package.get("license-file"):
            return f"see {package['license-file']}"
        return "UNDECLARED"
    return "NOT CACHED"


def npm_packages(lockfile: Path) -> list[tuple[str, str, str]]:
    """(name, version, licence) for every npm dependency the lockfile pins."""
    data = json.loads(lockfile.read_text(encoding="utf-8"))
    out = []
    for path, entry in sorted(data.get("packages", {}).items()):
        if not path.startswith("node_modules/"):
            continue
        name = re.sub(r"^.*node_modules/", "", path)
        out.append((name, entry.get("version", "?"), entry.get("license", "UNDECLARED")))
    return out


def section(title: str, rows: list[tuple[str, str, str]]) -> None:
    by_licence: dict[str, list[str]] = defaultdict(list)
    for name, version, licence in rows:
        by_licence[licence].append(f"{name} {version}")
    print(f"\n## {title}\n")
    print(f"{len(rows)} packages, {len(by_licence)} distinct licence expressions.\n")
    for licence in sorted(by_licence):
        print(f"### {licence}\n")
        print(", ".join(sorted(by_licence[licence])))
        print()


def main() -> None:
    manifest = ROOT / "packaging" / "python-distributions.json"
    # An application that bundles no interpreter says so instead of describing one;
    # the same script then serves a pure Rust/JavaScript bundle unchanged.
    distributions = json.loads(manifest.read_text()) if manifest.is_file() else None
    carried = "a Python runtime and compiled" if distributions else "compiled"
    print("# Third-party notices")
    print()
    print(f"This application is distributed as a bundle: it ships {carried} Rust")
    print("dependencies alongside its own code. Their licences require their text and")
    print("copyright notices to travel with the binary. This file lists every dependency")
    print("the lockfiles pin, with the licence each one declares.")
    print()
    print("Regenerate with `python3 -B -m scripts.third_party_notices`.")
    print()
    if distributions is not None:
        print("## Bundled Python runtime")
        print()
        print(f"CPython {distributions['python_version']}, redistributed as a standalone build from")
        print("astral-sh/python-build-standalone. CPython itself is under the Python Software")
        print("Foundation License; the build scripts are under the Mozilla Public License 2.0;")
        print("and the build embeds further components under their own terms, notably OpenSSL,")
        print("SQLite (public domain), libffi, zlib, bzip2, XZ Utils and ncurses. The upstream")
        print("release carries the full texts:")
        print()
        for architecture, target in sorted(distributions["targets"].items()):
            print(f"- {architecture}: {target['url']}")

    print()
    print("## What these obligations amount to")
    print()
    print("Nearly every entry below is MIT, Apache-2.0, BSD or ISC: permissive licences")
    print("that ask for the licence text and the copyright notice to be distributed with")
    print("the binary, which is what this file and the upstream archives provide. Two")
    print("groups need a further word:")
    print()
    print("- **MPL-2.0** (the CSS parsing crates Tauri pulls in) is file-level copyleft.")
    print("  Shipping them unmodified only requires this notice and a way to obtain their")
    print("  source, which crates.io provides at the pinned versions. Modifying one of")
    print("  those files would oblige us to publish the modified files under MPL-2.0.")
    print("- **Apache-2.0** carries a patent grant and requires that its NOTICE file, where")
    print("  a project ships one, be reproduced. The upstream archives carry theirs.")
    print()
    print("No dependency here is GPL or AGPL, so nothing obliges the application itself to")
    print("adopt a copyleft licence.")

    for title, lockfile, reader in (
        ("Rust dependencies (application)", ROOT / "src-tauri" / "Cargo.lock", "cargo"),
        ("Rust dependencies (host crate)", ROOT / "Cargo.lock", "cargo"),
        ("JavaScript dependencies", ROOT / "package-lock.json", "npm"),
    ):
        if not lockfile.is_file():
            continue
        if reader == "cargo":
            rows = [(n, v, declared_licence(n, v)) for n, v in locked_crates(lockfile)]
        else:
            rows = npm_packages(lockfile)
        if rows:
            section(title, rows)


if __name__ == "__main__":
    sys.exit(main())
