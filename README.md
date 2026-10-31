# ReqFlux

A local-first HTTP client built on [GPUI](https://www.gpui.rs/).
ReqFlux reads and writes Postman collection v2.1 and environment files, runs Postman
`pm.*` scripts, and lets AI agents manage collections and send requests through MCP.

ReqFlux is a standalone app: opened from your desktop it shows your own workspaces, kept by
the app under `~/.local/share/reqflux/workspaces`, and creates one named Personal on the first
run. A workspace is a directory of small TOML files, so one can also live in a repository
next to the code it calls, where collections diff, merge and review cleanly in git.

> Status: the MVP of [FEATURES.md](FEATURES.md) is in place. See
> [ARCHITECTURE.md](ARCHITECTURE.md) for how it is built.

## Install

Linux only for now.

- **Nix**: from a clone of this repository, `nix build .#reqflux` builds the package
  (binary, desktop entry and icons) into `./result`; `nix profile install .#reqflux`
  installs it; `nix run .#reqflux` starts the app directly.
- **Nightly**: once a day, when `main` has new commits, a prerelease under the single tag
  `nightly` is replaced with a fresh `reqflux-linux-amd64.tar.gz` (binary, desktop entry and
  icons); see the [releases page](https://github.com/0xErwin1/reqflux/releases).
- **Local build**: from the dev shell (`nix develop`), `cargo build --release -p reqflux`
  puts the binary in `target/release/reqflux`.

## Quick path

```sh
nix develop          # toolchain, mold and native libraries
cargo run            # opens the ReqFlux window on your last workspace (Personal at first)
cargo run -- ~/billing                     # opens the workspace in the directory ~/billing
cargo run -- run billing-api/invoices/list-invoices --workspace ~/billing --env staging
                     # sends one request, body to stdout, summary to stderr
scripts/lint.py      # fmt, clippy, file-size and layering checks
cargo nextest run    # tests
```

Without Nix you need the Rust toolchain from `rust-toolchain.toml`, `mold`, `pkg-config`
and the GPUI native dependencies (Wayland, X11, xkbcommon, Vulkan, fontconfig, freetype).

## Configuration

Your settings live in `~/.config/reqflux/config.toml`, not in any workspace. Agents connected to
`reqflux mcp` can read; anything else waits in the running window's MCP approvals view
(`Ctrl+Shift+M`, or the bell, the rail or the status bar count): Allow once, Allow for this
session, or Deny with an optional reason the agent receives. With no window open it is refused. Allow more there:

```toml
theme = "system"

[mcp]
write = "allow"
execute = "allow"
hosts = ["api.example.com", "localhost:8080"]
```

A workspace's `reqflux.toml` can add an `[mcp]` table, but it can only restrict. Every change and
execution an agent makes is logged to `~/.local/state/reqflux/audit.log`. See "Configuration and
policy" in [ARCHITECTURE.md](ARCHITECTURE.md).

## License

Licensed under the [Apache License 2.0](LICENSE-APACHE).
