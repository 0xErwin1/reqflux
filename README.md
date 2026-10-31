# ReqFlux

A local-first HTTP client built on [GPUI](https://www.gpui.rs/).
ReqFlux reads and writes Postman collection v2.1 and environment files, runs Postman
`pm.*` scripts, and lets AI agents manage collections and send requests through MCP.

Workspaces are directories of small TOML files, so collections diff, merge and review
cleanly in git.

> Status: early. The window is still empty; `reqflux run` sends a request of a workspace
> from the command line. See [FEATURES.md](FEATURES.md) for what is planned and
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
cargo run            # opens the ReqFlux window (on the current directory if it holds reqflux.toml)
cargo run -- ~/billing                     # opens the workspace in ~/billing
cargo run -- run billing-api/invoices/list-invoices --workspace ~/billing --env staging
                     # sends one request, body to stdout, summary to stderr
scripts/lint.py      # fmt, clippy, file-size and layering checks
cargo nextest run    # tests
```

Without Nix you need the Rust toolchain from `rust-toolchain.toml`, `mold`, `pkg-config`
and the GPUI native dependencies (Wayland, X11, xkbcommon, Vulkan, fontconfig, freetype).

## License

Licensed under the [Apache License 2.0](LICENSE-APACHE).
