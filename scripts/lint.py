#!/usr/bin/env python3
"""Repository checks for ReqFlux.

Usage: scripts/lint.py [check ...]

Checks (all run when none is named):
  fmt        cargo fmt --check on first-party crates
  clippy     cargo clippy on first-party crates, every target and feature,
             with warnings denied
  file-size  no Rust source file over MAX_LINES lines, without exemptions
  layering   only GPUI_CRATES may depend on GPUI, and internal dependencies
             follow the order in ARCHITECTURE.md
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAX_LINES = 1000
# `bolt` is DBFlux's design system, taken by git rev; it draws with GPUI.
GPUI_PACKAGES = {"gpui-pre", "gpui-component", "gpui-pre-platform", "gpui-mcp", "bolt"}
GPUI_CRATES = {"bolt_legacy", "reqflux", "reqflux_ui"}

# Each crate may depend only on the internal crates listed here.
ALLOWED_INTERNAL = {
    # ReqFlux's own copy of Bolt, until every screen moves to the shared `bolt`; it depends
    # on no ReqFlux crate.
    "bolt_legacy": set(),
    "reqflux_core": set(),
    "reqflux_store": {"reqflux_core"},
    "reqflux_postman": {"reqflux_core"},
    "reqflux_http": {"reqflux_core"},
    "reqflux_script": {"reqflux_core"},
    "reqflux_engine": {"reqflux_core", "reqflux_http", "reqflux_postman", "reqflux_script", "reqflux_store"},
    "reqflux_mcp": {"reqflux_core", "reqflux_engine"},
    "reqflux_ui": {"bolt_legacy", "reqflux_core", "reqflux_engine"},
    "reqflux": {"reqflux_core", "reqflux_engine", "reqflux_mcp", "reqflux_ui"},
}

# Crates from outside the workspace that only the listed crates may depend on.
ALLOWED_EXTERNAL = {
    # The design system shared with DBFlux: only the front end draws with it.
    "bolt": {"reqflux_ui", "reqflux"},
}


def first_party_packages() -> list[str]:
    """Workspace members under crates/; vendored crates keep upstream style."""
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    crates_dir = ROOT / "crates"
    return [
        package
        for package in metadata["packages"]
        if Path(package["manifest_path"]).is_relative_to(crates_dir)
    ]


def run(command: list[str]) -> bool:
    print(f"$ {' '.join(command)}", flush=True)
    return subprocess.run(command, cwd=ROOT).returncode == 0


def check_fmt() -> bool:
    packages = [arg for p in first_party_packages() for arg in ("-p", p["name"])]
    return run(["cargo", "fmt", *packages, "--check"])


def check_clippy() -> bool:
    packages = [arg for p in first_party_packages() for arg in ("-p", p["name"])]
    return run(
        ["cargo", "clippy", *packages, "--all-targets", "--all-features", "--", "-D", "warnings"]
    )


def check_file_size() -> bool:
    ok = True
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        lines = sum(1 for _ in path.open(encoding="utf-8"))
        if lines > MAX_LINES:
            print(f"{path.relative_to(ROOT)}: {lines} lines (limit {MAX_LINES})")
            ok = False
    return ok


def check_layering() -> bool:
    ok = True
    for package in first_party_packages():
        name = package["name"]
        if name not in ALLOWED_INTERNAL:
            print(f"{name}: not listed in ALLOWED_INTERNAL; add it with its allowed dependencies")
            ok = False
            continue
        for dependency in package["dependencies"]:
            dep = dependency["name"]
            if dep in GPUI_PACKAGES and name not in GPUI_CRATES:
                print(f"{name}: depends on {dep}, but only {sorted(GPUI_CRATES)} may use GPUI")
                ok = False
            if dep in ALLOWED_INTERNAL and dep not in ALLOWED_INTERNAL[name]:
                print(f"{name}: depends on {dep}, which ARCHITECTURE.md does not allow")
                ok = False
            if dep in ALLOWED_EXTERNAL and name not in ALLOWED_EXTERNAL[dep]:
                print(f"{name}: depends on {dep}, which only {sorted(ALLOWED_EXTERNAL[dep])} may use")
                ok = False
    return ok


CHECKS = {
    "fmt": check_fmt,
    "clippy": check_clippy,
    "file-size": check_file_size,
    "layering": check_layering,
}


def main(argv: list[str]) -> int:
    names = argv or list(CHECKS)
    unknown = [name for name in names if name not in CHECKS]
    if unknown:
        print(f"unknown check(s): {', '.join(unknown)}; expected one of {', '.join(CHECKS)}")
        return 2
    failed = [name for name in names if not CHECKS[name]()]
    if failed:
        print(f"failed: {', '.join(failed)}")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
