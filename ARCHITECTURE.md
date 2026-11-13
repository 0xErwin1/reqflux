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
  to it with an allowlist; an empty list means all of them. The environment menu lists every
  environment of the workspace; a send of that collection with one outside the list is refused
  (`environment prod is not allowed for collection Billing API`), as `reqflux run` and
  `collection_run` are.
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

`Engine::variable_info(request, environment, current, name)` answers what a front end shows
about one `{{name}}` (`reqflux_engine/src/variable_info.rs`): the scope that wins with its
current and initial values, the next scope down it overrides, whether it is secret (never
its value; `Engine::reveal_variable` reads that on an explicit reveal), the environments that
declare it when none is active, and the scopes a quick add can target (the active
environment, the request's collection, the globals). `Engine::set_variable_value` writes a
current value (a secret's value goes to the keyring instead), `Engine::reset_variable_value`
clears it, and `Engine::quick_add_variable` adds the variable with the value as its initial
value (a secret keeps only its key in the file). `Engine::copy_variables(from, keys, to, policy)`
(`reqflux_engine/src/variables_copy.rs`) copies the declared variables `keys` of one scope into
another: initial values, descriptions and enabled flags, and for a secret the keyring value into
the destination's own entry. A key the destination already declares is skipped
(`CopyPolicy::Skip`, the default) or replaced in place (`CopyPolicy::Overwrite`), as one file
write per key: a secret's new keyring value is set before the write and the previous one put back
if the write fails, so a key is never left half updated. A keyring that cannot be read or written
skips that key (`SkipReason::Keyring`) with the old variable intact; a secret is never copied
without its value. An unknown source or destination scope is an error even when no key reaches
it. The `CopyReport` names keys, never a value.

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

#### Collection runner

`Engine::run_collection` runs the requests under a collection or folder in tree order, depth
first, each through the `Engine::execute` path (scripts, the active environment, the host guard).
Script writes to globals, collection variables and the environment carry to the next requests as
current values in memory (`RunSummary::current`); `pm.variables` and secret writes do not carry,
and nothing reaches disk. `pm.setNextRequest(id|name)` jumps (an id first, then the first request
with that name); `null` stops; an unknown name ends the run as a failure. At most
`RunOptions::max_steps` sends (default 1000) end a jump loop. A cancellation token stops the run
and cancels the send in flight; `bail` stops at the first failed step. Events (`Started`,
`Finished(StepResult)`) and the summary carry only masked URL, error and test text; each
`StepResult` also carries the variable names that did not resolve and went out literally
(`StepResult::unresolved`), as `Execution::unresolved` does for a single send. A caller that
passes the app state gets one history entry per send; pre-send failures and cancelled sends are
not recorded.

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
│       ├── collection.test.js        optional collection post-response script
│       └── invoices/                 folder
│           ├── folder.toml           id, name, auth, order
│           ├── list-invoices.req.toml
│           ├── list-invoices.test.js     post-response script ("test" is Postman's event name)
│           └── list-invoices.examples/
│               ├── ok.example.toml
│               └── ok.body       optional, a body that is not UTF-8
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
description = "Start with the invoices folder."   # markdown, optional; a folder has one too
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
description = "Creates a draft invoice."   # markdown, optional
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
| `[auth]` | `type = "inherit"` (default, not written), `"none"`, `"basic"` (`username`, `password`), `"bearer"` (`token`), `"apikey"` (`key`, `value`, `location`), `"digest"` (`username`, `password`, `algorithm` `"MD5"`\|`"SHA-256"`), `"awsv4"` (`access_key`, `secret_key`, `session_token`?, `region`, `service`), `"oauth2"` (`grant_type` `client_credentials`\|`password`\|`authorization_code`\|`refresh_token`, `token_url`, `auth_url`?, `client_id`, `client_secret`?, `scope`?, `username`?, `password`?, `refresh_token`?, `client_auth` `body`\|`header`, `token_location` `header`\|`query`, `header_prefix`?, `query_param`?; Postman's key spellings are read too; credentials belong in `{{secret}}` variables), or any other type name with its fields kept as they are |
| `[body]` | `mode = "none"`, `"raw"` (`language`: `json`, `xml`, `text`, `html`, `javascript`; `text`), `"urlencoded"` (`[[body.fields]]` key and value rows), `"formdata"` (`[[body.parts]]` with `kind = "text"` and `value`, or `kind = "file"` and `path`), `"binary"` (`path`), `"graphql"` (`query`, `variables`) |
| key and value rows | `key`, `value`, optional `description`, `enabled = false` when disabled |

`description` on a collection, folder or request is markdown for the Docs tab and the generated
API docs. It is omitted when absent or blank, so a file without one reads and writes the same
bytes, and a description with several lines is a TOML multi-line string. A collection imported
from Postman before the field existed keeps the text under `extensions.postman`; reading falls
back to it (`description_text()`), a load never rewrites the file, and setting a description
(`Engine::set_description`, or the Docs editor's save) writes the field and drops that copy in
the same write, so clearing the text cannot bring the old one back.

## Drafts and saving

Saving is explicit, as in Postman: an edit never touches the workspace file until `Ctrl+S`.

- An edited entity becomes a **draft**, stored in the per-user app state and keyed by
  workspace id and entity id. Drafts survive a restart and their tabs show them as unsaved.
  The app state is SQLite (`reqflux_engine::state`, `rusqlite`) at
  `$XDG_STATE_HOME/reqflux/state.sqlite3` (`$HOME/.local/state/reqflux/state.sqlite3` when the
  variable is unset), with an explicit, forward-only schema version (v2 adds the `history`
  table, v3 the `current_values` table keyed by workspace id, scope and key, v4 the
  `recent_workspaces` table, the 20 newest by canonical path, v5 `settings`, v6 the history
  `timings` column, v7 `cookies`, v8 `document_drafts`; an older file upgrades in one
  transaction and keeps what it holds). A draft stores the
  whole entity document as TOML, so later editable surfaces need no new storage.
- A draft payload is the request document as TOML. When the draft edits scripts, it adds a
  `[reqflux_draft_scripts]` table: `pre_request`, `test`, and the sidecar hashes
  `pre_request_base` / `test_base` the scripts were made from. A payload without that table
  means the scripts are as saved. Saving checks the request and both sidecar hashes first,
  then writes the sidecars (an empty script deletes its file) and the .req.toml if its
  document changed (`Engine::save_draft`).
- The Document layout's unsaved `.http` text of a folder is a draft of its own, in the
  `document_drafts` table (schema v8, keyed by workspace id and folder id), apart from the
  request drafts so their loader never meets it (`AppState::put_document_draft`,
  `document_draft`, `delete_document_draft`). Its payload is the text as typed and its base the
  content hash of the canonical document it was edited from. When the folder's canonical
  document no longer hashes to that base — when the text is reopened, or while it stays open
  and the workspace reloads — the text is stale and Save is refused until Discard, as a request draft whose file moved is conflicted until reverted. Without an app
  state the text lives only in the editor and a folder switch drops it.
- **Sending** a request with a draft sends the draft. History records what was sent. When the
  draft edits scripts, the send runs the draft's unsaved scripts in place of the request's
  sidecars (`Engine::execute_draft_with_scripts`); an empty or absent draft script falls back
  to the folder's or collection's, as an absent sidecar does. Resend of a history entry runs
  the saved scripts.

### Examples

A saved example is a request-and-response pair next to its request, one file each:
`<request-slug>.examples/<example-slug>.example.toml`, its slug from the name (a taken name gets
a number: `OK`, `OK 2`). The request is kept as written, `{{variables}}` included; the response
goes through the send's masker (headers, reason, a UTF-8 body; `Set-Cookie` values masked). A
body that is not UTF-8 is written as received to `<example-slug>.body` beside it. Renaming,
moving, duplicating or deleting the request carries its `.examples/` directory; ids are unique
within one request. `Engine::{examples, save_example, rename_example, delete_example}` own them,
and `Engine::save_response_body` writes a response body to a chosen path. A Postman import writes each item's
`response[]` entries as examples after the request: the request is `originalRequest` when present,
else the item's request, kept as written; response headers are masked as `save_example` masks
them, and so is the `value` of each entry in the Postman `cookie` array kept in
`extensions.postman`. An entry without a numeric status or with a request that does not map stays in the
request's `extensions.postman`, masked the same way (credential header values by the
`header_for_agent` rule and every `cookie[].value`); `response` leaves it only when every entry
mapped. The import
summary and preview count the examples, and the preview lists their files.

```text
id = "…"
name = "OK"

[request]               as written: method, url, params, headers, auth, body
method = "GET"
url = "{{base_url}}/v1/invoices"

[response]
status = 200
reason = "OK"

[[response.headers]]
key = "content-type"
value = "application/json"

[response.body]
text = '…'              or: file = "ok.body"
```

### History

Every completed or failed send in the window (not a cancelled one) is recorded in the app
state under the workspace id: when, the request id, name and path at that time, the request
document sent (TOML, references as written), the request as sent (method, URL, headers, body,
or a short description of a multipart, file or binary body) and the outcome (status, headers,
body, time and size, or the error). `reqflux_engine::state` owns the table (`record_history`,
`history`, `history_entry`, `clear_history`); front ends never write it. A body over 5 MB is
stored cut, on a character boundary for text, and flagged truncated; a workspace keeps its
newest 500 entries. A send that fails before it goes out (an unresolvable request, a failing
pre-request script) is not recorded, and neither `reqflux run` (a request or a collection run) nor MCP
record yet; the window records a collection run by passing its app state to
`Engine::run_collection`.

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
masker works on text. Schema v6 adds the send's timings (a JSON array, one object per hop, in
microseconds); rows recorded before it read as no timings. Each hop object also keeps the request that hop
sent (`method`, `url`, `headers` as `[name, value]` pairs in wire order), masked; the fields are
optional, so no schema bump: a row without them decodes with an empty wire. The send task
masks every hop's `wire` before the response leaves it (`Masker::timings`: each header through
`Masker::header`, the URL through `Masker::mask`), so `Execution.handle`, `Engine::wait`, the
runner, history and every front end only see masked hop requests; `RecordedResponse::from_response_masked`
masks them again. `PreparedRequest`'s `Debug` prints no URL, header value or body.
- **Saving** writes the entity file (atomic, hash-checked) and discards the draft. The
  engine entry points are `Engine::save_request`, which writes the request document back
  and returns the new file hash, and `Engine::request_base_hash`, which reports the hash
  the file had when the workspace was loaded.
- **Discarding** (the command `revert-request`, labelled "Discard changes") drops the draft
  and reloads the saved file.
- **Undo and redo** walk a per-request history of draft snapshots, taken on every
  `edit_draft` and `edit_draft_scripts` and kept in `shell/draft_history.rs`. A burst of
  edits to one slot of the request (typing in the URL, a body, a cell) within 800 ms is one
  step; a mode switch or a discard is a step of its own; the history keeps the last 100
  steps. Discarding is a step too, so one undo restores the discarded draft. A snapshot is
  the whole draft or "no draft", the history lives in memory only (not across restarts) and
  saving forgets it. `Ctrl+Z`, `Ctrl+Shift+Z` and `Ctrl+Y` run the `undo-draft` and
  `redo-draft` commands while no text editor has the focus; a focused editor keeps its own
  `Ctrl+Z`.
- **Mode switches keep data.** Switching the body mode or the auth type leaves the data of
  the mode switched away from in the draft's in-memory stash and restores it when the user
  switches back. The stash is never written: the file and the stored draft hold only the
  current mode.
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
Losing focus commits the field like Enter, without sending: a click on any other control, or on
an empty area of the shell (the shell root takes the focus), blurs the input, the draft keeps
the text and the read-only marks, with their popover, come back. A window deactivation is not
a blur, and while the variable popover is open the focus sits in it and the field stays. Blur and
Enter close the field through the same path (`commit_url_edit_as`, differing only in whether the
shell takes the focus back from whatever got it). When the popover closes, once the event that
closed it was handled (a press outside moves the focus after closing), the focus returns to the
field if it was still in the popover, else the field commits if it does not have the focus, so a
blur that arrived under the popover is never lost and no field is stranded open.
A row with an empty key and an empty value is never sent, and params go before a `#fragment`.
A row with an empty value shows as its bare key (`?b`); the request sends `b=`. While the URL
field is open, a grid change (a row toggled, edited or deleted) re-seeds it in place, keeping
the cursor; Escape then returns to that point.

The split-based modes show the response pane and the split after the request's first send in
the session, or at once when its history has a send (the newest one, masked as recorded); before
that the request uses the whole height.

The window has three built layout modes (v1-features S7, design/layouts/01): Stacked, the
request above its response across the draggable split; Side by side, the request and its
response in columns with the splitter standing vertically between them (`bolt`'s
`SplitAxis::Horizontal`); and Document, which draws no split at all, the active request's folder
as one `.http` document (`document-view`, numbered `document-line-<n>` rows grouped in
`document-section-<n>`, each line `bolt`'s `CodeView` inside one `FieldWell`, editable and
synced with the request files; see the `.http` document
paragraph below). Collapsing the response shows its bar across the island in both
split-based modes. The mode is `layout` in the user's config.toml (`"stacked"` by default,
`"side-by-side"`, `"document"`), read at startup and written by the same format-preserving
editor as `theme`; a name this build does not know reads as the default. The action-strip
switcher (`layout-strip`, at the foot of the request's action strip) changes it, as do the
`Layout: Stacked`, `Layout: Side by side` and `Layout: Document` commands (Ctrl+Alt+1,
Ctrl+Alt+2, Ctrl+Alt+3). The keymap is configurable (see `Configuration and policy` for the
`[keymap]` table).

A `.http` document, the text that mode reads, is plain text: sections separated by
`### <name>` lines, each with a `<METHOD> <url>` line (a lone URL line means GET), `Key: value`
headers, a blank line, then a verbatim body; `#` and `//` lines are comments. A `###` line opens
a section at column zero before the body, or after a blank line inside it, so an indented `###`
or one directly after body text is body. `reqflux_core::http_document::parse` never fails
(broken sections come back as warnings) and `write` re-emits canonical text, so `parse(write(x))
== x` and a canonical document round-trips byte for byte; `DocumentRequest::to_request` turns a
section into a model `Request`. The window shows a folder through `write`: every value as
written (no resolved secrets, never a draft's resolved value), no body text for the non-raw body
types yet. The document is editable as one text (`document-view`): clicking it opens a `bolt`
multi-line editor (`document-editor`) seeded from `write` of the folder and refreshed underneath
while the text is not dirty. Ctrl+S (`save-request`) parses the text and saves through the engine
— sections update their request (method, URL and params, enabled headers, raw body), new names
create requests in the folder, and requests whose section disappeared are deleted only after one
confirmation modal for the whole save (`document-confirm-delete`); parse warnings show in
`document-warnings` and block the save; Discard drops the edit. A dirty text is stored per
folder in the app state (see "Drafts and saving"), so switching to another folder and back, or
restarting, reopens the editor on it; Save and Discard forget it. A dirty text whose folder
changed since, reopened or still open when the workspace reloads, shows `document-stale` and
refuses to save until it is discarded.
Running a section there is
Ctrl+Enter (`send-request`, intercepted while the focus is in the document; the `.http` editor
consumes a secondary Enter, so the document captures it like the GraphQL editor does) or the
editor's Send (`document-send`): the section the cursor is in — the last one that starts before it
when the cursor sits between sections — maps to its folder request: the editor ties each seeded
section to its request and carries the ties over every edit, by name (the previous one, else the
seed's) and, for a rename in place, by position — only while the section count is unchanged,
every section that kept its name kept its place, the section kept its method and URL, and the
name is not two headers glued together. So an unsaved rename keeps the section's request, its
response block and its Retry, while a section typed above another, or merged from two, belongs
to no request and Ctrl+Enter there sends nothing (a Retry whose section was deleted says so in a
toast). It sends through the same draft path as a request tab, so it records history and honours
the environment allowlist and the host policy without saving. The response draws under that
section in the read-only document, or below the editor while it is open
(`document-section-<n>-response` over `document-response`, with `document-response-status`,
`document-response-body`, `document-response-error` and its `document-response-retry`), reusing the
response pane's views; the read-only document shows the response of the request the window sent
last, else the active request's. Escape in the Document layout cancels the send the window started
last while it is in flight, whatever tab is active; the split layouts cancel the active request's.

In the Params and Headers grids, each row carries an enable box (`request-<params|headers>-enabled-<n>`) whose click toggles the row's `enabled`
state in the same draft on the spot, the grid shows the draft's rows while a draft exists,
and clicking a row's key, value or description cell
(`request-<table>-<key|value|description>-<n>`) swaps the cell for a field
(`request-cell-input`) seeded with the cell's text (a description in the interface face, an
empty one for a row without; an emptied description is removed from the row); Enter commits
it to that cell of the same draft, Escape drops what was typed, and clicking another cell commits the
open field first, like a spreadsheet; every cell field (the grids', the Variables document's,
the auth fields', the custom verb's) also commits when it loses focus, as the URL field does
(`Shell::watch_cell_blur`, run on every render, follows the open field's input). A field opens with
its caret at the end of the text and scrolled to it (`cell_edit::seed_focused`, also the URL field's
and the auth fields'), so a value longer than its cell shows its tail and the caret; typing and the
arrow keys keep scrolling with the caret. A value that
references a secret is never seeded: the
field opens empty with a `Secret value` placeholder and an empty commit keeps the stored
reference. The row counts next to the Params and Headers sub-tabs read the draft, so a
toggle moves them. A drafted tab carries a tint dot, Save and Discard changes
wake up, `Ctrl+S` saves and reloads the workspace so the tree and later sends see the file,
and a moved file shows `File changed on disk` with the draft kept until Discard changes. Sending a
drafted request goes through `Engine::execute_draft`.

Every other part of a request edits the same draft, and every pane shows the draft while one
exists:

- **Method.** The method chip (`request-method`) opens a menu of the standard verbs
  (`method-<verb>`) and `Custom…` (`method-custom`), which opens a field
  (`request-method-input`) that takes any HTTP token, uppercased; other text leaves the
  draft unchanged. The field opens with the verb selected, so typing replaces it.
- **Rows.** Params, Headers and the urlencoded or form-data body grid end with an `Add` row
  (`request-<table>-add`) that appends an empty enabled row and opens its key cell; an empty
  editable grid shows its header and that row, not an empty state (design/request-panes/01
  frame E), and the row lights up under the pointer. Every
  row has a delete control (`request-<table>-delete-<n>`). An open cell commits before a row
  is added or deleted, so typed text never lands in a shifted row.
- **Body.** The mode row (`request-body-mode`) opens a menu (`body-mode-<none|json|xml|text|
  urlencoded|formdata>`); switching between raw languages keeps the text, any other switch
  starts empty. Raw text opens a code editor (`request-body-input`) that writes the
  draft as it is typed: Enter inserts a newline, Ctrl+Enter sends, Escape leaves it.
  The raw and GraphQL editors, and an open script or description editor, fill the height
  the request/response split leaves the request pane and follow it when the split moves.
  They scroll their own lines, keep at least four, and the read-only wells fill the same area
  so nothing jumps when editing starts. Other tabs keep their natural height and the pane
  scrolls them.
  Highlighting (design/syntax/01): the raw text, its read-only well included, takes the
  grammar of its language (JSON, XML, HTML, JavaScript; Text is plain), and the open editor
  follows the language menu. The code editors are gpui-component's `EditorState` through
  `shell::code_edit`, with line numbers and without auto-closed brackets or smart indent, so a
  keystroke lands as typed; Tab indents by two spaces, and the gutter has no fold markers.
  Read-only wells draw `bolt`'s runs through `shell::syntax` with the `{{name}}` marks laid
  over them, the same marks the URL field draws (`request::spans::mark_style`: tint when the
  reference resolves, danger when it does not). A reference may stand where a JSON value goes,
  quoted or not: the read-only well highlights the line with an unquoted reference masked by a
  string of the same length, so the keys after it keep the `property` role, and the open editor
  marks every reference with decorations (`shell::input_marks`, `BodyEdit::marks`) set again on
  every change. The editor highlights the text as it is, so while an unquoted reference is in the
  text the grammar's error recovery colors the keys after it as strings (a `bolt` gap). The
  urlencoded and form-data grids edit like Params and Headers (a file part's value is its
  path). The GraphQL query (`request-body-query`) and variables (`request-body-variables`)
  each open a code editor (`request-body-query-input`, `request-body-variables-input`); the
  query is plain (no graphql grammar on tree-sitter 0.26 yet), the variables are JSON,
  that writes the draft as it is typed, one field at a time; the variables are validated as
  JSON on every edit against what the engine sends, the badge
  (`request-body-variables-badge`) shows `VALID JSON` or `NOT VALID JSON`, a failure shows the
  engine's exact message (`request-body-variables-error`) and Ctrl+Enter refuses the send while
  it stands. The binary body shows its path read-only (`request-body-file`, `No file` while
  empty) beside Choose file… (`request-body-choose`), which asks the OS for the file, and Clear
  (`request-body-clear`); only the path lands in the draft as `[body] path`, and the engine
  reads and sends the file itself.
- **Auth.** The type row (`request-auth-type`) opens a menu
  (`auth-type-<inherit|none|basic|bearer|apikey|digest|awsv4|oauth2>`); a new type starts with
  empty fields. Username, Key and the plain fields of digest, AWS SigV4 and OAuth 2.0 (region,
  service, client id, URLs, scopes) open seeded; Password, Token, the API key value and the
  `secret_fields` of those types open empty with a `Secret value` placeholder, a typed commit
  replaces the stored value and an empty commit keeps it, so a secret is never shown in a
  field. Typed fields are written back through the `From<…> for Auth` constructors, with
  canonical keys. Add to toggles header and query. Algorithm (`request-auth-algorithm`) and
  Grant type (`request-auth-grant`) are selects. An OAuth 2.0 auth adds the Token row: the kept
  token's state from `Engine::oauth_token_state` (never the token), Get new token
  (`Engine::fetch_oauth_token`, refresh token first, no send) and Clear
  (`Engine::forget_oauth_token_of`). While Get new token runs the form is locked: every row
  draws `bolt`'s disabled field row (`FieldRow::disabled`: dimmed at `Fields::DISABLED_OPACITY`,
  not-allowed cursor, its own content out of the pointer's reach), the selects under them are
  disabled and the menu the form had open closes with the lock. The Token row stays live:
  authorization code grant shows its loopback redirect with Open browser again and Cancel. Auth
  types ReqFlux does not support stay read-only and masked.
- **Settings.** The timeout opens a field (`request-setting-timeout-input`) that takes a
  whole number of milliseconds, or empty for the default; other text leaves the draft
  unchanged. Follow redirects and Verify TLS cycle Default, on and off on click; the redirects hint
  follows the setting (`Follows up to 10 redirects…` unless it is Off).

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
| `reqflux` | The binary: GUI startup and the `mcp`, `run`, `code` and `import` subcommands. | Contain logic beyond wiring. |

`reqflux_ui` starts as one crate with modules. DBFlux split its UI into six crates and then had
to forward every feature flag through all of them; ReqFlux splits only when compile times
measurably demand it, and then by one-way layers.

## HTTP engine

- `reqwest` with rustls, HTTP/1.1 and HTTP/2, gzip, brotli, deflate and zstd.
- A client's `NetworkSettings` apply to every send of the engine (the request, OAuth and digest
  requests, `pm.sendRequest`): a custom CA bundle merged with the platform roots, client
  certificates matched by host and port (`*.` matches subdomains only; first match wins), PEM or
  PKCS#12 (converted to PEM through `p12-keystore`, since rustls takes PEM identities only), and
  the proxy: `system` (the default: `HTTP(S)_PROXY`/`NO_PROXY`), `none`, or `manual` http,
  https, socks5 or socks5h with basic auth and a NO_PROXY-style bypass. A per-request "verify
  TLS" toggle stays. reqwest clients are cached by verify-TLS and the matched certificate; each
  redirect hop picks its own. Through a proxy, `dns` is the proxy lookup and `connect` includes
  the CONNECT or SOCKS handshake and TLS. Errors and `Debug` never print a passphrase or proxy
  password.
- Cookies: each engine has a per-workspace jar (`cookie_store`, RFC 6265 domain, path, Secure,
  HttpOnly and expiry, without the public suffix list) consulted on every hop of every send.
  `Engine::persist_cookies` keeps it in the app state (schema v7, table `cookies`, written
  through on every change), never in the workspace; `Engine::{cookies, add_cookie, set_cookie,
  replace_cookie, delete_cookie, clear_cookies}` edit it. `set_cookie` writes a cookie from its
  fields (the Cookies page's Add cookie) and `replace_cookie` edits one, a changed domain, path or
  name moving it; it refuses, rather than overwrites, a different cookie that already has the new
  domain, path and name. Both refuse an empty name or domain, a path without a leading `/`, a name or
  value with a separator or control character, an unknown `SameSite` and a past date, with an
  error text that never holds the value (`CookieError::Invalid`). The page's Add cookie and
  pencil open a dialog over those fields (the pencil names its cookie by domain, path and name,
  looked up when clicked, never by row position); a value shown as a secret opens empty and an empty
  field keeps it. The jar's `Cookie` header is recorded masked whole like
  every `Cookie` header. `reqflux run` keeps a jar for the run only, as Newman does.
- Timings (`Response.timings`: one `HopTimings` per hop, redirects included, with DNS, connect,
  TLS, first byte and download) come from a timed `getaddrinfo` resolver and a connector layer,
  attributed to the send through a Tokio task-local; the download time goes on the last hop.
  reqwest's connector layer wraps TCP and rustls together, so `connect` includes the TLS
  handshake and `tls` stays `None`; `dns` is `None` for an IP address and both are `None` for a
  reused connection. Splitting TLS out means dropping to `hyper-util` directly; the crate
  boundary keeps that change local.
- `reqflux_core::jsonpath::filter(body, expr)` filters a JSON body with RFC 9535 JSONPath
  (`serde_json_path`, the published crate that runs the compliance suite), returning the matches
  as a pretty-printed array in document order, keeping the server's key order.
- Known limits of the reqwest-based transport: repeated headers with the same name are grouped
  at the position of the first one (the order among their values is kept), and the reason
  phrase is the standard one for the status code rather than the server's own text. Both are
  local to `reqflux_http`; dropping to `hyper` fixes them if they ever matter.
- Transport defaults, each only when the request does not set the header itself: every hop sends
  `User-Agent: ReqFlux/<version>` (some gateways refuse a request without one), and a binary
  body from a regular file sends its `Content-Length` instead of chunked encoding (a pipe or
  device still goes chunked).
- Each hop records the request it put on the wire in `HopTimings.wire` (`WireRequest`: method,
  URL and the headers sent, in order, names lowercase like `Response.headers`). The transport
  builds the `reqwest::Request`, then adds what `reqwest` and `hyper` add before the bytes leave,
  by the same rules and in the same order: the client defaults `Accept: */*` and `User-Agent`,
  the decompression layer's `Accept-Encoding: zstd,gzip,deflate,br`, `Host` (with the port when
  it is not the default), and `Content-Length` for a known non-empty body or
  `Transfer-Encoding: chunked` for a stream, each only when the request lacks it. Raw-TCP
  capture tests (`tests/sent_hops.rs`) pin that the record equals the HTTP/1.1 request head. Gaps:
  an HTTP proxy's `Proxy-Authorization` is not recorded, over HTTP/2 `host` stands in for
  `:authority`, and a request that sets its own `Transfer-Encoding` keeps the `Content-Length`
  that `hyper` would drop.
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

`reqflux` with no arguments opens the window. `reqflux run` sends one request, or runs a
collection or folder, through `reqflux_engine`, the same path the UI and MCP use:

```text
reqflux run <target> [--workspace <dir>] [--environment|--env <slug>] [--var <key>=<value>]... [-i|--include] [--bail] [--json]
```

- `<target>` is the id or slug path of a request, a folder (`<collection>/<folder>/...`) or a
  collection (`<collection>`); a request wins over a folder at the same path. `--include` is for a
  request only; `--bail` and `--json` are for a run only, and using them otherwise is a usage
  error (exit 2).
- A collection or folder runs through `Engine::run_collection`. stdout gets one line per request
  as it finishes, `<METHOD> <name> -> <code> <reason> in <ms> ms, <size>, tests <passed>/<total>`
  or `<METHOD> <name> -> error: <message>`, plus `  fail <test>: <message>` per failing test, and
  ends with `<n> requests, <n> failed; tests <n> passed, <n> failed; <ms> ms` and, when the run
  ended early, `; <why>`. `--json` prints one document instead (`target`, `passed`, `end`,
  `next`, the counts, `elapsed_ms` and `results` with each step's method, masked URL, status,
  time, size, tests and error). Exit 0 when every request got a response, every test passed and
  the run completed or `pm.setNextRequest(null)` stopped it; 1 otherwise; 2 for usage. Ctrl-C
  cancels the run and still prints the report. A run writes nothing to the workspace or the app
  state, so it records no history.
- For a single request:
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
| 1 | The request could not be executed: transport error, unsupported auth, auth that cannot be completed (`authorization failed: …`), environment not allowed for the collection, invalid GraphQL variables. |
| 2 | Usage error: bad arguments, unknown request or environment, workspace that fails to load. |

Secret variables have no value in workspace files; a valueless secret resolves from the OS
keyring, and `--var` still wins over it. When the keyring cannot be read, stderr has one
`warning: secret variables were not read from the keyring: <why>` line after the load
warnings, those secrets stay unresolved and the request is still sent. A `--var` value is
visible in the process list and the shell history; prefer `reqflux secret set`, or reading it
from the environment, such as `--var token="$API_TOKEN"`.

`reqflux code` prints the code that sends a request:

```text
reqflux code <request> [--lang curl|httpie|python|fetch] [--workspace <dir>]
```

- `<request>` is an id or slug path; `--lang` defaults to `curl`. stdout gets the code, stderr
  one `warning:` line per workspace load warning. Exit 0, or 2 for usage (unknown request or
  language, workspace that fails to load).
- The code is the request as written, through `Engine::code`: the auth it inherits from its
  folders or collection, every `{{variable}}` left as `{{name}}` (secret values are never
  read), disabled rows left out, and the `Content-Type` the engine would add for a raw or
  GraphQL body written when no header sets one.

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
`bearer`, `apikey`, `digest`, `awsv4` and `oauth2` fail with `auth type <type> is not supported
yet`.

Digest, AWS SigV4 and OAuth 2.0 stay `Auth::Other` in the model and are read through typed views
(`Auth::typed`); `secret_fields(kind)` names their credential keys, which history and every
masked view hide. Editing a typed auth field (`Auth::with_typed`) rewrites only that key, under
the spelling the file already uses (a Postman alias such as `accessKey` stays); unknown keys are
kept. Only a newly picked auth type is written with the canonical keys in canonical order. Digest answers a `401` challenge once (RFC 7616: MD5, SHA-256, `-sess`,
`qop=auth`). SigV4 signs every header plus `host` and `x-amz-date`, and the payload hash, when the
send starts, so history records the signed request. OAuth 2.0 tokens live only in the OS keyring
under `<workspace>/collection/<collection>/oauth2/<request|folder|collection>/<id>` and are
refreshed before a send once expired; the authorization code grant uses PKCE S256, opens the
system browser (`Engine::set_browser` replaces it) and waits up to 300 s for a loopback redirect
on `127.0.0.1:<random>/callback` with a matching `state`. A `401` to an OAuth token drops it (a
kept refresh token stays, marked expired); a token that came from the keyring is retried once
with a new one. An `Authorization` request header still wins. `Engine::oauth_token_state`
reads whether a token is kept and when it expires, without the token; `Engine::fetch_oauth_token`
obtains a new one without a send and returns an `OAuthFetch` whose `cancel` stops it, loopback
listener included.
`Engine::prepare_draft` prepares a draft request document in the place of the stored one,
keeping the stored request's parents (and with them the scopes and the environment
allowlist), and `Engine::execute_draft` sends it.

`Engine::inspect` is what a front end shows before sending, against the active environment or
none: the selected request with its scripts, the URL as written with the enabled params
appended after the separator `prepare` uses, the names `prepare` reports unresolved with that
environment, and `Inspection::spans`, which splits a text into literal runs and `{{name}}`
references marked as resolving (globals, collection, environment or dynamic variables) and as
secret (secret in the globals, the collection or an environment the collection allows).
`Engine::inspect_draft_current` does the same for a draft in the place of the stored document, so
the window's unresolved notice counts what a send of the draft would report; the window inspects
again after every draft edit, undo and redo. When
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
- `pm.response.to` is Postman's chai-style chain: `to.have.status(code | reason)`,
  `to.have.header(name[, value])`, `to.have.body(text | RegExp)`,
  `to.have.jsonBody([path[, value]] | object)`, `to.be.ok` (200), `success` (2xx), `error`
  (4xx or 5xx), `clientError`, `serverError` and `json`, and `not` anywhere in the chain flips
  the verdict. A failure throws Postman's message (`expected response to have status code 200
  but got 404`), which `pm.test` records.
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
- `pm.setNextRequest`, `pm.execution.setNextRequest` and `postman.setNextRequest` record where a
  collection run goes next in `ScriptOutcome::next_request`: a string names a request by name or
  id, `null` stops the run, the last call in a script wins, and any other argument throws like
  Postman. Pre-request and test scripts both record it; a single send ignores it, and the runner
  resolves the name or id.
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
- Descriptions map to `description`: `info.description` of the collection, `description` of a
  folder item, and `request.description` or the item's `description` of a request (the request's
  wins), each a string or `{content}`. Export writes them back to `info`, the folder item and the
  `request`. A blank or non-text description stays in the kept Postman text.
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

`reqflux_engine::export` writes Postman v2.1 (`Engine::export_postman`): the entity's
`extensions.postman` is restored, saved examples become `response[]` and the auth, script and
variable mappings mirror the import, so an import, export and import again yields the same
workspace. Secret variable values are written empty unless `include_secrets` asks for them in
clear; then they are read from the keyring and the result carries their names.

`reqflux_engine::bundle` writes the bundle: `reqflux-bundle`, format version 1, the kind
(workspace, collection, folder, request or environment) and the entity files as the store writes
them, scripts and saved examples included. With `BundleSecrets::Encrypt` the secret values are
read from the keyring and encrypted as one age section (scrypt recipient from the passphrase,
ASCII armor); otherwise they are left out and listed in `secrets_omitted`. The passphrase comes
from stdin, never from an argument, and an empty one is refused. `import_bundle` checks every
target file before it writes: one that exists fails the whole import with
`BundleError::Exists`, and the target's own keyring receives the decrypted secrets.

`reqflux_engine::docs` renders a collection as Markdown and HTML (`Engine::docs`): requests
grouped by folder, method, URL, params, headers, body and auth type as written, the collection's
descriptions and its saved examples. A `{{variable}}` stays as written, so no value reaches the
pages.

The CLI exposes them as `reqflux export`, `reqflux bundle create|import` and `reqflux docs`, and
the MCP tools are `postman_export`, `bundle_export` and `docs_generate` (reads, they return the
document) with `bundle_import` (a write into the open workspace; it never replaces a file).
A file the CLI or the window writes goes through `reqflux_engine::write_export_file`: a new
file unless forced (`--force`, or the window's Replace), and mode 0600 when it holds secret
values in clear; `PostmanExport::text` is the document as that file holds it.

Because the workspace is already a directory of text files, sharing a whole workspace through
git needs no export at all.

A pasted curl command becomes a request through `reqflux_core::curl::parse`: bash and Windows
`cmd` quoting (Chrome's and Firefox's "Copy as cURL", Postman's export), method, URL and query,
headers, `-d`/`--data-*`/`--json` as raw, urlencoded or binary, `-F` as form-data with files,
`-u` basic, `--oauth2-bearer`, `-k`, `-L` and `-m`. Unknown options and options with no place in
a request are returned as warnings, never dropped silently; output-only options (`-s`, `-v`,
`--compressed`) are accepted. Without `-L` the workspace default for redirects applies, so a
browser copy does not turn them off. `reqflux_core::codegen` writes a request back as curl,
HTTPie, Python requests or JavaScript fetch.

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
and test results; it records no history and keeps no script writes); `collection_run` (target
by id or slug path, `environment`, `bail`: runs the collection, folder or request through
`Engine::run_collection` and returns per step the method, name, masked URL, status, time, tests
and error, the totals, `passed` and how the run ended; every send is recorded in the window's
history, a request that was not sent is not); `history_list` and
`history_get` (the window's history of this workspace, read-only); `environment_list`,
`environment_create`, `environment_rename`, `environment_delete`; `variables_get`,
`variable_set` and `variable_delete` by scope `global`, `collection:<id>` or `environment:<id>`.
The handlers run on the blocking pool under one lock; `request_execute` holds it for the whole
send, `collection_run` for the whole run, and a tool that panicked does not poison it for the session. The engine, which owns a
Tokio runtime, is dropped on its own thread, never inside the async server.

Secrets never leave the engine. A secret variable shows `{"set": true|false}`, never a value.
Basic passwords, bearer tokens, API key values, every field of an unsupported auth type, the
`secret_fields` of digest, AWS SigV4 and OAuth 2.0, and
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
`font_size` (the interface font size in px, 13 by default, clamped to 8..=32 as in DBFlux; App >
General and the interface size commands write it) and
`[mcp]` and `[network]` (certificates and proxies are per user and machine, never in a
workspace file; see below). `reqflux_engine::config` edits `font_size`, the `[mcp]` rules and the
`hosts`/`environments` lists the same way (`set_font_size`, `set_rule`, `add_list_entry`,
`remove_list_entry`); removing the last list entry removes the key (no list means any). The
`[network.proxy]` `mode` and `url` are edited by `set_proxy_mode` and `set_proxy_url` (a blank
URL removes the key); they create `[network.proxy]` after what the file holds and leave
`[network]`'s other entries, the per-workspace proxies and unknown tables as written. The proxy
password is never written there: it stays in the keyring. `set_proxy_url` refuses a URL that
carries a password (`ConfigError::PasswordInUrl`, nothing written); `set_proxy_url_with_secret`
is the caller that splits it: the password (percent-decoded) goes to `network/proxy` first, then
the file gets the URL without it and the user name as `username`, so a keyring failure writes
nothing. `set_proxy_password` is the writer of the Password field: it writes the keyring entry
`network/proxy` (an empty password removes it), then sets `username` from the user of `url` when
the file has none, because sends use the keyring password only with a `username`; a file that
does not parse or a keyring failure changes nothing. No error, log or toast carries the password.
`[keymap]` (`"save-request" = "ctrl-shift-s"`; only bindings that differ from the defaults are
kept) rebinds the window's commands (the registry in `shell/commands.rs`): the window reads it at
startup and after every edit, an unknown slug or a chord it cannot bind is a warning in the
Problems panel, and when two entries claim one chord the first command in registry order wins.
The App > Keymap section of the settings page follows DBFlux's Keyboard shortcuts page: a filter
(`keymap-filter`; it matches the label, the slug or the key as printed) and a context select
(`keymap-context`) above the registry, grouped by context under a header that folds
(`keymap-group-<context>`; a filter or a context opens every group), each row with its chord as
one keycap per part (`keymap-chord-<slug>`) and a pencil (`keymap-record-<slug>`); the
`[keymap]` block of config.toml is a collapsed, read-only section at the foot
(`keymap-config-toggle`) that opens into a block bounded at five grid rows (`keymap-config`). The page
captures a new chord on a click of the pencil or the keycaps
through bolt's `KeyCapture` (the chord never reaches the keymap; a bare letter keeps it
listening; Escape cancels, Delete or Clear returns the default, Reset to defaults removes the table) and
writes the file at once with `set_keymap_binding`, `remove_keymap_binding` and `clear_keymap`.
A missing file is the defaults; a file that does not parse is reported (path, line and
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

[network]
ca_bundle = "/etc/ssl/corp-ca.pem"        # absolute; ~/ expands
[network.proxy]
mode = "manual"                           # "system" (default) | "none" | "manual"
url = "http://proxy.corp:3128"            # http, https, socks5, socks5h
username = "me"
bypass = ["localhost", ".corp"]           # NO_PROXY syntax
[[network.certificates]]
host = "api.example.com"                  # "*.x" matches subdomains only
port = 8443                               # optional: any port when absent
cert = "/c/client.pem"
key = "/c/client-key.pem"                 # or: pkcs12 = "/c/client.p12"
[network.workspaces."01J00000000000000000000900".proxy]
mode = "none"
```

Passwords and passphrases live only in the OS keyring, service `reqflux`: the proxy password
under `network/proxy` (a workspace's under `network/workspace/<id>/proxy`) and a certificate
passphrase under `network/certificate/<host>[:<port>]`. A `[network]` entry that does not read
or has a relative path is skipped with a warning that never carries a secret.

The engine tools are policed (`reqflux_engine::policy`); every tool has a class: read
(`workspace_open`, `workspace_info`, `request_get`, `history_list`, `history_get`,
`environment_list`, `variables_get`, `cookie_list` with values masked, `postman_export`,
`bundle_export`, `docs_generate`), write (`workspace_create`, `collection_create`,
`folder_create`, `request_create`, `item_rename`, `item_duplicate`, `item_move`, `request_update`,
`environment_create`, `environment_rename`, `variable_set`, `bundle_import`), execute (`request_execute`, `collection_run`) and
destructive (`item_delete`, `environment_delete`, `variable_delete`, `cookie_delete`,
`cookie_clear`). Each class is `allow`,
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
class or the host, the rule and the file and table to change it in. `collection_run` checks
every request of the run, its script sends and redirects through the same guard; a refused
request is not sent and fails its step, and the run goes on unless `bail` is set.

Every write, execute and destructive call, allowed or refused, appends one JSON line to
`$XDG_STATE_HOME/reqflux/audit.log` (`reqflux_engine::audit`; owner-only, appended and synced per
line): time, tool, class, decision, reason, workspace id, the ids it targets and, for an
execution, the method and the masked URL. Argument values (documents, variable values, bodies)
are never written, and a call whose audit line cannot be written is refused. Read tools are not
audited. `collection_run` writes one line for the run (the target id, no URL) before anything is
sent, then one per request with its method and masked URL, refused with the refusal when the
guard refused it, one of its script sends or a redirect; a request line that cannot be written
stops the rest of the run.

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
For `collection_run` the target reads `<name> · N requests`, with the target id in `item_ids`.
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
`approvals-empty` (its text wraps inside a narrow detail pane). With the list focused `j`/`k` and the arrows move, `a` allows once, `s`
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
`-24h` by default, `-7d`; an entry whose time does not parse is always kept), `bolt`'s
multi-selects `audit-class` (read, write, execute, destructive) and `audit-outcome` (allowed,
refused), which keep the entries of any value they hold, all of them when they hold none, and
say so on their trigger (`Class · all`, `Class · 2`), and, while a filter is set, `audit-clear`. The toolbar wraps onto more
lines when the page is too narrow (its height is a minimum), so the time range, Class and Outcome stay inside the page
at an 800 px window. Under it, `audit-timeline` (the strip's region, with its own ground and
border) holds `bolt`'s `TimelineHistogram` `audit-histogram`: 48 bars over the range
(`audit-histogram-bar-<n>`, its `-rest` and `-highlight` segments), the refused calls stacked on
the others, and the legend (`audit-histogram-legend`, its `-swatch-<n>`). The table has a row per kept entry, `audit-row-<n>` from 1 at the top:
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
`workspace-new-popover` beside the menu, its top at the menu's bottom, with the title, the field
`workspace-new-name`, the hint `workspace-new-hint`, `workspace-new-cancel` and
`workspace-new-create`; Enter or Create makes a workspace kept by the
app and opens it, Escape, Cancel or a click outside closes it and creates nothing),
`New workspace in folder…` (`workspace-new-folder`, the OS save dialog;
`reqflux_engine::create_workspace` writes a `reqflux.toml` named after the directory into a
missing or empty directory), `Open folder…` (`workspace-open`, the OS folder dialog),
`Import Postman collection…` (`workspace-import`, while a workspace is open: the import dialog
`import-backdrop` of `design/bolt-v2/11-import-postman.html` asks for a collection file, shows
what the import would write through `reqflux_engine::postman_preview` (the counts
`import-count-<kind>`, the files `import-file-<n>`) or why it cannot import (`import-error`), and
`import-confirm` writes it into the open workspace with `reqflux_engine::postman` and reloads
it; Cancel or Escape writes nothing), `Export workspace…` (`workspace-export`, the Export…
dialog on the whole workspace, bundle only),
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
sidebar `Island`, tree rows as `TreeNavRow` (the selection bar, a guide per indent level, the
chevron `tree-row-<id>-chevron`, icon `-icon` and count `-count`; an example's status code
`tree-status-<id>` and the row actions sit in its trailing slot `-trailing`), document tabs,
`Button`
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
(collection, folders, request), and the request. A request that the workspace tree does not
hold or the engine cannot inspect opens no tab: the window logs it and shows the error toast
`Cannot open the request` with the reason. The request is shown editable as "Drafts and saving" describes:
an action strip (Save and Discard changes, live while the request has a draft; Undo and
Redo, live while its history has a step that way; Split, not built yet; every tooltip names the
action and its key), the request line (method, URL with the
enabled params, Send), and the Docs, Params, Headers, Body, Auth,
Scripts and Settings sub-tabs. Docs (`request-tab-docs`, first, like Postman; a request still opens
on Headers) shows the request's markdown description rendered (`request-docs-view`, selectable)
with Edit, or the empty state `No documentation` with `Add documentation`; the editor
(`request-docs-input`) puts every keystroke into the draft, Done closes it and Ctrl+S saves the
description with the request (design/docs/01, `shell/docs.rs`). The collection's settings section
carries the same Documentation block (`collection-docs-*`) between the Environments block and the
variables; it has no draft, so Save writes `collection.toml` through `Engine::set_description` and
Cancel or Escape drops the text. A folder has no settings page yet: its description is edited
through the engine and the Postman import only. The rendered description is `bolt`'s
`MarkdownView` under the block's `-view` id, its text the element `-view-body`, selectable by
default and in a well of `Spacing::MD` sides and `Spacing::SM` above and below. What it shows comes from `Engine::inspect` with the window's
active environment, which lives in memory, starts as none and re-inspects every open request
when it changes, keeping its sub-tab: variables are styled as resolved or unresolved, the
number of unresolved ones is shown in the sub-tab bar, or the inspection `problem` in its
place (`request-problem`), and a value that references a secret, a password, a token, an API key value or
any field of an auth type the engine does not support is masked as `••••••••` on screen and
in the automation tree. Unresolved references use the danger role everywhere (the marks and
the row wash, the `N unresolved variables` note, the popover's "Not defined"), not warning.
The Scripts pane's pre-request and test scripts, read-only and in their editor, are
highlighted as JavaScript; the Document layout's `.http` editor stays plain (no http grammar,
and a JSON body inside a section needs an injection or a head/body split: later).

Each `{{name}}` mark in the URL (`request-url-var-<n>`), a Params, Headers or form value cell
(`request-<table>-value-<row>-var-<n>`) and an auth field opens the variable popover
(`variable-popover`, design/variable-popover/01; `shell/variable_popover.rs` and
`request/variable_popover_view.rs`) after `bolt`'s 400 ms hover delay or on a click; a click on
a mark opens it instead of editing the field, and a click elsewhere in the field still edits it.
The surface is `bolt`'s `Popover`, whose state the shell owns: it points at the mark's
rectangle in window coordinates (each mark's layout records it, and the shell keeps one anchor
per mark the frame draws: a mark records its rectangle with the frame's number and the frame,
which ends after its children are laid out, drops every anchor no mark recorded, so a closed
request or a tab that left the screen leaves none behind), lands one `ANCHOR_GAP` under it,
flips above it near the window's bottom edge and keeps a margin from the edges. `bolt` owns the
click-away layer under the surface (so a click inside it, or on a list opened from its body, is
not outside), the focus in and back, and the 120 ms grace that closes a hover after the pointer
leaves both the mark and the surface. ReqFlux answers Escape itself, on the surface's body: it
installs no `bolt` `CommandSet`, and its fields bind Escape to gpui-component's `InputEscape`,
a binding that consumes the key before any key listener of the surface runs.
While the field is edited, F2 or Alt+click on a reference under the caret opens it under the
field. It shows what `Engine::variable_info` reports: the winning scope's badge, its current
value in a focused field (`variable-popover-input`), the initial value read-only and the
single "Overrides" line, with Copy, Reset to initial (`variable-popover-reset`) and Open in
Environments, which opens the settings page at that scope. Enter saves the field as the
current value of the winning scope (an empty field clears it); a secret shows its SECRET
badge and an empty masked field, the eye (`variable-popover-reveal`) reads its keyring value
in, and Enter writes the keyring. An undefined name offers the quick add: the "Add to"
dropdown (`variable-popover-target`, `bolt`'s `Dropdown`; its menu `variable-popover-target-menu`
opens above the surface, its scrolling column of rows `variable-popover-target-menu-rows` and
its row `n` from 0 `variable-popover-target-item-<n>`) over every environment the collection allows (`VariableInfo::targets`:
the active one first, marked "active" and preselected, then the others by name), then the
collection (the default with no active environment) and the globals, the value, which becomes
the initial value, and the Secret box; "Not defined" is danger red. With no active
environment, a name only environments declare lists them to activate, with an "Add to
collection or globals instead". Escape closes the "Add to" list first, then the popover, and a
click outside closes it without writing; after a write every open request is inspected again,
so the marks follow. A cell whose value references a secret stays
masked and has no marks; F2 while editing it still opens the popover.

The marks stay while the URL field, a value cell field or a plain auth field is edited
(`shell/input_marks.rs`, design/variable-popover/01 state f): `bolt::controls::decorate_input`
styles the reference ranges of the field's text with `request::spans::mark_style`, the style
the read-only marks draw, and whether each resolves comes from `Inspection::spans`. The
ranges follow edits but are never detected, so they are set again from the text on every
`InputEvent::Change`, after the URL field is re-seeded from the grid, and whenever the open
requests are inspected again. Key cells, secret fields and the settings and method fields
carry no marks.

Send or Ctrl+Enter sends the active request with the active environment through
`Engine::execute`. The UI awaits the execution's `JoinHandle` on GPUI's executor with
`cx.spawn` and never calls `Engine::wait`, which blocks. While it is in flight, Send turns into
Cancel; Cancel or Escape cancels it through its token, sending again cancels and replaces it,
and closing the tab cancels it and drops its response. The response sits under the request:
the Response tab with the status chip colored by class and the time and size, Tests and
Console tabs beside it when the scripts produced results, the Body / Headers / Cookies views as
a segmented control plus Timeline (`Received.timings` per hop on one time axis, each measured
phase a `bolt` `TimelineStrip` bar named by its phase and time, TLS inside connect, a dash for a
phase the engine did not time), a JSONPath filter beside the body search, both open as `bolt`'s
`SearchField` with its count
(`reqflux_engine::jsonpath::filter`: the matches replace the shown lines and search, Copy and the
line count follow; an error shows the engine message in a danger banner over the unfiltered
body), a bottom bar under the body with the Pretty / Raw / Preview switch and the
line count (Pretty re-indents JSON keeping key order), the body split into lines once
off the UI thread and drawn with `uniform_list` so only visible lines render, colored by
`bolt::syntax::highlight` (design/syntax/01: JSON when the body parsed as JSON, XML or HTML by
the content type; Raw JSON stays plain) with the search matches laid over the syntax runs by
`bolt::syntax::overlay`. The runs are computed once per shown lines and palette (the view
state caches them) and split by line; a body over 1 MiB (`HIGHLIGHT_LIMIT`) is drawn plain,
an error banner
with Retry, and the unresolved variables the send used. Each open request keeps its last
response in memory, and every send is recorded in history (see "History"). A request's saved
examples are its children in the tree (a leaf with the status code), opened read-only in the
example tab; Save as example and Export → Save response body to file… sit in the response
footer and write through `Engine::save_example` and `Engine::save_response_body`.

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
The key and description columns resize: their header cells (`request-<table>-header-key`,
`request-<table>-header-description`) carry the resize handle on the right edge, and the widths
(`shell::grid_widths`, which maps the grid kind to a table id) are kept per grid kind (Params,
Headers, urlencoded, form data), so every request shares them for the session. The value
column keeps filling the rest, but never under `GRID_VALUE_MIN_COLUMN` (160 px): a request pane
narrower than every column (`GridWidths::min_width`) scrolls the grid sideways in
`request-<table>-scroll` instead of the value collapsing under the cells beside it (the
description drawn over the value, L69). Every other table resizes the same way, see "Table
column resize".

### Table column resize

Every table that has a header row with columns resizes through one mechanism,
`shell::table_widths`. A table has a stable id and numbers its resizable columns from 0 in the
order it lists them; the widths the user dragged live in a global (`TableWidths`) keyed by
`(table id, column)`, for the session, never in a file. A view uses three functions and no
drag code of its own: `width(table, column, default, cx)` for a column's width (the default is
the view's token until the column is dragged), `resizable(root, table, cx)` on the element that
holds the whole table, which follows the drag with `bolt`'s `track_column_resize` and redraws,
and `with_handle(cell, table, column, default, cx)` on each fixed header cell, which adds
`bolt`'s `column_resize_handle` on its right edge. The width is the same everywhere the table is
drawn (the variables grid and the undeclared rows under it share one), a drag changes only that
column and `bolt` clamps it to 50 px. The column that fills the rest stays flex and has no
handle, as do the narrow fixed columns (row number, enable box, actions).

A table never lets the column that fills the rest collapse: it gives that column a minimum
(`GRID_VALUE_MIN_COLUMN`, `COOKIE_FLAGS_MIN_COLUMN`, `VARIABLES_DESCRIPTION_MIN_COLUMN`,
`AUDIT_TARGET_MIN_COLUMN`, `APP_SETTINGS_OVERRIDE_LISTS_MIN`), sums it with the other columns as
the user left them, and puts its header and rows in `table_widths::overflow` with that width:
`bolt`'s `GridOverflow`, one viewport (`<table>-scroll`, `request-<table>-scroll`) that scrolls
the two sideways together on a handle the store keeps, so a column stays over its own cells at
every offset. While the columns are wider than the pane the rows end one
`GridMetrics::SCROLLBAR_WIDTH` (12 px) above the viewport's edge and the horizontal bar draws in
the strip left free, the way DBFlux's data table draws it; a table that fits reserves nothing
and keeps its full height. Only the horizontal axis belongs to the viewport, so a vertical
wheel stays with the pane or page and the vertical scroll stays with the table's own rows. The
audit table, the runner results grid, the cookie jar and the response Cookies table take it
with the height of the pane they are given (`GridOverflow`'s viewport is `size_full` under a
`flex_1().min_h_0()` wrapper), since each is drawn in a pane that has a height of its own. The
pages that take their content's height — the request grids, the variables page, the app settings
page, the history grid — take it with `.content_height()` and chain no height instead: the
viewport then takes the height of its header and rows, still scrolls them sideways on the same
handle, still reserves the strip only while the columns overflow, and lets a vertical wheel
scroll the page around the grid, because the rows do not scroll vertically on their own there.
A viewport with no pane to measure reads the width it was given from the frame just drawn, so
the strip settles in the frame after the page's width first reaches the grid — the frame gpui's
own scrollbar settles in — and a grid that fits draws as it does unwrapped on every frame.

The global rather than shell state is deliberate: the response Cookies table, the recorded
history grid and the audit table draw from `&App`, with no shell to ask.

| Table | Id | Resizable columns | Fills the rest |
| --- | --- | --- | --- |
| Request grids | `request-params`, `request-headers`, `request-urlencoded`, `request-form-data` | key, description | value |
| Variables document | `variables` | key, initial value, current, stored in | description |
| Cookie jar | `cookies` | domain, path, name, value, expires | flags |
| Response cookies | `response-cookies` | name, value, domain, path, expires | flags |
| Recorded request (history) | `history-grid` | key | value |
| Runner results | `runner-grid` | all seven | none |
| Audit log | `audit` | time, class, tool, workspace, outcome | target |
| MCP policy overrides | `overrides` | workspace, the four rules | lists |

Not column tables, so without handles: the response Headers and the example request and
response grids (a name and a value list with no header row), and the keymap (commands grouped
under folding contexts). The runner grid's header is `bolt`'s `render_header`, which takes the
handles natively through `HeaderResize` (`table_widths::header_resize` gives it the shared drag
and wash).
The request and response panes draw with DBFlux's `bolt` (`bolt::colors`, rem tokens, `Button`,
`Badge`, `Checkbox`, `BannerBlock`, `EmptyState`, document tabs, `menu_frame`/`menu_row`,
`field_row`), so their sizes are DBFlux's: grid header 40 and rows 31, code lines 22, URL
field and Send 30, result rows 28, body-format footer 36. The closed body search and JSONPath
fields are `bolt`'s `SearchTrigger`, so the views row has no `// bolt gap:` stand-in left.

The views and overlays draw with DBFlux's `bolt` too. The MCP approvals view uses
`ApprovalsMetrics`, DBFlux's own approvals sizes (pending list 380 wide, 52 px detail title,
22 px between sections, 14 by 16 payload padding), `ListRow` rows with the selection bar,
`Badge` for the class and `Pending`, `Kbd`, `Input` for the reason field and `Button` for
Refresh, Deny, Allow for this session and Allow once. The history tab is a document tab, its
banner DBFlux's 34 px editor banner, its grid `GridMetrics` (header 40, rows 31), the table's
header and rows in `table_widths::overflow` with `.content_height()`, since the history page
takes its content's height; the History
panel uses the tree's `ShellMetrics` and `TreeMetrics` (header and footer 46, rows 26) and
`EmptyState`. The command palette is DBFlux's (`PaletteMetrics`: 56 px query row, 38 px rows
and footer) with document tabs as scope tabs. The settings page draws `Island`s, `ListRow`
nav entries and environment rows, the variables grid at `GridMetrics`, `Checkbox`,
`SelectTrigger` with `menu_frame`/`menu_row` (a `MenuItem::detail` line per option) for
`stored in`, and `Button`. The workspace and environment menus are
`menu_frame`/`menu_row` (30 px rows, 44 px recent rows) with `render_menu_header`. No
`// bolt gap:` stand-in remains in them.
`reqflux_ui::theme`
installs `bolt`'s `ModalSeams` as DBFlux's theme does: ReqFlux's modal actions bound in the
`Modal` key context (Escape cancels, Enter confirms, the arrows, Page Up, Page Down, Home and
End scroll), the `Esc` keycap and the close button, so every `bolt` modal answers its keys.

The run document (design/runner/01-run-document) is a document tab of its own, `runner-tab`,
opened from the tree's `Run…` on a collection or folder, `Run collection…` (Ctrl+Alt+R, for the
collection or folder selected in the tree) or the collection settings' `Run…`. The setup sets
the environment for this run only and Stop on first failure; the engine has no delay between
requests. Run calls `Engine::run_collection` on a thread of its own with a second connection to
the app state (`AppState::path`), so every send is recorded in history; its events reach the
window through a channel the document polls. Stop cancels the run's token. The results grid is
`bolt`'s grid, its header cells column headers named by their columns
(`runner-grid-header-cell-<n>`), under `bolt`'s determinate `Progress`; the header and the
`uniform_list` of rows sit in `table_widths::overflow`'s viewport (`runner-grid-scroll`), so a
document narrower than the seven fixed columns scrolls them sideways together and keeps the
last column reachable, with `bolt`'s bar in a 12 px strip under the rows.
selecting a finished row shows its tests beside the response history recorded for that send
(the last send when a jump ran it twice), and names the variables that step sent unresolved
above its tests. The setup's order list heads each folder's requests with the folder and its
count; requests at the collection root sit under no header. When the run ends, the current values its scripts
changed (`RunSummary::current` against the values it started with) are persisted like a single
send's script writes.

`shell/codegen.rs` draws the Generate code dialog (design/codegen/01-dialog-and-toast) on
`reqflux_engine::code`: the request line's `</>` button, Ctrl+Shift+C, the palette or the tree's
`Generate code…` open it, a segmented control picks curl, HTTPie, Python requests or JavaScript
fetch, and Copy (Enter) puts the code on the clipboard. Every `{{variable}}` stays as written.
For a draft it swaps the draft into a copy of the workspace, so the auth it inherits matches what
a send would use. The code is `bolt`'s `ModalCode` with line numbers (`codegen-code-line-<n>`),
colored by `ModalCode::language` (curl and HTTPie `bash`, Python requests `python`, JavaScript
fetch `javascript`), with its `{{variable}}` references in bold tint when they resolve and in
bold warning (`highlights_with_tone`, `BadgeTone::Warning`) when they do not.
`shell/export.rs` draws the Export… dialog (design/export/01-one-export-dialog) on
`Engine::export_postman`, `Engine::bundle` and `Engine::docs`, the calls the CLI makes. A tree
row's `Export…`, the workspace menu's `Export workspace…`, an environment's `Export…` and the
palette (`export`: the selected tree row, else the active request; `export-workspace`) open it,
and the title names the scope. The format control draws every format: Postman v2.1, ReqFlux
bundle and API docs, the ones the scope cannot take disabled (`SegmentedItem::disabled_reason`)
with a tooltip that says which scope takes it (API docs cover a whole collection, a Postman file
holds one collection, folder or request, a bundle holds requests and not an environment), so the
dialog shows what each scope exports instead of hiding it. Include secret values shows the
plaintext warning; a bundle leaves secrets out by default and lists them, or encrypts them with a
passphrase typed twice in masked fields. The secrets choice is one `OptionGroup`
(`option-group:export-secrets`, named "Secrets in a bundle", its options
`option-group:export-secrets:<omit|encrypt>` and a `:description` under each), which the arrow
keys move and assistive technology reads as a radio group. An empty or mismatched passphrase is
the field's own error (`Input::error`, the danger frame line and the alert caption
`export-passphrase-error` or `export-passphrase-confirm-error`) and is refused before the save
dialog; the passphrase never reaches a log, a toast or an error. The primary button
closes the dialog and asks the `ExportHost` seam (the OS save or folder dialog; tests answer it)
where to write. Files go through `write_export_file` with force, since the OS save dialog has
already asked before replacing a file (the CLI still refuses without `--force`); a file with
secrets in clear is 0600, a replaced one included. The passphrase fields stay secret inputs
while Show draws their text, so it never reaches the automation tree either. API docs that exist ask `Replace API docs?` once and Replace files writes with
force. The success toast carries the path and Show in folder (`App::reveal_path`; Open
index.html for docs, `App::open_with_system`); a failure keeps the error and offers Try again….
Every control it draws comes from `bolt`; no `// bolt gap:` stand-in remains here.
`shell/curl_paste.rs` catches the URL field's Paste action: a curl command goes through
`reqflux_core::curl::parse` and `Parsed::apply` onto the draft, a toast lists what was imported
and the options that were ignored, and its Undo action restores the previous draft. Any other
paste is URL text.

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

Every row of the response pane (tab row, views row, bottom bar) wraps instead of overflowing,
down to the 120 px the splitter leaves the pane (`SPLIT_PANE_MIN`): the JSONPath filter and the
search drop to a second line, each 240 px wide or the pane's width if that is less; the time and
size and the bottom bar's buttons follow. The tabs (`response-tabs-scroll`) and the views
control (`response-views-scroll`) never cut a chip: each is a viewport at most as wide as the
pane that scrolls sideways (wheel or trackpad, no scrollbar), `bolt::composites::scroll_strip`;
the selected chip is scrolled into view when the selection or the pane's width changes
(`StripReveal`: the tab row by child index, the views row and the format switch by segment id,
measured through `SegmentedControl::segment_bounds`, so the selected segment is revealed
exactly), so every tab and view down to Timeline is reachable at 120 px. A viewport is inside
the pane and a chip's drawn part is inside the viewport. The bottom bar's Pretty / Raw / Preview switch is
the same kind of strip (`response-format-scroll`), so it stays inside the pane at 120 px and every
segment is reachable. Under `RESPONSE_COMPACT_WIDTH` (360 px) Copy, Save as example and Export are
icons with a tooltip; the pane measures its own width into `ViewState::pane_width`
(design/response-views/04).
The search, the filters and the focus live with each request's send state.

The rail's History button (`rail-history`) swaps the sidebar tree for the History panel
(`history-row-<n>`, newest first, `METHOD path · status · time ago` or `failed`; the 200
newest; `No requests sent yet` when empty), and Collections (`rail-collections`) swaps back.
Clicking a row opens the read-only history tab (`history-readonly`, design/history/01) with the
recorded method, URL, Params, Headers, Body and response, without touching the request, its tab
or its draft. While the request exists its banner offers `Open source request`
(`history-open-source`) and `Resend` (`history-resend`), which sends the recorded document again
through `execute_draft` and records the result as a new entry; when the request was deleted the
banner says so and Resend is disabled.
In the history tab the response tab reads `Recorded response`; switching to the History
panel (the rail or Ctrl+2) gives it the keyboard focus, so `/` focuses its filter
(`history-filter`) at once; activating the history tab leaves the approvals or audit view; Next and Previous tab cycle through the history tab,
first in the row.

The settings page's App section (`design/app-settings/02-workspace-page-app-section.html`,
`shell/app_settings.rs`, `shell/app_settings_view.rs`) sits above its workspace sections and
opens even with no workspace: the rail's Settings (`rail-settings`, marked active while an App
section shows) opens it at General (`theme`, `font-size`, applied live through bolt's font
install); MCP policy holds `rule-read|write|execute|destructive` (Allow, Ask, Deny),
`hosts-list`, `environments-list` (Enter in their field adds, like the button) and the read-only `overrides-table` (`overrides-open-config`
opens `config.toml` with the OS handler, `open_file.rs`). Every control writes `config.toml` at
once (General and MCP policy say so under their title); while it does not parse, `config-error`
shows its path, line and column, the controls are dimmed and do not write (the theme and proxy
mode controls through `SegmentedControl::disabled`, the fields through `READ_ONLY_OPACITY`), and
`config-open` and
`config-reload` act. Every way into an App section (the rail, the nav entries, Network) reads
`config.toml` first (`Shell::prepare_app_section`), so a control never shows before the file was
read: a page opened at a workspace section used to leave the App controls dimmed, with the font
size select empty, until the rail's Settings was used.

The section headers of Network and Cookies wrap their buttons under the title in a narrow
window (the text column keeps `SETTINGS_HEADER_TEXT_MIN`, 240 px), and the two Network columns
stack below `NETWORK_COLUMN_MIN` (340 px) in a vertically scrolling body (`network-scroll`); at an
800 px window every control stays inside the page (`toolbar_narrow_tests`).
Header text never hard-cuts: the Cookies count wraps with its filter row, the audit and approvals
subtitles and the history banner (a minimum height) wrap instead of clipping, and
`assert_not_cut` in `toolbar_narrow_tests` checks a text is as wide as in a wide window or taller.
App > Network writes the proxy beside the keyring: `network-proxy-mode` (System, None, Manual)
writes `mode` of `[network.proxy]` and applies it to the open engine; Manual shows the field
`network-proxy-url`, which writes `url` on Enter and when it loses focus (a password typed in it
goes to the keyring through `set_proxy_url_with_secret` and the file keeps the URL without it; the
field shows a URL with a password, or whose password the keyring holds, masked, and a text that
still holds the mask is never written back). While the typed text carries a password the field is
a secret (`Input::secret`): the automation tree withholds its whole value, because a single-line
input cannot mask only the password on screen, where it stays visible until committed; the URL's
note points to the Password field, which has no such limit. `network-proxy-password` is a masked
secret input under the URL (it replaces the keyring badge row of the app proxy; a workspace's own
proxy and the certificate passphrases keep the badge and the Set dialog): it shows bullets while
typing, the tree never carries its value, and Enter or leaving it calls `set_proxy_password`. A
stored password shows as the mask and is never read back, typing next to the mask replaces it,
and clearing the field and committing removes the entry. Afterwards the field goes back to what
the keyring holds, so a typed password never stays on screen, and a keyring failure raises the
`Cannot save the proxy password` toast with nothing written (bolt's `Input` has no invalid state,
so the field draws no danger edge). While the keyring holds a password and neither the URL nor the
file names a user, `network-proxy-password-hint` says to add one to the URL, because sends ignore
the password without it. A URL the
keyring refused stays in the field minus its password (`config::url_without_password`), so only
the password is typed again. The CA
bundle, the certificates, the bypass list and a workspace's own proxy are read from the file. A
value the page cannot change draws in `bolt`'s read-only field (`ValueField`, a path in the
editor face) or, when the row holds more than one part, in a `FieldWell`. A
write that fails raises an error toast, titled after the cause (`Cannot save the proxy password`
for the keyring, `Cannot write config.toml` otherwise); with a file that does not parse the mode control is
dimmed and inert, so nothing that looks editable answers with an error.

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
`settings-description`); in a collection's section the Environments block
(`collection-allowlist-block`, `shell/settings_collection.rs`) writes the collection's
environment allowlist: the control (`collection-allowlist-all`, `collection-allowlist-selected`)
reads All environments while the list is empty and Selected only otherwise, and one checkbox per
workspace environment (`collection-allowlist-environment-<id>`) ticks it into the list; every
write goes through the engine at once and re-inspects open requests, and an empty list means
every environment. In Environments the environment list sits beside the variables:
`New environment` (`variables-new-environment`) creates one and opens its name into a field
(`variables-rename-input`, hint `variables-rename-hint`); each row
(`variables-scope-environment-<id>`, by name; the first is shown by default) has its count and
file (`variables-environment-meta-<id>`) and a `…` menu (`variables-scope-menu-<id>`: Rename,
`Delete environment…`); delete asks first (`variables-delete-prompt`; deleting the active
environment selects No environment). The variables column (`variables-document`) has a heading
(`variables-title`, `VARIABLES IN <NAME>` or `GLOBAL VARIABLES`) with the count of enabled
variables that resolve to nothing (`variables-empty`) and `Reset current values`
(`variables-reset-current`, clears the current values of the declared keys); then the grid,
row `variables-row-<n>`: selection box, number, enable box, key, initial value (in the file), current value
(this machine; `= initial` when unset, `empty` when both are), stored in (the cell
`variables-stored-<n>`, labeled `File` or `Keyring`, holds the select `variables-secret-<n>`; its
menu `variables-stored-menu` offers `variables-stored-file-<n>`, saved in the workspace file and
shared with whoever has the workspace, and `variables-stored-keyring-<n>`, a secret kept in this
machine's OS keyring, never in the file and shown masked; picking one moves the value there,
picking the current one only closes the menu, and Escape or a click outside closes it), description and
delete, plus the ghost row `variables-add` (key `new_variable`, opened with its text selected so typing names it). Variables are not drafts:
every commit writes the file (or the keyring, or the app state for a current value) at once
and re-inspects open requests. A secret's initial value reads `not in the file`; its current
value shows `••••••••` (`variables-masked-<n>`) and edits the keyring through an empty field
whose empty commit keeps it. Turning a secret off does not copy its value into the file, and
the page says so (`variables-warning`). Current values a script wrote for a key the scope does
not declare show under `ONLY ON THIS MACHINE` as `variables-undeclared-<n>` rows: the current
value edits (an empty commit clears it), `Declare` (`variables-declare-<n>`,
`Engine::declare_variable`) adds the key to the scope's file with no initial value and keeps the
current value local, and the discard control (`variables-discard-<n>`) clears it. Every cell
but the enable box and stored in edits in place: a click opens a field seeded with the cell's
text over the whole cell, Enter or a click away commits and Escape cancels. Committing the key
renames the variable (`VariableEdit::Key`): a secret's keyring entry and the key's current value
move with it, and an empty or taken key is refused. Empty values (the heading count,
the `empty` tags, the row wash and number) read in the danger tone. Globals and a collection also
copy variables into an environment: each row has a selection box (`variables-select-<n>`) and the
header one (`variables-select-all`), and `Copy to environment…` (`variables-copy`, disabled until a
row is selected) opens `variables-copy-dialog` with an environment select (`variables-copy-env`,
the active environment preselected), the policy box `variables-copy-overwrite` (off: keys the
environment already has are skipped) and, in a workspace with no environment,
`variables-copy-new-env`. The result (copied, and skipped with the reason) is the page's
`variables-warning` line. Notes name the
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
field takes keys first. `[keymap]` in the user's config.toml rebinds any command (see
`Configuration and policy`), and the palette and the menus show the effective keys.

| Command | Key | Can run when |
|---|---|---|
| Open palette | Ctrl+K, Ctrl+P | always |
| Send request, Cancel send | Ctrl+Enter, Escape | a request is open |
| Save request, Discard changes | Ctrl+S, Ctrl+Shift+R | the active request has a draft |
| Undo edit, Redo edit | Ctrl+Z, Ctrl+Shift+Z (Redo also Ctrl+Y) | a request is open and no text editor has the focus |
| Generate code | Ctrl+Shift+C | a request is open |
| Undo curl paste | Ctrl+Alt+Z | a pasted curl command is not undone yet (the paste toast's Undo shows the key) |
| Run collection… | Ctrl+Alt+R | a collection exists |
| Export… | Ctrl+Shift+X | a collection exists (the selected tree row, else the active request) |
| New request | Ctrl+N | a collection exists (in the selected collection or folder) |
| New collection, New environment | Ctrl+Shift+N, Ctrl+Shift+E | a workspace is open |
| Switch environment, Show Environments | Ctrl+E, Ctrl+3 | a workspace is open |
| Open workspace, New workspace | Ctrl+O, Ctrl+Shift+O | always |
| Close workspace, Workspace settings | Ctrl+Shift+W, Ctrl+, | a workspace is open |
| Export workspace… | Ctrl+Alt+X | a workspace is open |
| Close tab, Next tab, Previous tab | Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab | a request, the history tab or the run document is open (Next and Previous skip the run document) |
| Show Collections, History | Ctrl+1, Ctrl+2 | a workspace is open |
| Toggle theme | Ctrl+Shift+T | always |
| Increase, Decrease, Reset interface size | Ctrl+= (also Ctrl++, Ctrl+Shift+=, Ctrl+Shift++), Ctrl+-, Ctrl+0 | always (one pixel per step within 8..=32, reset to 13, saved as `font_size`; a step at a limit saves nothing) |
| Open MCP approvals | Ctrl+Shift+M | always |

Every region and tree row has a stable element id (`title-bar`, `sidebar`, `document`,
`status-bar`, `tree-row-<id>`, `doc-tab-<id>`, `request-tab-<name>`,
`request-<params|headers>-row-<n>`, `request-send`, `request-cancel`, `response-status`,
`response-meta`, `response-body-line-<n>`, `response-headers-row-<n>`, `response-error`,
`response-retry`, `status-last-request`), and text that automation must read carries a label role,
because the gpui-mcp bridge only reports text for nodes with a text role. Tree rows report
`selected` and `expanded`; tabs and sub-tabs report `selected`. `bolt`'s `Dropdown` numbers its
rows from 0 in the order it was built: an open menu `<id>-menu` holds its scrolling column
`<id>-menu-rows` and each row `<id>-item-<n>`, so automation and tests pick a row by name
instead of by where it landed. On Wayland a window renders
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
| Toasts | `design/toasts/01-toasts.html` |
| App settings (theme, font size, MCP policy, App > Network) | `design/app-settings/02-workspace-page-app-section.html` with `design/network/01-settings-sections.html` |
| Run document | `design/runner/01-run-document.html` |
| Generate code and curl paste | `design/codegen/01-dialog-and-toast.html` |
| Saved examples | `design/examples/01-tree-children.html` |
| Layout modes | `design/layouts/01-side-by-side.html` (Stacked, the default, is `design/bolt-v2/01-main.html`) |
| Request panes (stacked with the grip) | `design/request-panes/01-stacked-grip.html` |

The other artboards in those folders are directions that were not chosen, and nothing reads them.

Bolt is DBFlux's `bolt` crate, a git dependency pinned to the DBFlux rev the GPUI stack uses;
ReqFlux never copies it. Every screen draws with it, and what gpui-component draws (inputs,
scrollbars, the code editor) is DBFlux's palette with ReqFlux's accent.

| Topic | Rule |
|-------|------|
| What `bolt` owns | Everything product-agnostic: color roles and washes, typography roles, spacing, cuts and chrome metrics, the chamfer shape, the theme mapping onto gpui-component, and the components (button, field, select trigger, segmented control, tabs, menu, badge, keycap, banner, checkbox, list row, empty state, modal, splitter, island, status chip). It never names ReqFlux, HTTP or a ReqFlux type. |
| What ReqFlux passes in | `reqflux_ui::brand::reqflux()`, the `Brand` (cobalt accent fill, hover, pressed, tint, text on accent, app name), and its own tokens in `reqflux_ui::tokens` (method hues, ReqFlux-only widths); syntax colors are `bolt`'s `SyntaxColors` roles. |
| Install | `reqflux_ui::theme::install` loads `bolt`'s bundled fonts (Archivo, Archivo Expanded, JetBrains Mono) at the default interface size (13), so a rem is 16 px and `bolt::tokens` sizes are DBFlux's pixels; then the default density and `bolt::theme::install` with the brand and the mode from `config.toml`. It also installs `bolt`'s modal seams: Escape cancels, Enter confirms, the scroll keys scroll; and ReqFlux's translator into `bolt::strings` (`reqflux_ui::strings`), which answers in English every key `bolt`'s surfaces read (dropdown and multi-select, toasts, command palette, chart legend), interpolating `%{name}`, and answers an unknown key with the key itself so a missing one shows instead of rendering blank. A test lists every key `bolt` uses at the pinned rev. The first install registers the Tree-sitter grammars `bolt` bundles behind the `syntax-*` features ReqFlux enables (json, javascript, bash, python, xml, html) with `bolt::syntax::register_bundled`; a theme switch does not register them again. |
| Icons | The Lucide catalog of `gpui-kit-assets` (the one gpui-component bundles), drawn through `bolt`'s `IconSource::Svg`; `reqflux_ui::assets()` is its asset source. |
| Screens | Use `bolt` roles (`bolt::colors(cx)`, `bolt::brand::brand(cx)`) and components, never gpui-component `Theme` fields or raw colors. A guardrail test rejects color literals and bare pixel values outside `tokens.rs` and `brand.rs`, as DBFlux does. A control `bolt` lacks is added to `bolt` in DBFlux; nothing in `reqflux_ui` is left as a stand-in. |
| Theme parity | A test compares the gpui-component theme ReqFlux installs with `bolt::theme::install` alone (DBFlux's install) with ReqFlux's brand, in dark and light and after a switch: ReqFlux overrides no field. |

**Palettes.** `bolt::Colors::dark()` and `Colors::light()` are DBFlux's. `PaletteMode::{Dark,
Light, System}` is passed to `theme::install` and switched at runtime with `theme::set_mode`,
which applies `bolt`'s theme for the resolved palette; `bolt` keeps only the resolved
palette, so ReqFlux keeps the selected mode itself and System follows the window appearance.
In light the accent washes use the accent instead of the tint. ReqFlux's method colors and
`bolt`'s syntax colors read the active palette. The choice is the `Toggle theme` command (Ctrl+Shift+T, Dark,
Light, System) and persists as `theme` in the user's `config.toml` (a value from the app
state's older `settings` key is moved there once); Dark is the default.

**Toasts.** The shell owns `bolt`'s `ToastHost`, publishes it as `ToastGlobal` and mounts it
(deferred, above dialogs and the palette) over the region between the title bar and the status
bar, so the stack sits at the top right of the document island. `theme::install` installs
`ToastIcons` with the Lucide icons. `shell::toasts` pushes two kinds: a success that closes on
its own ("Copied" after Copy body, "Imported <n> requests into <workspace>" after a Postman
import) and an error that stays, its error text behind "Show details": Copy body with nothing
to copy, a failed import, a file the OS handler cannot open, a save that cannot write, a
theme that cannot be saved, and the app state refusing a write the user caused ("Cannot
remove <workspace> from the recent workspaces", "Cannot remember the active environment",
"Cannot remember the values the scripts set"). An OS dialog (open folder, new workspace in
folder, import file) answers `Picked`, a `Result`: its error shows as "Cannot open the folder
dialog", "Cannot open the new workspace dialog" or "Cannot open the file dialog" with the OS
error text, and a dismissed dialog shows nothing. Two failures carry fixed text because their
error would quote a file: Resend of a recorded request that does not parse ("Cannot resend
this entry") and a reload that fails after a save or an import ("Saved, but the workspace
could not be reloaded", "Imported, but …"); their error reaches only the log. Every error toast is also logged.
Failures with nothing for the user to do (background reads and writes of the app state,
history, drafts, the split layout) stay in the log only. A toast holds fixed sentences,
counts, names and the error text the engine already masks, never a value. Tests read the
automation tree (`toast-<n>`, `toast-toggle-<n>`) and a test-only record of the pushed texts,
since `bolt` keeps them private; the test clock never moves on its own, so a success toast
stays until a test advances it. To make the app state refuse writes, a test swaps the shell's
state for `AppState::open_read_only` on the same file.

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
  `gpui-pre`. Bump it for all of them at once. The source is DBFlux's published repository
  (`https://github.com/0xErwin1/dbflux`), pinned to a rev on its main.

`reqflux mcp` serves these tools itself: `ui_list_targets`, `ui_select_target`, `ui_ping`,
`ui_tree`, `ui_find`, `ui_element`, `ui_element_bounds`, `ui_wait_for_element`, `ui_click`,
`ui_type`, `ui_key`, `ui_focus` and `ui_scroll`. ReqFlux reimplements discovery, descriptor
validation and the framing instead of depending on upstream's server, whose client is private,
so no separate automation server is needed to drive the app.

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
the earlier scheme are deleted by the first run. A `build-macos` job, modeled on DBFlux's,
builds `reqflux-macos-arm64.dmg` and `reqflux-macos-amd64.dmg` on `macos-15` (amd64 cross-built
from the Apple Silicon runner, without link-time optimization to fit the runner's memory): an
`.app` from `resources/macos/Info.plist` and `packaging/icons/reqflux.icns`, signed with
`MACOS_CERTIFICATE_P12` when that secret is set (ad-hoc otherwise, so keychain grants do not
survive an update) and not notarised. The release waits for both builds, so a failed macOS build
holds the Linux tarball and the nix pin back until the next run. On macOS the in-window MCP
approvals are off: they need `$XDG_RUNTIME_DIR`, which macOS does not set. The nightly version is the workspace version
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
