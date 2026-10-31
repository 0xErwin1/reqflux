# Agent guide

Read [ARCHITECTURE.md](ARCHITECTURE.md) before changing code. It owns the domain model, the
on-disk format and the crate boundaries.

## Rules

- Respect the crate order in ARCHITECTURE.md. Only `bolt`, `reqflux_ui` and `reqflux` may depend on
  GPUI; `scripts/lint.py layering` enforces it.
- `bolt` is the design system shared with DBFlux. It never names ReqFlux, HTTP or a ReqFlux type;
  ReqFlux-specific tokens and the brand live in `reqflux_ui`. Screens use `bolt` roles and
  components, never gpui-component `Theme` fields or raw colors.
- `reqflux_engine` is the only API the UI, MCP and CLI use. Never reimplement engine logic in a
  front end.
- No source file over 1000 lines. Split by responsibility before you reach it;
  `scripts/lint.py file-size` enforces it with no exemptions.
- No `unwrap`, `expect`, `panic!`, `todo!` or slice indexing outside tests (workspace lints).
  No `let _ =` on a fallible expression: handle or log the error.
- Workspace files must stay mergeable: one entity per file, no volatile fields, stable key
  order. See "On-disk format".
- Secrets never reach a workspace file, a log, an MCP response or an export without explicit
  encryption or a plaintext flag.
- Update the docs that describe a behavior in the same change as the behavior.
- Every visual change follows the design: check the artboard under `design/` that covers the
  screen first, and when the change is not there, update or add the artboard in the same change.
- Generated artifacts (code, comments, docs, commit messages) are in English.

## Driving the app

Every window runs the `gpui-mcp` bridge. Agents drive it through the `ui_*` tools of
`reqflux mcp`; until that exists, register `scripts/gpui-mcp.sh --app-id reqflux` (after
`scripts/gpui-mcp.sh install`) to inspect the element tree, click and type. Verify UI changes in the
running app, not only in tests. See "UI automation" in ARCHITECTURE.md.

Nothing is vendored. Prefer published crates; a git dependency needs a pinned `rev`.

## Checks

```sh
scripts/lint.py                       # fmt --check, clippy -D warnings, file-size, layering
cargo nextest run --workspace         # tests
```

Commits follow Conventional Commits.
