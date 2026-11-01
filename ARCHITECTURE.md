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
| GPUI | DBFlux's GPUI stack: `gpui-pre` 0.3.9, `gpui-component` 0.7.3, `gpui-pre-platform` 0.3.9 and `gpui-kit-assets` 0.7.3; every other dependency tracks its latest release. `gpui-pre`, `gpui-base`, `gpui-component` and `gpui-pre-linux` are patched, and `gpui-mcp` and `gpui-mcp-protocol` taken, from the DBFlux git repository at one pinned rev; nothing is vendored here. |
| Crates | Nine crates in a strict one-way dependency order. Only `reqflux_ui` and `reqflux` link GPUI. |
| Design system | DBFlux's `bolt` crate, taken by git at the same DBFlux rev as the GPUI stack, with ReqFlux's brand. The theme is DBFlux's palette with ReqFlux's accent. |
| UI automation | The upstream `gpui-mcp` bridge is always installed. `reqflux mcp` exposes it as `ui_*` tools beside the engine tools, so agents declare one MCP server; the policy covers both, and its `ask` is a prompt in the window. |
| User configuration | `~/.config/reqflux/config.toml` (theme, MCP policy) belongs to the user, not to a workspace; a workspace can only restrict the MCP policy. |

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

A workspace is a directory, wherever it lives. Workspaces the app keeps are under
`$XDG_DATA_HOME/reqflux/workspaces/<workspace id>/` (`~/.local/share/reqflux/workspaces/` when
the variable is unset), named by id so a rename never moves files
(`reqflux_engine::create_managed_workspace`); any other directory, such as one inside a
repository, opens the same way. Both kinds use this format, so a later sync between machines
works on the same files.

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
- A draft payload is the request document as TOML. When the draft edits scripts, it adds a
  `[reqflux_draft_scripts]` table: `pre_request`, `test`, and the sidecar hashes
  `pre_request_base` / `test_base` the scripts were made from. A payload without that table
  means the scripts are as saved. Saving checks the request and both sidecar hashes first,
  then writes the sidecars (an empty script deletes its file) and the .req.toml if its
  document changed (`Engine::save_draft`).
- **Sending** a request with a draft sends the draft. History records what was sent. When the
  draft edits scripts, the send runs the draft's unsaved scripts in place of the request's
  sidecars (`Engine::execute_draft_with_scripts`); an empty or absent draft script falls back
  to the folder's or collection's, as an absent sidecar does. Resend of a history entry runs
  the saved scripts.

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
masked: header values (`Authorization`, `Proxy-Authorization` and `Cookie` masked whole),
the reason phrase and a UTF-8 text body. A `Set-Cookie` is stored as received, apart from a
secret value inside it, as Postman keeps cookies: it is the user's own data in per-user state,
so a session cookie sits in the state file in clear. A body that is not UTF-8 is stored as received, since the
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
(`request-url-input`) at the same position as the text it replaces; typing stores a draft on
every keystroke, Enter commits it to the draft and sends the request, and Escape drops what
was typed. The URL's query string and the Params grid are one list (`reqflux_core::query`):
the draft keeps the base URL and the params as rows, a query typed in the field becomes rows
(merged into the rows the request had when the field opened, so order, descriptions and
disabled or blank rows survive), and the line shows the base plus the params that are sent.
A row with an empty key and an empty value is never sent, and params go before a `#fragment`.
A row with an empty value shows as its bare key (`?b`); the request sends `b=`. While the URL
field is open, a grid change (a row toggled, edited or deleted) re-seeds it in place, keeping
the cursor; Escape then returns to that point.
The response pane and the split appear after the request's first send in the session, or at
once when its history has a send (the newest one, masked as recorded); before that the request
uses the whole height. In the Params and Headers grids, each row carries an enable box (`request-<params|headers>-enabled-<n>`) whose click toggles the row's `enabled`
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
                                bolt (DBFlux, git) ──────┘   (design system, no ReqFlux types)
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
- `reqflux_http` follows redirects itself (at most 10; `reqwest`'s policy is off), because a
  `reqwest` policy can follow or stop but cannot change the next request's headers. 301, 302
  and 303 continue as GET without a body (HEAD stays HEAD); 307 and 308 resend the method and
  the body, except a multipart or file body, which is streamed once, so that redirect is
  returned as the response. The target query is the `Location`'s own; the original URL's query
  is never copied. The timeout bounds the whole chain and the response reports the final URL.
- A hop to another origin (scheme, host or port) drops `Authorization`, `Cookie`,
  `Proxy-Authorization` and `WWW-Authenticate`, plus every header, `Referer` included, whose
  name or value holds a secret of the send. The engine passes the send's `Masker` as that
  predicate for the main request and every script `pm.sendRequest` in every front end, with or
  without a host guard. A 307 or 308 to another origin whose body holds a secret is not
  followed: the send fails with `a redirect to <target, masked> was not followed: the request
  body holds a secret`. A same-origin hop keeps every header.
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
- `reqflux_engine::postman_preview` runs the same mapping and collision checks and returns the
  counts and the files the import would write, writing nothing; the app's import dialog shows it
  before it imports.
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
whole, and a `Set-Cookie` with its cookie value masked but its name and attributes shown
(`Masker::header_for_agent`; `request_execute` output does the same); when the keyring cannot be read, `history_get` leaves out the
response body and header values and says why. A load or parse error names the file, line,
column and the parser's message, never the source line, which can hold a literal secret; this
holds for `reqflux run` and the window too.

For the `ui_*` tools: `reqflux_mcp` reads the bridge endpoint descriptors under the runtime directory
and speaks the framed wire protocol to one window (`reqflux_mcp::bridge`), and serves thirteen
`ui_*` tools over stdio (`reqflux_mcp::server`). A session talks to the only live target, asks
for `ui_select_target` when several are running, and reports a target that died instead of
switching. Stdout carries only protocol frames; logs go to stderr. Text the bridge withholds
as redacted stays withheld, and a bridge failure is a tool error, never a panic.

### Configuration and policy

The user's configuration is `$XDG_CONFIG_HOME/reqflux/config.toml` (`~/.config/reqflux/config.toml`
when unset; `reqflux_engine::config`). It holds `theme` (`dark`, `light` or `system`, written by
`Toggle theme` with a format-preserving editor that keeps comments, order and unknown keys),
`font_size` (the interface font size in px, 13 by default, clamped to 8..=32 as in DBFlux) and
`[mcp]`. `reqflux_engine::config` edits `font_size`, the `[mcp]` rules and the
`hosts`/`environments` lists the same way (`set_font_size`, `set_rule`, `add_list_entry`,
`remove_list_entry`); removing the last list entry removes the key (no list means any). A missing file is the defaults; a file that does not parse is reported (path, line and
column, never the source line) as an error in the window's Problems panel and by `reqflux mcp`,
and the defaults apply. Unknown keys under `[mcp]` and `[mcp.workspaces."<id>"]`, and a key that is not a
workspace id, are reported as warnings the same way; they change no rule.

```toml
[mcp]
read = "allow"          # the defaults: read allow, everything else "ask"
write = "allow"
execute = "allow"
destructive = "ask"
hosts = ["api.example.com", "*.example.com", "localhost:8080"]   # absent: any host
environments = ["dev", "staging"]                                  # absent: any environment

[mcp.workspaces."01J00000000000000000000900"]
destructive = "deny"
```

The engine tools are policed (`reqflux_engine::policy`); every tool has a class: read
(`workspace_open`, `workspace_info`, `request_get`, `history_list`, `history_get`,
`environment_list`, `variables_get`), write (`workspace_create`, `collection_create`,
`folder_create`, `request_create`, `item_rename`, `item_duplicate`, `item_move`, `request_update`,
`environment_create`, `environment_rename`, `variable_set`), execute (`request_execute`) and
destructive (`item_delete`, `environment_delete`, `variable_delete`). Each class is `allow`,
`ask` or `deny`; `ask` asks the running window (below). The effective rule is the strictest
of the user's `[mcp]`, the matching `[mcp.workspaces."<id>"]` (the id in any case) and the open
workspace's `reqflux.toml` `[mcp]`, and the effective `hosts` and `environments` are what every
list given allows: a workspace, even a cloned repository, can only restrict what the user
allows. `workspace_open` checks `read` against the target workspace before returning anything
from it. The check runs before any file is read or written and before anything is sent, and the
config file is read again on every call, so an edit applies without a restart.

`hosts` is checked against the request as prepared (variables resolved): the main request, every
`pm.sendRequest` a pre-request or test script makes (`reqflux_engine::HostGuard`), and every
redirect target before it is followed (`reqflux_http::RedirectCheck`). A refused script send
reaches the script as a transport error; a refusal in a pre-request script or on the main
request's redirects refuses the call. Entries are exact hosts, `*.example.com` for subdomains
only (not the apex) and `host:port` for one port, case-insensitive. A refusal names the tool, the
class or the host, the rule and the file and table to change it in.

Every write, execute and destructive call, allowed or refused, appends one JSON line to
`$XDG_STATE_HOME/reqflux/audit.log` (`reqflux_engine::audit`; owner-only, appended and synced per
line): time, tool, class, decision, reason, workspace id, the ids it targets and, for an
execution, the method and the masked URL. Argument values (documents, variable values, bodies)
are never written, and a call whose audit line cannot be written is refused. Read tools are not
audited.

The `ui_*` tools are policed too: `ui_tree`, `ui_find`, `ui_element`, `ui_element_bounds` and
`ui_wait_for_element` are read; `ui_click`, `ui_type`, `ui_key`, `ui_focus` and `ui_scroll` are
execute, since a click on Send executes a request. `ui_list_targets`, `ui_select_target` and
`ui_ping` are not. `hosts` and `environments` do not apply to them; an execute-class ui call is
audited with the target id, never the typed text or keys.

#### Approvals

`ask` is a question to the user in the running window (`reqflux_engine::approvals`). The window
listens on `$XDG_RUNTIME_DIR/reqflux/approvals-<pid>.sock` (directory 0700, socket and its
`approvals-<pid>.json` descriptor 0600; peers of another user are refused) for one
newline-delimited JSON exchange per call: tool, class, workspace, target names and, for an
execution, the method and the masked URL from a prepare that runs no script, never argument
values. Optional fields added within protocol version 1 carry the MCP client's name (its
`initialize` `clientInfo` title, else its name), the ids the call targets, the environment
as the agent named it, the rule that asked (`execute → approval`, or
`execute in <workspace> → approval` when only that workspace's override or manifest asks) and
when it asked; a request without them still parses. The window reads everything else from
its own open workspace by id through `reqflux_engine::approvals::preview`, which prepares the
request with the named environment without running scripts or sending anything and masks the
URL, headers and body with that preparation's masker; a secret variable's value is never
read into it. The detail is read when the call arrives and again on Refresh. There is no modal: a request opens nothing, takes no focus and does not raise the
window. The count shows as a badge on the title bar bell (`approvals-badge`) and on the rail's
`rail-approvals` (`rail-approvals-count`), only when N > 0; the status bar does not show it. The MCP
approvals view (`approvals-view`, `shell/approvals_view.rs` and `shell/approvals_detail.rs`,
artboard `design/bolt-v2/12-agent-approval.html`) opens from the rail button, the bell, the
palette ("Open MCP approvals") and Ctrl+Shift+M; while `Panel::Approvals` is the
rail's view the sidebar keeps the collection tree and the view takes the document island. Its
tab row shows the open requests and the active `MCP approvals` tab (`doc-tab-approvals`;
`doc-tab-approvals-close`, a request tab or a tree row goes back). The header shows
`approvals-title` ("MCP approvals"), `approvals-subtitle` ("agent calls that a policy parked
for a person") and `approvals-refresh`. On the left, `approvals-list`: the header
`approvals-pending-label` ("Pending · N"), a row per call longest wait first
(`approvals-row-<n>`: the tool in mono with its class badge `approvals-row-<n>-class`; the
method and path, or the target breadcrumb for a call that sends nothing, with the environment,
`approvals-row-<n>-request`; "<client> · waiting just now / N min / N h",
`approvals-row-<n>-meta`, the client "an agent" when it gave no name) and the footer
`approvals-footer` ("j k move · decisions go to the audit log"). On the right, the selected
call: `approval-title` (the tool), `approval-status` ("Pending") and `approval-call-id`; the
fields `approval-client` (Requested by), `approval-environment`, `approval-class` (Policy
class), `approval-tool`, `approval-policy` and `approval-request` (the target as
`collection › folder › name` from the open workspace, else the names the request carries).
For `request_execute`, "What it will send" (`approval-sends`): `approval-method`,
`approval-url-template` with its variables highlighted, `approval-url` (resolved, masked),
`approval-resolved-in`, the headers `approval-header-<n>` (a credential header masked whole)
and `approval-body` (the first lines); then "Variables it uses" (`approval-variables`, rows
`approval-variable-<n>`: variable, value, source; a secret shows `••••••••` with the SECRET
badge and `keyring`). When the call works on another workspace than the open one, or its
request cannot be read, the box shows `approval-unavailable` (why) with the method, masked URL
and `approval-targets` the request carries. Then the reason field `approval-reason`, `approval-deny` (`d`),
`approval-allow-session` (`s`) and `approval-allow` (`a`), each with its key on a keycap,
and the footnote `approval-footnote`; with nothing pending,
`approvals-empty`. With the list focused `j`/`k` and the arrows move, `a` allows once, `s`
allows for this session and `d` denies; the reason field sits outside the list, so typing
never answers. Escape answers
nothing. Answering a call selects the next. A denial carries the trimmed reason in the
response's optional `reason` (protocol version 1); `reqflux mcp` strips control characters,
cuts it to 200 characters and refuses with "denied in the window: <reason>". "For
this session" covers the class and workspace until that `reqflux mcp` exits. No live window
(no runtime dir, no descriptor of a live pid), no answer within 120 s, or a dropped connection
refuses the call; a client that gives up withdraws its request from the list. The audit line
records `approved once`, `approved for this session`, `denied in the window` (with the reason),
the timeout or `no window is running to approve it`.

An agent can never approve its own call. The gpui-mcp bridge has no input hook and gpui does not
expose whether an event is synthetic, so while any approval is pending the window shuts its
bridge down (every `ui_*` call then refuses with "an approval is pending in the window; ui tools
are paused until it is answered") and installs it again, with a new descriptor and token, when
the queue empties. `reqflux mcp` follows its selected window to the new bridge by app id and
pid, choosing the target of that pid that was not already running beside it. The cost is that
reads pause too; an input-only guard needs one upstream in gpui-mcp.

#### Audit view

The rail's `rail-audit` and the palette ("Open audit log", Ctrl+Shift+A) open the audit view (`audit-view`, `shell/audit.rs`,
`shell/audit_model.rs`, `shell/audit_view.rs` and `shell/audit_table.rs`, artboard
`design/bolt-v2/13-audit.html`); while `Panel::Audit` is the rail's view the sidebar keeps the
collection tree and the view takes the document island, and a request opened or activated takes
it back. Its tab row shows the open requests and the active `Audit log` tab (`doc-tab-audit`;
`doc-tab-audit-close` goes back). It is read-only. `reqflux_engine::audit::read_log` reads `audit.log` newest first; a
line that does not parse is skipped and reported by its 1-based number, a blank line is
ignored and a missing file is empty. The view reads the log when it opens and on Refresh
(`audit-refresh`). The header shows `audit-title` ("Audit log") and `audit-subtitle`. The
toolbar has the search field `audit-search` (tool, class, targets, method, URL, reason and
workspace id, ignoring case), the time range `audit-range` (`audit-range-15m`, `-1h`, `-6h`,
`-24h` by default, `-7d`; an entry whose time does not parse is always kept), the single
selects `audit-class` and `audit-outcome` (all, allowed, refused) and, while a filter is set,
`audit-clear`. Under it, `audit-timeline` draws 48 bars over the range, the refused calls
stacked on the others. The table has a row per kept entry, `audit-row-<n>` from 1 at the top:
`-time` (local), `-class` (badge), `-tool`, `-target` (the method and masked URL of an
execute, else the target ids, named from the open workspace when the entry is its own),
`-workspace` (the open workspace's name, else the id) and `-outcome` ("Allowed" or
"Refused"). A click expands it into `audit-detail-<n>`: the time as written, the decision,
`-reason`, the workspace id, `-targets` and `-request`. Skipped lines show as the warning
`audit-skipped` ("N lines of the audit log could not be read and are not shown: lines …"),
never their text; a log that cannot be read shows `audit-error`, an empty one `audit-empty`, and
filters that keep nothing `audit-no-match`. The footer shows `audit-path` and `audit-count`
("N events in <range>"). The log has no actor, environment, duration, response or body, so the
view shows none. Tests set the path; the app takes `audit::default_path`.

- **More tools**: runner (run a collection or folder), import and export.
- **Resources**: requests, collections and environments addressed by id, so an agent can read
  them without a tool call.
- **Secrets never leave the engine.** Tools return `{{token}}`, not the token. Execution
  resolves secrets inside the engine, and response output redacts any secret value it finds
  echoed back.

The GUI and `reqflux mcp` coordinate through the files themselves: atomic writes and
hash-checked saves with conflict detection. There is no second runtime that has to agree with the GUI about a
database, which was DBFlux's most expensive structural mistake.

## UI

What exists today: ReqFlux is a standalone app. `reqflux` without a directory opens the most
recent workspace that still loads, and on the first run creates a workspace named Personal
kept by the app; the current directory never decides it. `reqflux <dir>` (`reqflux .` for the
current one) opens that directory. The workspace opens through `reqflux_engine` before the
window appears (a slow disk delays the window) and is recorded as recent. The window remembers
its active environment per workspace (by environment id, in the app state's `settings`) and
restores it when the workspace opens. The title bar's workspace
selector (`workspace-selector`) opens a menu (`workspace-menu`) with the recent workspaces
(`workspace-recent-<n>`, the name with `In ReqFlux` for a workspace the app keeps or the path
for a directory, the current one marked; `workspace-recent-remove-<n>`, shown while its row is
hovered or holds the keyboard focus and always on a row that did not load, drops one from the
list and deletes nothing), `New workspace…` (`workspace-new`, the name popover
`workspace-new-popover` beside the menu with the field `workspace-new-name`,
`workspace-new-cancel` and `workspace-new-create`; Enter or Create makes a workspace kept by the
app and opens it, Escape, Cancel or a click outside closes it and creates nothing),
`New workspace in folder…` (`workspace-new-folder`, the OS save dialog;
`reqflux_engine::create_workspace` writes a `reqflux.toml` named after the directory into a
missing or empty directory), `Open folder…` (`workspace-open`, the OS folder dialog),
`Import Postman collection…` (`workspace-import`, while a workspace is open: the import dialog
`import-backdrop` of `design/bolt-v2/11-import-postman.html` asks for a collection file, shows
what the import would write through `reqflux_engine::postman_preview` (the counts
`import-count-<kind>`, the files `import-file-<n>`) or why it cannot import (`import-error`), and
`import-confirm` writes it into the open workspace with `reqflux_engine::postman` and reloads
it; Cancel or Escape writes nothing),
`Workspace settings…` (`workspace-settings`, the settings page at Globals) and
`Close workspace` (`workspace-close`). Switching cancels sends in flight, closes every tab,
restores the new workspace's remembered environment and loads its drafts and current values;
the previous workspace's drafts stay in the app state. A workspace that fails to load keeps the
current one and shows why (`workspace-error`). With no workspace the document island shows
`New workspace…` (`empty-new`), `New workspace in folder…` (`empty-new-folder`),
`Open folder…` (`empty-open`) and the recent list (`empty-recent-<n>`). One window shows one workspace. The window is the Bolt shell of
`design/bolt-v2/01-main.html`: a title bar with the workspace and environment on the left,
the command search in the middle and the approvals bell on the right; the activity rail
(Collections, History, Environments, MCP approvals, Audit log, Settings); a sidebar island with
the collection tree (icons, indent guides, child counts, method labels in Bolt's semantic
colors) and a request and collection count; a document island; and a status bar that shows
the last send (`<METHOD> <path> · <status> · <time>`, or `sending` or `failed`) on the left
and, on the right, the Problems chip (`status-problems`, artboard
`design/status-bar/02-problems-panel.html`, `shell/problems.rs` and `shell/problems_panel.rs`).
The chip shows only when there is a problem: `1 error` (`status-problems-errors`) when the
config file did not load, with a danger wash and icon, then `N warnings`
(`status-problems-warnings`) for the config file's warnings and the workspace load warnings
together; warnings alone keep it neutral with a warning icon. A click opens the Problems panel
(`problems-panel`) anchored above it: a header (`problems-title`, `problems-count`,
`problems-keycap`), a `CONFIG` section (`problems-config-label`) with the error
(`problems-config-error`, its title "Config not loaded · defaults in use" and
`problems-config-error-detail`, which names the file, line and column), and a `WARNINGS · N`
section (`problems-warnings-label`) with one row per warning (`problems-warning-<n>`, config
warnings first), its file (`-location`, relative to the workspace for a load warning) over what
happened (`-message`). Escape, bound in the panel's `Problems` key context, or a click on the
backdrop (`problems-backdrop`) closes it. `theme::startup` reports the config file as a
`ConfigReport` (an error, or warnings) that the shell takes through `set_config`. In the tree the arrow keys move the selection, Enter or the chevron toggles a
folder, and a click or Enter on a request opens it in a document tab, or focuses its tab.
`/` focuses the filter (`tree-filter`), which narrows the tree to requests whose name, method
or URL contains the text (case-insensitive) with their folders expanded; Escape clears it and
restores the expansion; `tree-no-match` shows when nothing matches; the footer counts what is
shown.
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

The shell chrome draws with DBFlux's `bolt` as DBFlux draws its own: `bolt::colors` roles,
`ShellMetrics`, `TreeMetrics`, `TabMetrics`, `EditorMetrics` and `NotificationMetrics`, the
sidebar `Island`, tree rows as `ListRow` with the selection bar, document tabs, `Button`
(the new-collection button, the row `…` button, the action strip, the delete dialog),
`Kbd`, `menu_frame`/`menu_row` for the tree menu and the Move to… list, and `Modal` (danger,
without a header) for the delete dialog. Its sizes are DBFlux's: title bar 44, rail 46 with
38 px buttons, command search 420 by 30 (shrinking to 160 on a narrow window), bell 34 by 30, sidebar 290 with a 46 px header and
footer, tree rows 26, document tabs 46 and 30, action strip 32 with 24 px buttons, status
bar 38 with 26 px chips; the Problems panel is DBFlux's notifications popover (46 px header)
at 540 wide, and the delete dialog is 480 wide with 28 px request rows. The path bar (28)
and the method column stay ReqFlux tokens. No `// bolt gap:` stand-in remains: every piece,
including the approvals bell (`approvals`, a button named "Approvals") and the History empty
state's send hint (`history-empty-action-0`, keycap `history-empty-key-0`), comes from `bolt`.

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
the Response tab with the status chip colored by class and the time and size, Tests and
Console tabs beside it when the scripts produced results, the Body / Headers / Cookies views as
a segmented control, a bottom bar under the body with the Pretty / Raw / Preview switch and the
line count (Pretty re-indents JSON keeping key order), the body split into lines once
off the UI thread and drawn with `uniform_list` so only visible lines render, an error banner
with Retry, and the unresolved variables the send used. Each open request keeps its last
response in memory, and every send is recorded in history (see "History").

`bolt`'s splitter, a 6 px gutter with a three-dot grip (`request-response-split`), divides the request
(`request-area`) from the response (`response-area`). Dragging it sets the request's height;
Up and Down on the focused gutter move it by 16 px; Enter or a double click collapses the
response to a 40 px bar (`response-bar`) with its status, time and size and test count, and
restores it, as do the bar's expand button (`response-expand`) and a drag. Each pane keeps at
least 120 px. The position is per user, shared by every request tab, and kept across restarts
in the app state's `settings` (`request_response_split/height` in pixels and
`request_response_split/collapsed`); a stored value that does not parse keeps the default.
The Params, Headers and body grids are spreadsheet cells: enable, key, value (filling the
rest), description and delete columns with a line on every side of every cell, the key
column's kind and, with an active environment, `resolved in <environment>` beside the value
column's name (`request-<table>-value-hint`); an open cell is a well with a tint focus ring.
The request and response panes draw with DBFlux's `bolt` (`bolt::colors`, rem tokens, `Button`,
`Badge`, `Checkbox`, `BannerBlock`, `EmptyState`, document tabs, `menu_frame`/`menu_row`,
`field_row`), so their sizes are DBFlux's: grid header 40 and rows 31, code lines 22, URL
field and Send 30, result rows 28, body-format footer 36. No `// bolt gap:` stand-in remains.

The views and overlays draw with DBFlux's `bolt` too. The MCP approvals view uses
`ApprovalsMetrics`, DBFlux's own approvals sizes (pending list 380 wide, 52 px detail title,
22 px between sections, 14 by 16 payload padding), `ListRow` rows with the selection bar,
`Badge` for the class and `Pending`, `Kbd`, `Input` for the reason field and `Button` for
Refresh, Deny, Allow for this session and Allow once. The history tab is a document tab, its
banner DBFlux's 34 px editor banner, its grid `GridMetrics` (header 40, rows 31); the History
panel uses the tree's `ShellMetrics` and `TreeMetrics` (header and footer 46, rows 26) and
`EmptyState`. The command palette is DBFlux's (`PaletteMetrics`: 56 px query row, 38 px rows
and footer) with document tabs as scope tabs. The settings page draws `Island`s, `ListRow`
nav entries and environment rows, the variables grid at `GridMetrics`, `Checkbox`, `Badge`
for `file` and `KEYRING`, and `Button`. The workspace and environment menus are
`menu_frame`/`menu_row` (30 px rows, 44 px recent rows) with `render_menu_header`. No
`// bolt gap:` stand-in remains. `reqflux_ui::theme`
installs `bolt`'s `ModalSeams` as DBFlux's theme does: ReqFlux's modal actions bound in the
`Modal` key context (Escape cancels, Enter confirms, the arrows, Page Up, Page Down, Home and
End scroll), the `Esc` keycap and the close button, so every `bolt` modal answers its keys.

An image or HTML response (by `Content-Type`, then PNG, JPEG, GIF or WebP magic bytes) adds
`Preview` (`response-view-preview`). An image is decoded off the UI thread and drawn at its
natural size, shrunk to fit and never enlarged, with its size and format
(`response-preview-meta`); one that does not decode says so. GPUI has no web view, so HTML
shows its title and readable text (tags stripped, scripts and styles dropped) and
`Open in browser` (`response-preview-open`), which writes the body to a new file under
`$XDG_CACHE_HOME/reqflux/preview/` and opens it with the system handler; ReqFlux never fetches
anything to render a page. An image over 20 MB or HTML over 5 MB is too large to preview. The
Cookies sub-tab (`response-tab-cookies`) is a table of every `Set-Cookie` header
(`response-cookie-<n>`): name, value, domain, path, expires (`Max-Age N`, the `Expires` date in
UTC, or `Session`) and the HttpOnly, Secure and SameSite flags; a header that does not parse
shows raw with a warning mark. Every cell goes through the send's masker, and a value that holds
a secret the send knows, or that history stored masked, shows as dots with the SECRET badge.
Nothing is stored, since the cookie jar comes later. History keeps `Set-Cookie` headers as
received apart from secret values; MCP output masks their cookie values (see "History").

Clicking the response content gives it the keyboard focus; Ctrl F there (or a click on
`response-search`) opens the body search in the views row: case-insensitive matches over the
lines the body shows, the count `n / total`, Enter and Shift+Enter or the arrows step and wrap,
the current match filled in the warning color and the others washed in it, and Escape closes
it. The Tests pane has an All / Failed filter under `<n> passed` and `<n> failed` badges, the
Console pane an All / Warnings / Errors filter with counts; rows keep their numbers under a
filter. Copy (`response-copy`) in the body's bottom bar puts a UTF-8 body on the clipboard.
The search, the filters and the focus live with each request's send state.

The rail's History button (`rail-history`) swaps the sidebar tree for the History panel
(`history-row-<n>`, newest first, `METHOD path · status · time ago` or `failed`; the 200
newest; `No requests sent yet` when empty), and Collections (`rail-collections`) swaps back.
Clicking a row opens its request's tab and shows the recorded response under
`From history · <time>` (`response-history-label`) without touching the request or its draft;
`Resend` (`history-resend`) sends the recorded document again through `execute_draft` and
records the result as a new entry. A row whose request was deleted opens a read-only view
(`history-readonly`) with the recorded method, URL and response, and Resend disabled.
In the history tab the response tab reads `Recorded response`; switching to the History
panel (the rail or Ctrl+2) gives it the keyboard focus, so `/` focuses its filter
(`history-filter`) at once; activating the history tab leaves the approvals or audit view; Next and Previous tab cycle through the history tab,
first in the row.

The settings page's App section (`design/app-settings/02-workspace-page-app-section.html`,
`shell/app_settings.rs`, `shell/app_settings_view.rs`) sits above its workspace sections and
opens even with no workspace: the rail's Settings (`rail-settings`, marked active while an App
section shows) opens it at General (`theme`, `font-size`, applied live through bolt's font
install); MCP policy holds `rule-read|write|execute|destructive` (Allow, Ask, Deny),
`hosts-list`, `environments-list` and the read-only `overrides-table` (`overrides-open-config`
opens `config.toml` with the OS handler, `open_file.rs`). Every control writes `config.toml` at
once; while it does not parse, `config-error` shows its path, line and column, the controls do
not write, and `config-open` and `config-reload` act.

Environments and variables are edited on the workspace settings page (`settings-page`,
`design/variables/01-settings-page.html`, `shell/settings_page.rs`, `shell/settings_view.rs`,
`shell/variables_document.rs`), which takes the place of the sidebar and document islands
beside the rail. The rail's Environments button (`rail-environments`) and `Show Environments`
(Ctrl+3) open it at Environments, `Workspace settings…` in the workspace menu and the palette's
`Workspace settings` (Ctrl+,) at Globals, a palette variable row at Environments; Collections,
History, `settings-close` or Escape leave it. A header row names the workspace and its path
(`settings-title`, `settings-workspace`, `settings-workspace-path`); the nav island
(`settings-nav`) lists Globals (`variables-scope-globals`) and Environments
(`settings-nav-environments`) under `WORKSPACE` and one entry per collection
(`variables-scope-collection-<id>`) under `COLLECTION VARIABLES`, each with its count, over the
resolution order. The content island shows the section title (`settings-heading`,
`settings-description`); in Environments the environment list sits beside the variables:
`New environment` (`variables-new-environment`) creates one and opens its name into a field
(`variables-rename-input`, hint `variables-rename-hint`); each row
(`variables-scope-environment-<id>`, by name; the first is shown by default) has its count and
file (`variables-environment-meta-<id>`) and a `…` menu (`variables-scope-menu-<id>`: Rename,
`Delete environment…`); delete asks first (`variables-delete-prompt`; deleting the active
environment selects No environment). The variables column (`variables-document`) has a heading
(`variables-title`, `VARIABLES IN <NAME>` or `GLOBAL VARIABLES`) with the count of enabled
variables that resolve to nothing (`variables-empty`) and `Reset current values`
(`variables-reset-current`, clears the current values of the declared keys); then the grid,
row `variables-row-<n>`: number, enable box, key, initial value (in the file), current value
(this machine; `= initial` when unset, `empty` when both are), stored in (`variables-secret-<n>`,
the `file` pill or the `KEYRING` badge `variables-stored-<n>`, a toggle), description and
delete, plus the ghost row `variables-add` (key `new_variable`). Variables are not drafts:
every commit writes the file (or the keyring, or the app state for a current value) at once
and re-inspects open requests. A secret's initial value reads `not in the file`; its current
value shows `••••••••` (`variables-masked-<n>`) and edits the keyring through an empty field
whose empty commit keeps it. Turning a secret off does not copy its value into the file, and
the page says so (`variables-warning`). Current values a script wrote for a key the scope does
not declare show under `ONLY ON THIS MACHINE` as `variables-undeclared-<n>` rows: the current
value edits (an empty commit clears it), `Declare` (`variables-declare-<n>`,
`Engine::declare_variable`) adds the key to the scope's file with no initial value and keeps the
current value local, and the discard control (`variables-discard-<n>`) clears it; renaming a
key leaves its current value on the old key, where it shows as such a row. Notes name the
keyring entry of the first secret (`variables-keyring-note`) and what Declare does
(`variables-declare-note`). A refused write shows its reason in `variables-error` and changes
nothing.

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
| Switch environment, Show Environments | Ctrl+E, Ctrl+3 | a workspace is open |
| Open workspace, New workspace | Ctrl+O, Ctrl+Shift+O | always |
| Close workspace, Workspace settings | Ctrl+Shift+W, Ctrl+, | a workspace is open |
| Close tab, Next tab, Previous tab | Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab | a request or the history tab is open |
| Show Collections, History | Ctrl+1, Ctrl+2 | a workspace is open |
| Toggle theme | Ctrl+Shift+T | always |
| Open MCP approvals | Ctrl+Shift+M | always |

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
values; the artboards under `design/` are the approved spec of every view, built from DBFlux's
own. Before changing or adding anything visible, open the artboard that covers it (the
`gentle_design` tool lists them and records which one a change implements); when the change is
not drawn, update or add the artboard in the same change, and say in the change what the code
does not back yet.

| Area | Artboard |
|------|----------|
| Shell, request, response states, workspace settings, collection settings, welcome, import, MCP approvals, audit | `design/bolt-v2/` (01 to 13) |
| Response island (views, format bar, Tests, Console) | `design/response-views/02-script-tabs-format-bar.html` |
| Status bar and Problems panel | `design/status-bar/02-problems-panel.html` |
| Workspace menu | `design/workspace-menu/01-refined-dropdown.html` |
| Command palette | `design/command-palette/02-scope-tabs.html` |
| Collection tree editing | `design/tree-editing/01-context-menu-modal.html` |
| History | `design/history/01-sidebar-readonly-tab.html` |
| Environments and variables | `design/variables/01-settings-page.html` |

The other artboards in those folders are directions that were not chosen.

Bolt is DBFlux's `bolt` crate, a git dependency pinned to the DBFlux rev the GPUI stack uses;
ReqFlux never copies it. Every screen draws with it, and what gpui-component draws (inputs,
scrollbars, the code editor) is DBFlux's palette with ReqFlux's accent.

| Topic | Rule |
|-------|------|
| What `bolt` owns | Everything product-agnostic: color roles and washes, typography roles, spacing, cuts and chrome metrics, the chamfer shape, the theme mapping onto gpui-component, and the components (button, field, select trigger, segmented control, tabs, menu, badge, keycap, banner, checkbox, list row, empty state, modal, splitter, island, status chip). It never names ReqFlux, HTTP or a ReqFlux type. |
| What ReqFlux passes in | `reqflux_ui::brand::reqflux()`, the `Brand` (cobalt accent fill, hover, pressed, tint, text on accent, app name), and its own tokens in `reqflux_ui::tokens` (method hues, syntax colors, ReqFlux-only widths). |
| Install | `reqflux_ui::theme::install` loads `bolt`'s bundled fonts (Archivo, Archivo Expanded, JetBrains Mono) at the default interface size (13), so a rem is 16 px and `bolt::tokens` sizes are DBFlux's pixels; then the default density and `bolt::theme::install` with the brand and the mode from `config.toml`. It also installs `bolt`'s modal seams: Escape cancels, Enter confirms, the scroll keys scroll. |
| Icons | The Lucide catalog of `gpui-kit-assets` (the one gpui-component bundles), drawn through `bolt`'s `IconSource::Svg`; `reqflux_ui::assets()` is its asset source. |
| Screens | Use `bolt` roles (`bolt::colors(cx)`, `bolt::brand::brand(cx)`) and components, never gpui-component `Theme` fields or raw colors. A guardrail test rejects color literals and bare pixel values outside `tokens.rs` and `brand.rs`, as DBFlux does. A control `bolt` lacks is added to `bolt` in DBFlux; until then the stand-in carries a `// bolt gap:` comment that says what is missing. |
| Theme parity | A test compares the gpui-component theme ReqFlux installs with `bolt::theme::install` alone (DBFlux's install) with ReqFlux's brand, in dark and light and after a switch: ReqFlux overrides no field. |

**Palettes.** `bolt::Colors::dark()` and `Colors::light()` are DBFlux's. `PaletteMode::{Dark,
Light, System}` is passed to `theme::install` and switched at runtime with `theme::set_mode`,
which applies `bolt`'s theme for the resolved palette; `bolt` keeps only the resolved
palette, so ReqFlux keeps the selected mode itself and System follows the window appearance.
In light the accent washes use the accent instead of the tint. ReqFlux's method and syntax
colors read the active palette. The choice is the `Toggle theme` command (Ctrl+Shift+T, Dark,
Light, System) and persists as `theme` in the user's `config.toml` (a value from the app
state's older `settings` key is moved there once); Dark is the default.

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
- UI tools are named `ui_*` and go through the MCP policy (see "Configuration and policy"):
  reads are class `read`, input is class `execute`, because a click on Send executes a request
  just as `request_execute` does. While an approval is pending the bridge is down (see
  "Approvals").
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
- The bridge, `gpui-mcp-protocol` and the patched GPUI crates all come from one DBFlux rev, set
  once in the root `Cargo.toml`, where DBFlux vendors upstream `gpui-mcp` against its patched
  `gpui-pre`. Bump it for all of them at once. Until the DBFlux branch is published the URL is a
  local `file://` repository; then it becomes the published repository at the same rev.

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

`.github/workflows/nightly.yml` runs daily at 03:17 UTC (and on manual dispatch), as DBFlux's
nightly does. A guard job skips the run when `main` has not advanced since the commit the
`nightly` tag points at, or when it advanced only in Markdown and `design/` files; a manual
dispatch always builds; a run from any ref other than `main` fails without publishing. A
build compiles `reqflux` on `ubuntu-latest`, clears the assets of the `nightly` prerelease,
uploads `reqflux-linux-amd64.tar.gz` there with the commits since the previous nightly in the
release body, and only then moves the single `nightly` tag to the built commit, so a run that
fails partway leaves the tag where it was and the next scheduled run retries. There is only ever one nightly tag; the dated `nightly-*` tags and releases of
the earlier scheme are deleted by the first run. The nightly version is the workspace version
with its pre-release suffix stripped (`0.1.0-dev.0` → `0.1.0`) plus the short commit as
semver build metadata, e.g. `0.1.0-nightly+928b3d5`.

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
| Vendor nothing. | DBFlux carries four patched vendored crates, each with its own refresh procedure. ReqFlux takes DBFlux's patched GPUI stack and its `gpui-mcp` copy by git rev instead of copying them. |
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
