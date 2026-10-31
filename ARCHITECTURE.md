# ReqFlux architecture

ReqFlux is a local-first HTTP client built on GPUI. It reads and writes Postman
v2.1 collections and environments, runs Postman `pm.*` scripts, and exposes the same engine to
AI agents through MCP. The workspace on disk is a directory of small text files, so the
collaboration path is git.

This document records the decisions made before the first line of product code. Each one
names the alternative it rejects and, where relevant, the DBFlux experience behind it.

## Decisions at a glance

| Area | Decision |
|------|----------|
| Source of truth | A workspace is a directory of TOML files, one file per request, folder, collection and environment. |
| Saving | Edits are local drafts that survive restarts. `Ctrl+S` writes the workspace file. |
| Windows | One app instance with one workspace open at a time, as in DBFlux. Switching workspace replaces the window's content; each workspace keeps its own drafts. |
| Protocols | HTTP only until after v1. |
| App state | History, recent workspaces, cookies and UI state live in a per-user SQLite file, never in the workspace. |
| Secrets | Secret variable values live in the OS keyring; workspace files hold only the key and `secret = true`. |
| Engine | One `reqflux_engine` crate drives load, resolve, script, send and record. The UI, the MCP server and the CLI all call it. |
| MCP | `reqflux mcp` is a stdio server over `reqflux_engine`. It needs no shared database with the GUI because both read the same files. |
| Scripting | QuickJS through `rquickjs`, with a `pm` API shim and bundled Postman sandbox libraries. |
| Postman | Import and export of collection v2.1 and environment files. Unsupported fields are preserved, not dropped. |
| HTTP | `reqwest` with rustls, per-request clients cached by TLS and proxy configuration. |
| GPUI | Published `gpui-pre` 0.3.7, `gpui-component` 0.7.0 and `gpui-pre-platform` 0.3.7, the newest the UI automation bridge supports; every other dependency tracks its latest release. `gpui-pre` is patched from the `gpui-mcp` git repository at a pinned rev; nothing is vendored. |
| Crates | Ten crates in a strict one-way dependency order. Only `bolt`, `reqflux_ui` and `reqflux` link GPUI. |
| Design system | Bolt, copied from DBFlux, in the product-agnostic `bolt` crate. ReqFlux supplies only its accent and its own tokens, so `bolt` can become a shared library without touching ReqFlux's screens. |
| UI automation | The upstream `gpui-mcp` bridge is always installed. `reqflux mcp` exposes it as `ui_*` tools under the same policy, so agents declare one MCP server. |

## Domain model

```text
Workspace
├── globals            (variables, lowest precedence)
├── environments[]     (N per workspace)
└── collections[]      (N per workspace)
    ├── variables      (collection variables)
    ├── environments   (optional allowlist of workspace environments)
    ├── auth, scripts  (inherited by folders and requests)
    └── items[]
        ├── Folder     (auth, scripts, items[])
        └── Request    (method, url, params, headers, body, auth, scripts, settings, examples[])
```

- **Workspace** owns N collections and N environments.
- **Environment** belongs to the workspace. A collection may restrict which environments apply
  to it with an allowlist; an empty list means all of them. The UI only offers the allowed
  environments when a request of that collection is active.
- **Collection** owns N requests, optionally grouped in folders, plus collection variables,
  auth and scripts.
- **Request** covers any HTTP method, including custom verbs. Body modes follow Postman: `none`,
  `raw` (with a language hint), `urlencoded`, `formdata` (text and file parts), `binary` and
  `graphql`.
- **Auth** is an enum with `inherit` and `none` plus the Postman types: `basic`, `bearer`,
  `apikey`, `digest`, `oauth1`, `oauth2`, `awsv4`, `hawk`, `ntlm`, `jwt`, `edgegrid`, `asap`.
  `inherit` walks folder, then collection.
- **Example** is a saved request and response pair (Postman `response[]`).

Every entity carries a stable `id` (ULID). The file path is the human address; the id is what
history, MCP and Postman's `_postman_id` refer to. A copied file duplicates its id, so the loader
detects duplicates and assigns a fresh id to the later file, with a warning.

### Variable resolution

Precedence follows Postman, lowest to highest:

1. Workspace globals
2. Collection variables
3. Active environment
4. Iteration data (runner only)
5. Local variables (set by scripts for one execution)

Within each of the first three scopes a current value beats the shared value, and a current
value for a key the scope does not declare resolves in that scope too. A secret variable
resolves from the OS keyring only. `--var` values are local. Current values come from the
per-user app state through `ExecuteOptions::current`; `reqflux run` passes none and never
opens the app state.

`{{name}}` is resolved after pre-request scripts run, in the URL, params, headers, body, auth
fields and certificate paths. Dynamic variables (`{{$guid}}`, `{{$timestamp}}`,
`{{$randomInt}}` and the rest of Postman's list) resolve at the same point. An unresolved
variable stays literal and is reported in the response view, as Postman does.

### Execution pipeline

```text
load → collection pre-request → folder pre-request → request pre-request
     → resolve variables and auth → send → collection test → folder test → request test
     → record history
```

Each step reads and writes one `ExecutionContext` (variables by scope, request draft,
response, test results, console output). Scripts mutate the context, never the files. After a
send from the window completes, `Engine::persist_script_writes` keeps the writes of
`pm.environment`, `pm.collectionVariables` and `pm.globals` as current values, as in Postman:
a write to a secret variable goes to the keyring and never to the app state, an unset of a
secret leaves the keyring alone, and `pm.variables` writes stay local to the execution.
`reqflux run` does not persist writes.

## On-disk format

```text
billing/                              workspace root
├── reqflux.toml                        workspace manifest: id, name, format version, globals
├── environments/
│   ├── dev.env.toml
│   └── prod.env.toml
├── collections/
│   └── billing-api/
│       ├── collection.toml           id, name, variables, auth, environment allowlist, order
│       ├── collection.pre.js         optional collection pre-request script
│       ├── collection.test.js        optional collection test script
│       └── invoices/                 folder
│           ├── folder.toml           id, name, auth, order
│           ├── list-invoices.req.toml
│           ├── list-invoices.test.js
│           └── list-invoices.examples/
│               └── ok.toml
└── .reqflux/                           local, git-ignored, reserved; nothing is written here yet
```

Rules that keep files mergeable:

- **One entity per file.** Two people editing two requests never touch the same file.
- **Order lives in the parent.** `order = ["list-invoices", "create-invoice"]` in
  `folder.toml` or `collection.toml`. Unlisted children append in name order. Reordering
  rewrites one file, not every sibling.
- **No volatile fields.** No `updated_at`, no last-response cache, no UI state in workspace
  files. Volatile data in shared files turns every save into a merge conflict.
- **Scripts are sidecar `.js` files.** Editors and linters work on them directly, and a diff
  of a script is a diff of code, not of an escaped string.
- **Stable serialization.** Keys are written in a fixed order and tables are only emitted when
  non-empty, so an unchanged entity serializes to the same bytes.
- **Shared versus current values.** A workspace file holds the shared value. The current
  value a person or script sets lives in the per-user app state (see "Drafts and saving"),
  never in the workspace. Secret keys have no value in the file: their value is in the OS
  keyring, and it is both their shared and their current value.
- **Format version.** `reqflux.toml` carries `format = 1`. A newer format refuses to load with a
  clear message rather than guessing.

- **Reserved request slugs.** A request cannot be named `collection` or `folder` on disk: its
  sidecars would be `collection.pre.js` or `folder.pre.js`, the parent's own scripts. Saving
  one fails; loading one skips it with a warning.
- **Stability scope.** Files ReqFlux wrote reload and save to the same bytes. A file written by
  hand may be normalized once on its first save (string quoting, inline tables, unknown
  top-level keys moved under `[extensions]`); key order inside `[extensions]` and unmodeled
  auth types is preserved.

Writes are atomic (temporary file plus rename) and carry the content hash the writer last
read. A mismatch means someone else changed the file (another window, `reqflux mcp`, a `git
pull`) and the write fails with a conflict instead of overwriting. The check and the rename
are not locked together, so a write landing between them is lost; the window is one rename.
ReqFlux has no live file watcher yet: a change made by another editor, a `git pull` or an agent
is noticed when the next save hits the changed file (the conflict) or when the workspace is
reopened. A watcher that reloads changed files while the window is open is a planned
addition.

### File schemas

Keys appear in the order shown. Defaults are never written: `enabled = true`,
`secret = false`, empty lists, `auth` of type `inherit`, and absent settings. Keys ReqFlux does
not model are kept under `[extensions]` and written back. `reqflux_store`'s round-trip test
(`crates/reqflux_store/tests/round_trip.rs`) holds a fixture with every shape below.

`reqflux.toml`

```toml
id = "01JB7X3K9Q0000000000000001"
name = "Billing"
format = 1

[[globals]]
key = "host"
value = "api.test"

[[globals]]
key = "api_key"
secret = true            # a secret never has a value in the file
```

`environments/<slug>.env.toml`

```toml
id = "01JB7X3K9Q0000000000000002"
name = "Staging"

[[variables]]
key = "base_url"
value = "https://api.staging.test"
description = "staging gateway"

[[variables]]
key = "tenant_id"
value = "t_42"
enabled = false
```

`collections/<slug>/collection.toml` and `<folder>/folder.toml` (a folder has no
`environments` and no `variables`)

```toml
id = "01JB7X3K9Q0000000000000003"
name = "Billing API"
environments = ["01JB7X3K9Q0000000000000002"]   # allowlist; absent means all
order = ["invoices", "health"]

[auth]
type = "bearer"
token = "{{access_token}}"

[[variables]]
key = "version"
value = "v1"
```

`<slug>.req.toml`

```toml
id = "01JB7X3K9Q0000000000000004"
name = "Create invoice"
method = "POST"                     # any verb, stored uppercase
url = "{{base_url}}/v1/invoices"     # never holds the query string

[[params]]                           # the query string, in order
key = "expand"
value = "lines"

[[headers]]
key = "Idempotency-Key"
value = "{{$guid}}"

[auth]
type = "apikey"
key = "X-Api-Key"
value = "{{api_key}}"
location = "header"                  # or "query"

[body]
mode = "raw"
language = "json"
text = '{"currency": "usd"}'

[settings]
timeout_ms = 5000
follow_redirects = false
verify_tls = true
```

The other shapes:

| Table | Variants |
|-------|----------|
| `[auth]` | `type = "inherit"` (default, not written), `"none"`, `"basic"` (`username`, `password`), `"bearer"` (`token`), `"apikey"` (`key`, `value`, `location`), or any other type name with its fields kept as they are |
| `[body]` | `mode = "none"`, `"raw"` (`language`: `json`, `xml`, `text`, `html`, `javascript`; `text`), `"urlencoded"` (`[[body.fields]]` key and value rows), `"formdata"` (`[[body.parts]]` with `kind = "text"` and `value`, or `kind = "file"` and `path`), `"binary"` (`path`), `"graphql"` (`query`, `variables`) |
| key and value rows | `key`, `value`, optional `description`, `enabled = false` when disabled |

## Drafts and saving

Saving is explicit, as in Postman: an edit never touches the workspace file until `Ctrl+S`.

- An edited entity becomes a **draft**, stored in the per-user app state and keyed by
  workspace id and entity id. Drafts survive a restart and their tabs show them as unsaved.
  The app state is SQLite (`reqflux_engine::state`, `rusqlite`) at
  `$XDG_STATE_HOME/reqflux/state.sqlite3` (`$HOME/.local/state/reqflux/state.sqlite3` when the
  variable is unset), with an explicit, forward-only schema version (v2 adds the `history`
  table, v3 the `current_values` table keyed by workspace id, scope and key, v4 the
  `recent_workspaces` table, the 20 newest by canonical path; an older file upgrades in one
  transaction and keeps what it holds). A draft stores the
  whole entity document as TOML, so later editable surfaces need no new storage.
- **Sending** a request with a draft sends the draft. History records what was sent.

### History

Every completed or failed send in the window (not a cancelled one) is recorded in the app
state under the workspace id: when, the request id, name and path at that time, the request
document sent (TOML, references as written), the request as sent (method, URL, headers, body,
or a short description of a multipart, file or binary body) and the outcome (status, headers,
body, time and size, or the error). `reqflux_engine::state` owns the table (`record_history`,
`history`, `history_entry`, `clear_history`); front ends never write it. A body over 5 MB is
stored cut, on a character boundary for text, and flagged truncated; a workspace keeps its
newest 500 entries. A send that fails before it goes out (an unresolvable request, a failing
pre-request script) is not recorded, and neither `reqflux run` nor MCP record yet.

Secrets never reach history in clear. The engine builds the request as sent
(`Executed::sent`, `SentRequest`) with every value of every secret variable in scope replaced
by `••••••••`, as written, percent-encoded, form-encoded and JSON-escaped, in the URL, header
values and body; the values of `Authorization`, `Proxy-Authorization` and `Cookie` are always
masked (keeping the scheme word), so a credential a script assembled is masked too. The same
masking is available as `Execution::masker()` for any text a front end stores, such as the
error of a failed send. A very short secret masks every occurrence of its text, erring on
the side of hiding. The window records a response through the same masker
(`RecordedResponse::from_response_masked`), so a secret the server echoes back is stored
masked: header values (credential headers such as `Set-Cookie` masked whole), the reason
phrase and a UTF-8 text body. A body that is not UTF-8 is stored as received, since the
masker works on text. `PreparedRequest`'s `Debug` prints no URL, header value or body.
- **Saving** writes the entity file (atomic, hash-checked) and discards the draft. The
  engine entry points are `Engine::save_request`, which writes the request document back
  and returns the new file hash, and `Engine::request_base_hash`, which reports the hash
  the file had when the workspace was loaded.
- **Reverting** discards the draft.
- When the file changes underneath a draft (a `git pull`, an agent's write tool, another
  editor), the draft is kept and its tab shows that the saved version moved. Saving then
  fails with a conflict until the user picks a side. ReqFlux has no live file watcher yet: the
  change is noticed when the draft is saved (that conflict) or when the workspace is
  reopened; a watcher that notices it while the window is open is a planned addition.
- Agent write tools write files directly; they never create or read drafts. An agent and the
  user editing the same request meet at that conflict, not at a silent overwrite.

What the window does today: clicking the URL of an open request swaps it for a field
(`request-url-input`); typing stores a draft on every keystroke, Enter commits it to the
draft and Escape drops what was typed. In the Params and Headers grids, each row carries an
enable box (`request-<params|headers>-enabled-<n>`) whose click toggles the row's `enabled`
state in the same draft on the spot, the grid shows the draft's rows while a draft exists,
and clicking a row's key or value cell (`request-<table>-<key|value>-<n>`) swaps the cell
for a field (`request-cell-input`) seeded with the cell's text; Enter commits it to that
cell of the same draft, Escape drops what was typed, and clicking another cell commits the
open field first, like a spreadsheet. A value that references a secret is never seeded: the
field opens empty with a `Secret value` placeholder and an empty commit keeps the stored
reference. The row counts next to the Params and Headers sub-tabs read the draft, so a
toggle moves them. A drafted tab carries a tint dot, Save and Revert
wake up, `Ctrl+S` saves and reloads the workspace so the tree and later sends see the file,
and a moved file shows `File changed on disk` with the draft kept until Revert. Sending a
drafted request goes through `Engine::execute_draft`.

Every other part of a request edits the same draft, and every pane shows the draft while one
exists:

- **Method.** The method chip (`request-method`) opens a menu of the standard verbs
  (`method-<verb>`) and `Custom…` (`method-custom`), which opens a field
  (`request-method-input`) that takes any HTTP token, uppercased; other text leaves the
  draft unchanged.
- **Rows.** Params, Headers and the urlencoded or form-data body grid end with an `Add` row
  (`request-<table>-add`) that appends an empty enabled row and opens its key cell, and every
  row has a delete control (`request-<table>-delete-<n>`). An open cell commits before a row
  is added or deleted, so typed text never lands in a shifted row.
- **Body.** The mode row (`request-body-mode`) opens a menu (`body-mode-<none|json|xml|text|
  urlencoded|formdata>`); switching between raw languages keeps the text, any other switch
  starts empty. Raw text opens a multi-line editor (`request-body-input`) that writes the
  draft as it is typed: Enter inserts a newline, Ctrl+Enter sends, Escape leaves it. The
  urlencoded and form-data grids edit like Params and Headers (a file part's value is its
  path). GraphQL and binary bodies stay read-only.
- **Auth.** The type row (`request-auth-type`) opens a menu
  (`auth-type-<inherit|none|basic|bearer|apikey>`); a new type starts with empty fields.
  Username and Key open seeded; Password, Token and the API key value open empty with a
  `Secret value` placeholder, a typed commit replaces the stored value and an empty commit
  keeps it, so a secret is never shown in a field. Add to toggles header and query. Auth
  types ReqFlux does not support stay read-only and masked.
- **Settings.** The timeout opens a field (`request-setting-timeout-input`) that takes a
  whole number of milliseconds, or empty for the default; other text leaves the draft
  unchanged. Follow redirects and Verify TLS cycle Default, on and off on click.

Picking the value a request already has, with no draft, creates no draft. A menu closes on
Escape or a click outside. A stored secret cannot be cleared to empty from the window yet.
Undo, Redo and Split stay disabled.

Tree operations (`reqflux_store::tree_ops`, exposed as `Engine::create_collection`,
`create_folder`, `create_request`, `rename`, `duplicate`, `delete`, `move_item` and `reorder`)
change the tree on disk by id and reload the workspace after each success:

- **Create** writes `New request` (`GET`, no URL), `New folder` or `New collection`, numbered
  `2`, `3`, ... when the name is taken, with a new id, at the end of the parent.
- **Rename** changes the name and the slug, so the file or directory follows the name: a
  request's `.pre.js`, `.test.js` and `.examples/` move with it, the parent's `order` entry is
  renamed in place and the id is kept. A taken, reserved or empty name fails.
- **Duplicate** copies a request or a folder with everything under it, with new ids, as
  `<name> copy`, right after the original.
- **Delete** removes a request with its sidecars, or a folder or collection with everything
  under it, and the parent's `order` entry.
- **Move** and **reorder** put an item at an index in another parent or in its own; a folder
  never moves into itself or a descendant, and a slug clash in the target fails.

Every operation validates and hash-checks everything it touches before its first write, so a
refused operation changes nothing. Before a folder or collection is deleted or duplicated,
every entity file and sidecar script under it is compared with what the load read: a file
added or changed since makes the operation a conflict instead of a silent loss. An operation
is several atomic steps; a failure partway can leave files moved with a stale `order`, which
still loads (unlisted children append). Slugs are ASCII: other letters become `-`. Rewriting a
parent's `order` lists every child in display order, so the first rewrite of a parent may
list children that were unlisted.

Two rules keep a workspace's paths unambiguous. Saving a request whose slug is already a
folder in the same parent, or the other way round, fails with `SlugCollision` instead of
writing an ambiguous tree; a workspace that already has one still loads, with a `SharedSlug`
warning that says an `order` entry reaches both. And the layout helpers that compose a path
from a slug are checked: `checked_request_file` and its siblings validate the slug and return
`Result`, while the raw composers are crate-private, so no caller outside the store can build
a path that escapes the root.

Drafts stay out of the workspace directory so that a commit never picks one up.

## Crate map

```text
reqflux_core ──┬── reqflux_store ───┐
             ├── reqflux_postman ─┤
             ├── reqflux_http ────┼── reqflux_engine ──┬── reqflux_mcp ──┐
             └── reqflux_script ──┘                  └── reqflux_ui ───┴── reqflux (binary)
                                                         │
                                              bolt ──────┘   (design system, no ReqFlux types)
```

| Crate | Owns | Must not |
|-------|------|----------|
| `reqflux_core` | Domain types, validation, variable scopes, `{{}}` templating, dynamic variables. Pure, no IO. | Touch the filesystem, network or GPUI. |
| `reqflux_store` | The on-disk format: load, save, atomic writes, conflict detection, native export bundles. | Know about HTTP or scripts. |
| `reqflux_postman` | Postman collection v2.1 and environment import and export. | Do IO beyond reading and writing JSON values. |
| `reqflux_http` | Building and sending requests: TLS, client certificates, CA bundles, proxies, cookies, redirects, timings, auth signing. | Resolve variables or run scripts. |
| `reqflux_script` | The QuickJS sandbox, the `pm` API, bundled libraries, time and memory limits. | Send requests directly; `pm.sendRequest` goes through a host callback. |
| `reqflux_engine` | The execution pipeline, collection runner, history and app state (SQLite), keyring secrets. The single API surface for all front ends. | Depend on GPUI or rmcp. |
| `reqflux_mcp` | The MCP server: tools, resources, agent policy, audit. | Duplicate engine logic. |
| `bolt` | Bolt, the design system shared with DBFlux: tokens, theme, generic components. | Know about ReqFlux, HTTP or any product type. |
| `reqflux_ui` | Everything GPUI: theme, components, sidebar, documents, keymap. | Contain domain rules that the engine or core should own. |
| `reqflux` | The binary: GUI startup and the `mcp`, `run` and `import` subcommands. | Contain logic beyond wiring. |

`reqflux_ui` starts as one crate with modules. DBFlux split its UI into six crates and then had
to forward every feature flag through all of them; ReqFlux splits only when compile times
measurably demand it, and then by one-way layers.

## HTTP engine

- `reqwest` with rustls, HTTP/1.1 and HTTP/2, gzip, brotli, deflate and zstd.
- Clients are cached by their TLS and proxy configuration, because a client certificate, a
  CA bundle or `danger_accept_invalid_certs` is per client in `reqwest`.
- Certificates follow Postman's model: a list of client certificates matched by host and port,
  plus an optional custom CA bundle and a per-request "verify TLS" toggle.
- Timings (DNS, connect, TLS, first byte, download) come from a custom resolver and a connector
  layer. If that proves insufficient, `reqflux_http` drops to `hyper-util` directly; the crate
  boundary keeps that change local.
- Known limits of the reqwest-based transport: repeated headers with the same name are grouped
  at the position of the first one (the order among their values is kept), and the reason
  phrase is the standard one for the status code rather than the server's own text. Both are
  local to `reqflux_http`; dropping to `hyper` fixes them if they ever matter.
- Responses keep the raw bytes. Pretty-printing, JSON paths and previews are views over them.
- `reqflux_engine` owns a Tokio runtime. GPUI tasks await Tokio `JoinHandle`s, which are
  executor-agnostic futures, so no GPUI code runs on Tokio and no HTTP code runs on GPUI's
  executor.

## Command line

`reqflux` with no arguments opens the window. `reqflux run` sends one request through
`reqflux_engine`, the same path the UI and MCP use:

```text
reqflux run <request> [--workspace <dir>] [--env <slug>] [--var <key>=<value>]... [-i|--include]
```

- `<request>` is a request id or its slug path, `<collection>/<folder>/.../<request>`.
- `--workspace` defaults to the current directory. `--env` selects an environment by slug;
  without it no environment applies. `--var` sets a local variable, the highest scope, and
  repeats.
- stdout is the response body, byte for byte. `--include` writes the status line
  `HTTP/<version> <code> <reason>` and the headers first, then a blank line.
- stderr has one `warning: <message>` line per workspace load warning, the keyring warning
  below when it applies, one `warning: unresolved variable {{<name>}}` line per unresolved variable, then the summary
  `<METHOD> <url> -> <code> <reason> in <ms> ms, <size>`, with the size in `B`, `KB` or `MB`
  (1024 based). Errors print `error: <message>`.

| Exit code | Meaning |
|-----------|---------|
| 0 | A response was received, whatever its status. |
| 1 | The request could not be executed: transport error, unsupported auth, environment not allowed for the collection, invalid GraphQL variables. |
| 2 | Usage error: bad arguments, unknown request or environment, workspace that fails to load. |

Secret variables have no value in workspace files; a valueless secret resolves from the OS
keyring, and `--var` still wins over it. When the keyring cannot be read, stderr has one
`warning: secret variables were not read from the keyring: <why>` line after the load
warnings, those secrets stay unresolved and the request is still sent. A `--var` value is
visible in the process list and the shell history; prefer `reqflux secret set`, or reading it
from the environment, such as `--var token="$API_TOKEN"`.

`reqflux secret` manages the keyring values through `reqflux_engine::SecretStore`:

```text
reqflux secret set <scope> <key> [--workspace <dir>]
reqflux secret unset <scope> <key> [--workspace <dir>]
reqflux secret list [<dir>]
```

- `<scope>` is `global`, a collection slug, or `env:<slug>` for an environment; `<key>` must
  be a secret variable declared there. The directory defaults to the current one.
- `set` reads the value from stdin, never from an argument: from a terminal it prompts
  `Value for <key>: ` on stderr and reads without echo, otherwise it reads all of stdin and
  strips one trailing newline. An empty value is a usage error.
- `unset` deletes the value; a value that is not set is success.
- `list` prints `<scope>\t<key>\t<set|not set>` per secret variable on stdout, globals first,
  then collections and environments by slug. When the keyring cannot be read every line says
  `unknown`, a `warning:` line on stderr says why, and the exit code is still 0.
- No output ever contains a value. Errors print `error: <message>`.

| Exit code | Meaning |
|-----------|---------|
| 0 | Done. |
| 1 | `set` or `unset`: the keyring failed or is not available. |
| 2 | Usage error: bad arguments, unknown scope, a key that is not a secret variable in that scope, an empty value, workspace that fails to load. |

The binary tests point `DBUS_SESSION_BUS_ADDRESS` at a socket that does not exist, so on
Linux they never reach the real keyring; on macOS they would read the Keychain.

The engine API behind it: `Engine::open(root)` loads the workspace; `Engine::prepare` selects
the request, checks the environment and resolves variables, auth, body and settings into a
`reqflux_http` request without IO (`reqflux_engine::prepare` is the same step as a pure function);
`Engine::execute` sends it as a task on the engine runtime and returns an `Execution` with its
`JoinHandle` and a cancellation token. Auth types other than `inherit`, `none`, `basic`,
`bearer` and `apikey` fail with `auth type <type> is not supported yet`.
`Engine::prepare_draft` prepares a draft request document in the place of the stored one,
keeping the stored request's parents (and with them the scopes and the environment
allowlist), and `Engine::execute_draft` sends it.

`Engine::inspect` is what a front end shows before sending, against the active environment or
none: the selected request with its scripts, the URL as written with the enabled params
appended after the separator `prepare` uses, the names `prepare` reports unresolved with that
environment, and `Inspection::spans`, which splits a text into literal runs and `{{name}}`
references marked as resolving (globals, collection, environment or dynamic variables) and as
secret (secret in the globals, the collection or an environment the collection allows). When
`prepare` fails with that environment (unknown environment, environment not allowed for the
collection, unsupported auth, invalid GraphQL variables), `problem` carries its error text,
`unresolved` is empty, and the spans still resolve with the environment when the collection
allows it and without one otherwise.

## Scripting

`rquickjs` embeds QuickJS, as DBFlux already does in `dbflux_js`.

- The `pm` object mirrors Postman: `pm.environment`, `pm.collectionVariables`, `pm.globals`,
  `pm.variables`, `pm.iterationData`, `pm.request`, `pm.response`, `pm.test`, `pm.expect`,
  `pm.cookies`, `pm.info`, `pm.sendRequest`, `pm.execution.setNextRequest` and
  `pm.execution.skipRequest`.
- The legacy globals (`tests[...]`, `responseBody`, `postman.setEnvironmentVariable`) are
  supported, because many shared collections still use them.
- `require()` serves `lodash` 4.17.21, `moment` 2.30.1 and `chai` 4.5.0, vendored unchanged
  under `crates/reqflux_script/assets/` with their versions, licences and source URLs in
  `assets/LICENSES.md`, and compiled the first time a script asks for one. An unknown module
  fails with its name. The rest of Postman's set (crypto-js, uuid, ajv, cheerio, xml2js, tv4,
  csv-parse) is added when a collection needs it, so the binary carries only what is used.
- `pm.sendRequest` blocks the script until the host answers, and the engine answers it on the
  same `reqflux_http` client the request path uses, bounded by the request's own timeout. Because
  a script can wait for the network, every script runs on a dedicated blocking thread with its
  own current-thread runtime, so neither the GPU thread nor the engine's async runtime waits.
- Every run is bounded three ways, so a script in a shared collection cannot hang the app or
  exhaust the machine: a 64 MiB QuickJS heap, a 5 s wall clock enforced by the interrupt
  handler, and an 8 MiB cap on what the host itself collects (console lines, test results and
  variable writes). The host cap exists because the interrupt is polled between bytecodes and
  cannot bound a run whose time sits in host calls — a loop of `console.log` with a large
  string grew past 3 GB before it existed — so every host call also checks the deadline and
  reports which limit stopped it.
- A script never runs on the caller's thread (S14): the engine runs it on a dedicated thread
  with its own current-thread Tokio runtime, so a script that waits for the network can block
  there without blocking the GPU thread or the engine's async runtime, and the caller only
  awaits the outcome through a channel — never on the UI thread and never inside the
  engine's async runtime.

What runs today (S7): before preparing a request, the engine runs the effective pre-request
script — the request's, else the innermost folder's that has one, else the collection's — with
`pm.request` as the stored document and the merged variables of the execution's scopes, and
applies its variable writes to those scopes for this run only, never to disk. After the
response it runs the effective test script the same way, with `pm.response`. Test results and
console entries travel beside the response (`Executed::scripts`); a failing test or a script
error — reported as an error console entry — never fails the request.

`reqflux run` prints the script results to stderr before its summary line, pre-request entries
first: one line per recorded test (`test pass <name>` or `test fail <name>: <message>`) and
one line per console entry (`console <level>: <message>`, level one of `log`, `info`, `warn`,
`error`). The exit code is unchanged by a failing test. On a transport failure the
pre-request results still print, before the error line. In the app, the response area shows
the same results: `Tests` and `Console` entries beside Body and Headers, with one row per
test and one per console entry, absent when the response has nothing to show (S12). Still
later: `require()` of the bundled libraries.

## Postman compatibility

- Collection v2.1 and environment JSON, import and export.
- Fields ReqFlux does not model yet are kept in an `extensions` table on the entity and written
  back on export, so a round trip loses nothing.
- Import maps Postman items to the on-disk layout: folders to directories, request names to
  slugged file names (with a numeric suffix on collision), scripts to sidecar files.
- The engine writes the import: `reqflux_engine::import::postman` maps the documents and writes the
  collection, its folders and requests with their scripts and the environments through
  `reqflux_store`, creating the workspace manifest when the directory is empty and never
  overwriting a file that is already there.
- A fixture corpus of real exported collections drives round-trip tests: import, export and
  compare the JSON structurally.
- `reqflux import <collection.json> [--workspace <dir>] [--environment <file>]...` imports into a
  workspace directory: it creates the manifest when the directory has none, writes the
  collection and its environments, prints what it wrote, and refuses to run when a target file
  already exists, so an import never overwrites existing work.

## Export and import

| Unit | Native bundle | Postman |
|------|---------------|---------|
| Workspace | Yes | No (Postman has no portable workspace format) |
| Collection | Yes | Collection v2.1 |
| Folder | Yes | Collection v2.1 containing that folder |
| Request | Yes | Collection v2.1 containing that request |
| Environment | Yes | Environment JSON |

A native bundle is a single versioned file (`format_version`, `reqflux_version`, kind, payload).
Secrets are excluded by default. Including them requires either an age passphrase, which
encrypts only the secrets section, or an explicit plaintext flag. This is the DBFlux
portability model, which held up well.

Because the workspace is already a directory of text files, sharing a whole workspace through
git needs no export at all.

## MCP

`reqflux mcp [--workspace <path>] [--app-id <id>]` runs an rmcp stdio server with two groups of
tools: engine tools that read and change a workspace through `reqflux_engine`, and the `ui_*`
tools of "UI automation". Each group has its own `#[tool_router]`; the server serves their sum.

Engine tools (`reqflux_mcp::engine_tools`) work on one open workspace per session: the one
`--workspace` names (one that does not load exits 2), else the current directory when it holds
a `reqflux.toml`, else none until `workspace_open` or `workspace_create`. Every read reloads the
workspace first, so it sees what the window or `git` wrote. The tools: `workspace_open`,
`workspace_create`, `workspace_info` (name, id, path, warnings and the flat tree);
`collection_create`, `folder_create`, `request_create`, `item_rename`, `item_duplicate`,
`item_move`, `item_delete` (the tree operations, by id); `request_get` and `request_update`
(the request document as TOML; `request_get` returns the `base_hash` of the file it read and
`request_update` requires it and saves hash-checked against it, so an edit made after the
agent's read is a conflict whatever other tools reloaded in between);
`request_execute` (status, headers, body text cut at 256 KiB, time, size, unresolved variables
and test results; it records no history and keeps no script writes); `history_list` and
`history_get` (the window's history of this workspace, read-only); `environment_list`,
`environment_create`, `environment_rename`, `environment_delete`; `variables_get`,
`variable_set` and `variable_delete` by scope `global`, `collection:<id>` or `environment:<id>`.
The handlers run on the blocking pool under one lock; `request_execute` holds it for the whole
send, and a tool that panicked does not poison it for the session. The engine, which owns a
Tokio runtime, is dropped on its own thread, never inside the async server.

Secrets never leave the engine. A secret variable shows `{"set": true|false}`, never a value.
Basic passwords, bearer tokens, API key values, every field of an unsupported auth type and
the values of `Authorization`, `Proxy-Authorization` and `Cookie` headers show as `••••••••`
in `request_get` and history documents; a `request_update` that sends the mask back for such a
field or header keeps the stored value, and never restores it into another auth type. An
executed response's headers, body and error text go through the send's masker. History output
(the recorded URL, headers, bodies and error text) is masked again with every secret of the
workspace now (`Engine::workspace_masker`: the secret variables of the globals, every
collection and every environment, and the literal auth credentials), credential header values
(`Set-Cookie` included) whole; when the keyring cannot be read, `history_get` leaves out the
response body and header values and says why. A load or parse error names the file, line,
column and the parser's message, never the source line, which can hold a literal secret; this
holds for `reqflux run` and the window too.

For the `ui_*` tools: `reqflux_mcp` reads the bridge endpoint descriptors under the runtime directory
and speaks the framed wire protocol to one window (`reqflux_mcp::bridge`), and serves thirteen
`ui_*` tools over stdio (`reqflux_mcp::server`). A session talks to the only live target, asks
for `ui_select_target` when several are running, and reports a target that died instead of
switching. Stdout carries only protocol frames; logs go to stderr. Text the bridge withholds
as redacted stays withheld, and a bridge failure is a tool error, never a panic.

Not yet: policy, approvals and audit. Every tool runs unpoliced and the help text says so. For
the `ui_*` tools that adds no exposure the bridge does not already have (see the trust model
below). The engine tools can change workspace files and send requests, with the stored
credentials, to any URL the agent writes: the same as a shell with `reqflux run`, and what the
host allowlist and approvals below are for.

- **More tools**: runner (run a collection or folder), import and export.
- **Resources**: requests, collections and environments addressed by id, so an agent can read
  them without a tool call.
- **Secrets never leave the engine.** Tools return `{{token}}`, not the token. Execution
  resolves secrets inside the engine, and response output redacts any secret value it finds
  echoed back.
- **Policy** lives in the workspace manifest and the user's settings: whether agents may write
  files, which environments they may execute against, and a host allowlist. Each tool has an
  execution class (read, write, execute, destructive), mapped to allow, ask or deny.
- **Approvals.** With the GUI running, `ask` shows an approval prompt in the app over a local
  socket. Headless, `ask` resolves to deny. An agent can never approve its own call.
- **Audit.** Every write and execution is appended to a per-user audit log, with secrets
  redacted.

The GUI and `reqflux mcp` coordinate through the files themselves: atomic writes and
hash-checked saves with conflict detection. There is no second runtime that has to agree with the GUI about a
database, which was DBFlux's most expensive structural mistake.

## UI

What exists today: `reqflux [<dir>]` opens a workspace through `reqflux_engine` before the window
appears (a slow disk delays the window) and records it as recent. The title bar's workspace
selector (`workspace-selector`) opens a menu (`workspace-menu`) with the recent workspaces
(`workspace-recent-<n>`, name and path, the current one marked; `workspace-recent-remove-<n>`
drops one), `Open folder…` (`workspace-open`, the OS folder dialog), `New workspace…`
(`workspace-new`, the OS save dialog; `reqflux_engine::create_workspace` writes a
`reqflux.toml` named after the directory into a missing or empty directory) and
`Close workspace` (`workspace-close`). Switching cancels sends in flight, closes every tab,
resets the active environment and loads the new workspace's drafts and current values; the
previous workspace's drafts stay in the app state. A workspace that fails to load keeps the
current one and shows why (`workspace-error`). With no workspace the document island shows
`Open folder…` (`empty-open`), `New workspace…` (`empty-new`) and the recent list
(`empty-recent-<n>`). One window shows one workspace. The window is the Bolt shell of
`design/bolt-v2/01-main.html`: a title bar with the workspace and environment on the left,
the command search in the middle and the approvals bell on the right; the activity rail
(Collections, History, Environments, MCP approvals, Audit log, Settings); a sidebar island with
the collection tree (icons, indent guides, child counts, method labels in Bolt's semantic
colors) and a request and collection count; a document island; and a status bar that shows
the last send (`<METHOD> <path> · <status> · <time>`, or `sending` or `failed`) and
load warnings. In the tree the arrow keys move the selection, Enter or the chevron toggles a
folder, and a click or Enter on a request opens it in a document tab, or focuses its tab.
The sidebar header's `+` (`sidebar-new-collection`) creates a collection. A right click on a
row, or its `…` button (`tree-row-menu-<id>`), opens a menu (`tree-menu-new-request`,
`tree-menu-new-folder` on collections and folders, `tree-menu-rename`, `tree-menu-duplicate`
off collections, `tree-menu-delete`). Rename is a field over the row name
(`tree-rename-input`; Enter commits, Escape cancels), and a new item opens straight into it.
Delete asks first (`tree-delete-prompt`, naming the item and how many requests it holds;
`tree-delete-confirm`, `tree-delete-cancel`), then closes the tabs and drops the drafts of
every deleted request. Alt+Up and Alt+Down move the selected request or folder among its
siblings; dragging a request or folder row drops it before another row, or inside a
collection or folder when dropped on its name (`tree-name-<id>`). A refused operation shows
its reason in `sidebar-error`; while a request is being sent, the tree is not changed and the
error asks to wait. Open tabs and drafts follow a rename or move, since ids are stable.
The environment button of the title bar opens a menu with `No environment` and the workspace
environments by name; choosing one makes it the window's active environment, and Escape or a
click outside closes the menu without a change. Icons are Lucide, from `gpui-kit-assets`,
installed by the binary as the application's asset source.

The document island shows the open requests as tabs, the path bar of the active one
(collection, folders, request), and the request, editable as "Drafts and saving" describes:
an action strip (Save and Revert, live while the request has a draft; Undo, Redo and Split,
not built yet), the request line (method, URL with the
enabled params, Send), and the Params, Headers, Body, Auth,
Scripts and Settings sub-tabs. What it shows comes from `Engine::inspect` with the window's
active environment, which lives in memory, starts as none and re-inspects every open request
when it changes, keeping its sub-tab: variables are styled as resolved or unresolved, the
number of unresolved ones is shown in the sub-tab bar, or the inspection `problem` in its
place (`request-problem`), and a value that references a secret, a password, a token, an API key value or
any field of an auth type the engine does not support is masked as `••••••••` on screen and
in the automation tree.

Send or Ctrl+Enter sends the active request with the active environment through
`Engine::execute`. The UI awaits the execution's `JoinHandle` on GPUI's executor with
`cx.spawn` and never calls `Engine::wait`, which blocks. While it is in flight, Send turns into
Cancel; Cancel or Escape cancels it through its token, sending again cancels and replaces it,
and closing the tab cancels it and drops its response. The response sits under the request:
the status chip colored by class, the time and size, Body and Headers sub-tabs, a
Pretty/Raw toggle (Pretty re-indents JSON keeping key order), the body split into lines once
off the UI thread and drawn with `uniform_list` so only visible lines render, an error banner
with Retry, and the unresolved variables the send used. Each open request keeps its last
response in memory, and every send is recorded in history (see "History").

An image or HTML response (by `Content-Type`, then PNG, JPEG, GIF or WebP magic bytes) adds
`Preview` (`response-view-preview`). An image is decoded off the UI thread and drawn at its
natural size, shrunk to fit and never enlarged, with its size and format
(`response-preview-meta`); one that does not decode says so. GPUI has no web view, so HTML
shows its title and readable text (tags stripped, scripts and styles dropped) and
`Open in browser` (`response-preview-open`), which writes the body to a new file under
`$XDG_CACHE_HOME/reqflux/preview/` and opens it with the system handler; ReqFlux never fetches
anything to render a page. An image over 20 MB or HTML over 5 MB is too large to preview. The
Cookies sub-tab (`response-tab-cookies`) parses every `Set-Cookie` header into name, value and
attributes (`response-cookie-<n>`), raw with a warning mark when it does not parse; nothing is
stored, since the cookie jar comes later. History stores `Set-Cookie` values masked whole
(see "History").

The rail's History button (`rail-history`) swaps the sidebar tree for the History panel
(`history-row-<n>`, newest first, `METHOD path · status · time ago` or `failed`; the 200
newest; `No requests sent yet` when empty), and Collections (`rail-collections`) swaps back.
Clicking a row opens its request's tab and shows the recorded response under
`From history · <time>` (`response-history-label`) without touching the request or its draft;
`Resend` (`history-resend`) sends the recorded document again through `execute_draft` and
records the result as a new entry. A row whose request was deleted opens a read-only view
(`history-readonly`) with the recorded method, URL and response, and Resend disabled.

The rail's Environments button (`rail-environments`) swaps the sidebar for the Variables panel
(`variables-panel`): Globals (`variables-scope-globals`), each collection
(`variables-scope-collection-<id>`) and each environment (`variables-scope-environment-<id>`).
Its `+` (`variables-new-environment`) creates an environment and opens its name into a field
(`variables-rename-input`); an environment's `…` menu renames or deletes it, and delete asks
first (`variables-delete-prompt`; deleting the active environment selects No environment).
Selecting a scope shows its variables in the document island (`variables-document`) as a grid,
row `variables-row-<n>`: enable box, key, shared value, current value, secret toggle,
description and delete, plus `Add` (`variables-add`, key `new_variable`). Variables are not
drafts: every commit writes the file (or the keyring, or the app state for a current value) at
once and re-inspects open requests. A secret's value shows `••••••••` and edits through an empty
field whose empty commit keeps it; its current value is the same keyring value. Turning a
secret off does not copy its value into the file, and the panel says so
(`variables-warning`). Current values a script wrote for a key the scope does not declare show
as `variables-undeclared-<n>` rows that can be edited or cleared; renaming a key leaves its
current value on the old key, where it shows as such a row. A refused write shows its reason in
`variables-error` and changes nothing.

Ctrl+K, Ctrl+P or a click on the title bar's search (`command-search-button`) opens the command
palette (`palette`, input `palette-input`, rows `palette-row-<n>`). An empty query lists the
commands; typing searches requests (by `METHOD name` and `METHOD url` as written) and commands
together with a fuzzy subsequence score that favors runs and word starts. Up and Down move the
highlight, Enter or a click runs it, Escape or a click outside closes it. One registry
(`shell/commands.rs`) defines every command with its label, key, key context and when it can
run; the palette, the key bindings and a coverage test (every command has a key, no key is
shared in a context) all read it. A command that cannot run is listed dimmed and its key does
nothing. The window gives the shell the first focus, so keys work before any click; a text
field takes keys first. The keymap is not configurable yet.

| Command | Key | Can run when |
|---|---|---|
| Open palette | Ctrl+K, Ctrl+P | always |
| Send request, Cancel send | Ctrl+Enter, Escape | a request is open |
| Save request, Revert request | Ctrl+S, Ctrl+Shift+R | the active request has a draft |
| New request | Ctrl+N | a collection exists (in the selected collection or folder) |
| New collection, New environment | Ctrl+Shift+N, Ctrl+Shift+E | a workspace is open |
| Switch environment | Ctrl+E | a workspace is open |
| Open workspace, New workspace | Ctrl+O, Ctrl+Shift+O | always |
| Close workspace | Ctrl+Shift+W | a workspace is open |
| Close tab, Next tab, Previous tab | Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab | a request is open |
| Show Collections, History, Variables | Ctrl+1, Ctrl+2, Ctrl+3 | a workspace is open |

Every region and tree row has a stable element id (`title-bar`, `sidebar`, `document`,
`status-bar`, `tree-row-<id>`, `doc-tab-<id>`, `request-tab-<name>`,
`request-<params|headers>-row-<n>`, `request-send`, `request-cancel`, `response-status`,
`response-meta`, `response-body-line-<n>`, `response-headers-row-<n>`, `response-error`,
`response-retry`, `status-last-request`), and text that automation must read carries a label role,
because the gpui-mcp bridge only reports text for nodes with a text role. Tree rows report
`selected` and `expanded`; tabs and sub-tabs report `selected`. On Wayland a window renders
and handles input only while its workspace is on screen, so automation needs the window
visible.

The plan:

- Sidebar tree: workspace, collections, folders, requests, environments.
- Documents in tabs: request editor, collection and folder settings, environment editor,
  runner results.
- Request editor: method and URL bar, then tabs for params, headers, body, auth, scripts and
  settings.
- Response view: body (pretty, raw, preview), headers, cookies, timings, test results and
  script console.
- Keyboard: GPUI `KeyBinding`s with context predicates, a command palette, and a test
  that every action reachable by pointer is also bound to a key.
- Theming through Bolt (see "Design system"), with a test that rejects raw colors and bare
  pixel values outside the token files.

## Design system

ReqFlux and DBFlux are a family of apps: both use **Bolt** (islands, 45 degree cuts, Archivo and
JetBrains Mono, compact density) with one accent per product. `design/system.md` holds the
values; `design/bolt-v2/` holds the artboards of every view, built from DBFlux's own.

Bolt is copied from DBFlux for now. The goal is to make the later move to a shared library a
dependency swap, so the copy lives behind a boundary from the start:

- **`crates/bolt`** owns everything product-agnostic: color roles and washes, typography roles,
  spacing, cuts and chrome metrics, the chamfer shape, the theme mapping onto gpui-component,
  and generic components (island, button, input, tabs, tree row, table, keycap, status chip).
  It depends on GPUI and gpui-component only and never names ReqFlux, HTTP or a ReqFlux type.
- **ReqFlux passes in what differs:** a `Brand` value (accent fill, hover, pressed, tint, app
  name, logo) and its own extra tokens (`method-*`, `draft`), defined in `reqflux_ui`.
- **`reqflux_ui` never reads gpui-component `Theme` fields or raw colors directly.** It asks
  `bolt` for a role (`text strong`, `panel`, `accent tint`) or uses a `bolt` component.
  A guardrail test rejects color literals and bare pixel values outside the token files, as
  DBFlux does.
- Extraction then means moving `crates/bolt` to its own repository, depending on it from
  both products, and deleting DBFlux's copy. Bolt values that change in the meantime are
  changed in both products or recorded as a divergence in `design/system.md`.

## UI automation

Every window installs the [`gpui-mcp`](https://github.com/themixednuts/gpui-mcp) bridge. It is
always on, not a development feature: agents drive the running app through the same `reqflux mcp`
server they use for collections and requests, so a client declares one MCP server, not two.

```text
agent ── stdio (MCP) ──> reqflux mcp ──┬── reqflux_engine          collections, requests, execution
                                     └── gpui-mcp-protocol ──> bridge in each ReqFlux window
```

- `reqflux_mcp` speaks `gpui-mcp-protocol` to the bridge directly: endpoint discovery, the token
  handshake and operations such as reading the element tree, focusing, typing, pressing keys
  and clicking. There is no second process; `gpui-mcp-server` is upstream's own client and
  ReqFlux does not ship it.
- UI tools are named `ui_*` and go through the same policy as every other tool. Reading the
  tree is class `read`; any input is class `execute`, because a click on Send executes a
  request just as `request_execute` does. Without that, the UI would be a way around the policy.
- With several ReqFlux windows or processes, `ui_list_targets` lists them and `ui_select_target`
  chooses one; a single target is selected automatically.
- Screenshots need native window capture, which on Linux only works for X11 windows. They are
  a later addition through `gpui-mcp-capture`; the element tree and input work on Wayland.

Trust model, and what it costs:

- The bridge listens on an owner-only socket under `$XDG_RUNTIME_DIR/gpui-mcp/` and writes a
  descriptor with a random token. Any process running as the same user can read that token and
  drive the app or read what it renders, with no policy in between; only `reqflux mcp` enforces
  the policy. This is the same exposure the user's files and keyring already have to such a
  process, and it is the price of having automation always on.
- Secret values must never be rendered in clear text by default. Secret inputs are masked, and
  the bridge withholds masked and `frame_redacted` text from observers.
- The bridge, the patched `gpui-pre` and `gpui-mcp-protocol` all come from one `gpui-mcp`
  rev, set once in the root `Cargo.toml`. Bump it for all of them at once.
- `gpui-mcp` supports `gpui-pre` 0.3.5 to 0.3.7, which caps ReqFlux at `gpui-component` 0.7.0
  until upstream supports a newer release.

`reqflux mcp` serves these tools itself: `ui_list_targets`, `ui_select_target`, `ui_ping`,
`ui_tree`, `ui_find`, `ui_element`, `ui_element_bounds`, `ui_wait_for_element`, `ui_click`,
`ui_type`, `ui_key`, `ui_focus` and `ui_scroll`. ReqFlux reimplements discovery, descriptor
validation and the framing instead of depending on upstream's server, whose client is private,
so `scripts/gpui-mcp.sh` is no longer needed to drive the app.

The protocol crate gives the types; the transport, the size caps (1 MiB per request, 24 MiB
per response) and the 10 s I/O timeout are ReqFlux's. A request carries the descriptor's token
and a fresh request id, and a response must match both the id and the protocol version.

## Packaging and releases

The flake builds a Linux package (`packages.reqflux`, aliased by `packages.default`) with
`rustPlatform.buildRustPackage`: the `reqflux` binary wrapped with the GPUI runtime
library path, the desktop entry (`assets/icon/reqflux.desktop`) and the icon set
installed under the standard XDG paths (`share/applications`, `share/icons/hicolor`).
`nix develop` is unchanged apart from ImageMagick, which is listed so the icon PNGs can
be regenerated offline.

The icon is a hand-written SVG (`assets/icon/reqflux.svg`, a paper plane on the brand
cobalt `#2B45B8`; `reqflux-small.svg` is a filled variant for 16 and 32 px). The PNGs are
committed and regenerated with ImageMagick's librsvg delegate, which works from the dev
shell offline:

```sh
magick -background none assets/icon/reqflux.svg -depth 8 -resize 256x256 assets/icon/reqflux-256.png
```

Every push to `main` (and manual dispatch) runs `.github/workflows/nightly.yml`, which
builds `reqflux` on `ubuntu-latest` and publishes a prerelease tagged `nightly-YYYYMMDD`
with `reqflux-linux-amd64.tar.gz` attached. The nightly version is the workspace version
with its pre-release suffix stripped (`0.1.0-dev.0` → `0.1.0`) plus the UTC date and
short commit as semver build metadata, e.g. `0.1.0-nightly+20260213.928b3d5`. The date
also forms the tag, so pushes to `main` on the same day replace that day's release, and
older `nightly-*` releases are deleted, leaving one rolling nightly — the same
behaviour as DBFlux's nightly.

## Lessons from DBFlux

| Keep | Evidence in DBFlux |
|------|--------------------|
| Capability seams so the UI never branches on a backend id. In ReqFlux: auth types and body modes. | `AGENTS.md` "never branch on driver_id" |
| GPUI-native keymap with context predicates and a keyboard coverage test. | `dbflux_ui_base/src/keymap/mod.rs` |
| Semantic theme tokens with a guardrail test against raw colors. | `DESIGN.md`, `dbflux_components/src/theme.rs` |
| Versioned export bundle with age-encrypted secrets and an explicit plaintext flag. | `dbflux_portability/src/bundle.rs` |
| Execution classes, human-only approvals and redacted audit for agent actions. | `docs/MCP_AI_INTEGRATION.md` |
| Strict workspace lints (no `unwrap`, `expect`, `panic`, indexing outside tests). | root `Cargo.toml`, `clippy.toml` |
| Secrets in the OS keyring, references in storage. | `dbflux_core/Cargo.toml` (`keyring`) |
| A separate data directory per release channel, so a nightly migration cannot damage stable data. | `ARCHITECTURE.md` storage section |

| Change | Why |
|--------|-----|
| User content in text files instead of one SQLite file. | Collections must diff, merge and be reviewed in git. |
| MCP as a thin layer over the shared engine, in one crate. | DBFlux runs a second driver runtime in the MCP process, coordinates through a shared SQLite file and splits MCP across two overlapping crates. |
| One UI crate to start. | Six UI crates forced feature forwarding through every one of them and spread the keymap over three crates. |
| Vendor nothing. | DBFlux carries four patched vendored crates, each with its own refresh procedure. ReqFlux uses published crates plus one git patch of `gpui-pre` from the `gpui-mcp` repository, which upstream maintains. |
| A hard 1000-line limit on source files from day one, with no grandfathering. | DBFlux carries 2,000 to 3,900 line `lib.rs` and `mod.rs` files. |
| Persistence never depends on governance crates. | `dbflux_storage` depends on `dbflux_approval`. |
| Docs updated in the same change as the code they describe. | DBFlux still documents a removed hand-rolled MCP transport. |

## Roadmap

| Phase | Outcome |
|-------|---------|
| 0 Foundation | This document, a compiling workspace, dev shell, lint and CI. |
| 1 Model and store | `reqflux_core` types and templating, `reqflux_store` load and save with round-trip tests. |
| 2 HTTP | `reqflux_http` with basic, bearer and API key auth; `reqflux run` sends a request from the CLI. |
| 3 UI MVP | Sidebar, request editor, response view, environment switcher, and `reqflux mcp` with the `ui_*` tools so agents verify the UI from here on. |
| 4 Postman | v2.1 collection and environment import and export against a fixture corpus. |
| 5 Scripts | `pm.*` variables, tests and expectations, then `pm.sendRequest` and bundled libraries. Required for v1. |
| 6 MCP | Workspace, collection, request and runner tools, resources, policy, approvals and audit. |
| 7 Depth | Client certificates, proxies, cookies, digest, OAuth 2, AWS v4, collection runner, native bundles. |

FEATURES.md assigns each feature to MVP, v1 or later. Phases 1 and 2 are the foundation everything else stands on; the UI starts once a request can
be loaded from disk and sent from the CLI.
