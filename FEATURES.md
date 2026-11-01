# ReqFlux features

What ReqFlux will do, by release level. ARCHITECTURE.md explains how each piece is built.

| Level | Meaning |
|-------|---------|
| MVP | Enough to replace Postman for daily HTTP work. |
| v1 | Parity with the Postman features real collections depend on. |
| Later | Valuable, but nothing blocks on it. |

Scope: HTTP only. WebSocket, gRPC, SSE and GraphQL subscriptions are out of scope until after
v1.

## Workspaces, collections and environments

| Feature | Level |
|---------|-------|
| Create, open, close and switch between recent workspaces | MVP |
| Collection and folder tree: create, rename, move, duplicate, delete | MVP |
| Reorder by drag and by keyboard | MVP |
| Workspace environments with an active environment selector | MVP |
| Global, collection and environment variables, with shared and current values | MVP |
| Secret variables stored in the OS keyring and masked in the UI | MVP |
| Conflict detection for externally changed files (git pull, another editor, an agent) at save, and reload at reopen | MVP |
| Live workspace watcher that notices external changes while the window is open, without a save or reopen | Later |
| Global search of requests by name, URL and method (command palette) | MVP |
| Per-collection environment allowlist | v1 |

## Editing and saving

| Feature | Level |
|---------|-------|
| Edits are local drafts: kept across restarts, marked unsaved in their tab | MVP |
| Explicit save (`Ctrl+S`) writes the workspace file and discards the draft | MVP |
| Sending a request with a draft sends the draft, as Postman does | MVP |
| Discard a draft and revert to the saved file | MVP |

## Requests

| Feature | Level |
|---------|-------|
| Any method, including custom verbs | MVP |
| URL with `{{var}}` highlighting, resolved-value preview and unresolved-variable warnings | MVP |
| Params and headers as tables with a per-row enable toggle | MVP |
| Body: `none`, `raw` (JSON, XML, text) with an editor, `urlencoded`, `formdata` with files | MVP |
| Auth: `inherit`, `none`, `basic`, `bearer`, `apikey` | MVP |
| Per-request settings: timeout, redirects, TLS verification | MVP |
| Dynamic variables (`$guid`, `$timestamp`, `$random*`) | MVP |
| Body: `binary`, `graphql` | v1 |
| Auth: `digest`, `oauth2` (with token acquisition flows), `awsv4` | v1 |
| Saved examples (request and response pairs) | v1 |
| Generate code (curl and other languages) | v1 |
| Paste a curl command into the URL bar to import it | v1 |
| Auth: `oauth1`, `hawk`, `ntlm`, `jwt`, `edgegrid`, `asap` | Later |

## Responses

| Feature | Level |
|---------|-------|
| Body as pretty, raw and preview (HTML and images) | MVP |
| Status, total time and size | MVP |
| Headers and cookies | MVP |
| Large responses (several MB) without blocking the UI | MVP |
| Timing breakdown (DNS, connect, TLS, first byte, download) | v1 |
| Search in the body and JSONPath filtering | v1 |
| Save a response to a file or as an example | v1 |

## Network

| Feature | Level |
|---------|-------|
| HTTP/1.1 and HTTP/2, response decompression | MVP |
| Cancel an in-flight request | MVP |
| Custom CA bundle and per-host client certificates | v1 |
| Proxies: system, manual, per workspace | v1 |
| Per-workspace cookie jar, editable | v1 |

## Scripts and tests

| Feature | Level |
|---------|-------|
| Pre-request and test scripts on collections, folders and requests, with `pm.*` variables, request and response | v1 |
| `pm.test` and `pm.expect` (chai) with a results panel | v1 |
| Script console | v1 |
| `pm.sendRequest` and `require` of the Postman sandbox libraries | v1 |
| Legacy globals (`tests[]`, `postman.*`) | Later |

## Runner

| Feature | Level |
|---------|-------|
| Run a collection or folder in order, with `setNextRequest` | v1 |
| `reqflux run` on the command line, with an exit code for CI (single requests work today; running a collection or folder is still v1) | v1 |
| Iterations over CSV or JSON data | Later |

## History

| Feature | Level |
|---------|-------|
| Execution history with the resolved request and the response | MVP |
| Reopen and resend from history | MVP |

## Import and export

| Feature | Level |
|---------|-------|
| Import Postman collection v2.1 and environments | MVP |
| Export to Postman v2.1: collection, folder, request, environment | v1 |
| Native bundle of a workspace, collection, request or environment, with age-encrypted secrets | v1 |
| Local API documentation generated from a collection | v1 |
| Import OpenAPI, Insomnia and HAR | Later |

## MCP for agents

One MCP server, `reqflux`, for everything.

| Feature | Level |
|---------|-------|
| `ui_*` tools: element tree, click, keyboard, focus | MVP |
| Create, read, update and delete workspaces, collections, requests and environments | MVP |
| Execute a request and read history | MVP |
| Policy by execution class (read, write, execute, destructive), host and environment allowlists | v1 |
| In-app approvals and audit log | v1 |
| App settings: theme, interface font size and MCP policy editing | v1 |
| Run the collection runner | v1 |
| Resources addressed by id | Later |

## User experience

| Feature | Level |
|---------|-------|
| Full keyboard control: command palette, context-aware keymap, keyboard coverage test | MVP |
| Document tabs with unsaved state | MVP |
| Light and dark themes from semantic tokens | MVP |
| Islands layout: sidebar, editor and response as separate rounded panels on a canvas | MVP |
| Layout mode Classic: tree, request above, response below (default) | MVP |
| Layout mode Side by side: activity rail, request and response in columns | v1 |
| Layout mode Document: no sidebar, request as an `.http`-style document, navigation by palette | v1 |
| Configurable keymap | v1 |
| Split panes and multiple windows | Later |

## After v1

Directions, not commitments:

- **Peer-to-peer sync.** A coordination server where peers only discover each other by sharing
  an id, then sync workspaces directly, in the spirit of Tailscale. This is the path to
  sharing between people beyond git.
- **Published documentation.** Collection documentation shared over that sync network.
- **Mock servers.** Always local, served from saved examples.
- **More protocols.** WebSocket, gRPC, SSE and GraphQL subscriptions.
