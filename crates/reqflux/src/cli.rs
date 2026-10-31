//! Command-line arguments.

use std::path::PathBuf;

#[cfg(test)]
use clap::CommandFactory as _;
use clap::{Args, Parser, Subcommand};

const AFTER_HELP: &str = "\
Without DIR, ReqFlux opens the last workspace you used that still loads; on the first run it
creates a new workspace named Personal, kept by the app under
$XDG_DATA_HOME/reqflux/workspaces. With DIR, it opens that directory (reqflux . opens the
current one). A directory named like a subcommand needs a path prefix: reqflux ./run";

/// A local-first HTTP client. Without a subcommand, opens the ReqFlux window.
#[derive(Debug, Parser)]
#[command(
    name = "reqflux",
    version,
    args_conflicts_with_subcommands = true,
    after_help = AFTER_HELP
)]
pub(crate) struct Cli {
    /// The workspace directory to open in the window; without it, the last workspace.
    #[arg(value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// What to do instead of opening the window.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// The subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Send one request of a workspace and write the response body to stdout.
    ///
    /// Uses the workspace in the current directory, or --workspace.
    Run(RunArgs),
    /// Serve the MCP tools that edit and run a workspace and drive the running app, over stdio.
    ///
    /// The engine tools start on the workspace in the current directory, or --workspace, else
    /// the most recent workspace that loads.
    Mcp(McpArgs),
    /// Import a Postman collection and its environments into a workspace.
    Import(ImportArgs),
    /// Set, unset and list secret variable values in the OS keyring.
    ///
    /// Uses the workspace in the current directory, or --workspace (a DIR argument for
    /// list).
    Secret(SecretArgs),
}

/// The application id the window reports, and the default `--app-id`.
pub(crate) const DEFAULT_APP_ID: &str = "reqflux";

const RUN_AFTER_HELP: &str = "\
Output:
  stdout  the response body, byte for byte; with --include, the status line and the
          headers first, then a blank line
  stderr  one `warning:` line per workspace load warning, one when secret variables
          could not be read from the OS keyring (they stay unresolved), and one per
          unresolved variable, then `<METHOD> <url> -> <code> <reason> in <ms> ms, <size>`

Exit codes:
  0  a response was received, whatever its status
  1  the request could not be executed: transport error, unsupported auth, environment
     not allowed for the collection, invalid GraphQL variables
  2  usage error: bad arguments, unknown request or environment, workspace that fails
     to load";

/// Arguments of `reqflux run`.
#[derive(Debug, Args)]
#[command(after_help = RUN_AFTER_HELP)]
pub(crate) struct RunArgs {
    /// The request to send: its id, or its slug path
    /// `<collection>/<folder>/.../<request>`.
    pub(crate) request: String,
    /// The workspace directory. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// The slug of the environment to use. Without it, no environment applies.
    #[arg(long, value_name = "SLUG")]
    pub(crate) env: Option<String>,
    /// Sets a local variable, which overrides globals, collection and environment
    /// variables. Repeatable. Overrides a secret value from the OS keyring.
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub(crate) vars: Vec<String>,
    /// Write the status line and the response headers before the body.
    #[arg(short, long)]
    pub(crate) include: bool,
}

const IMPORT_AFTER_HELP: &str = "\
A collection v2.1 is written as a collection directory with a folder for each Postman
folder and a file for each request; environments are written beside it.
An import never overwrites: when a collection or request file already exists the whole
import fails before anything is written.

Output:
  stdout  the workspace root, the collection, and how many folders, requests and
          environments were written

Exit codes:
  0  the documents were imported
  1  a document does not import, its JSON does not parse, or a target file exists
  2  usage error: bad arguments or an unreadable path";

/// Arguments of `reqflux import`.
#[derive(Debug, Args)]
#[command(after_help = IMPORT_AFTER_HELP)]
pub(crate) struct ImportArgs {
    /// The Postman collection v2.1 JSON file.
    #[arg(value_name = "FILE")]
    pub(crate) file: PathBuf,
    /// The workspace directory to import into. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// A Postman environment JSON file. Repeatable.
    #[arg(long = "environment", value_name = "FILE")]
    pub(crate) environments: Vec<PathBuf>,
}

const SECRET_AFTER_HELP: &str = concat!(
    "\
Secret values live in the OS keyring (Secret Service, Keychain or Credential Manager),
keyed by workspace, scope and variable ids, never in a workspace file. `set` reads the
value from stdin, never from an argument; values never appear in any output.

",
    "\
Scopes:
  global      a workspace global
  <slug>      a variable of the collection with this slug
  env:<slug>  a variable of the environment with this slug

",
    "\
Exit codes:
  0  done
  1  the OS keyring failed or is not available (set, unset)
  2  usage error: bad arguments, unknown scope, a key that is not a secret variable in
     that scope, an empty value, workspace that fails to load"
);

/// Arguments of `reqflux secret`.
#[derive(Debug, Args)]
#[command(after_help = SECRET_AFTER_HELP)]
pub(crate) struct SecretArgs {
    /// What to do.
    #[command(subcommand)]
    pub(crate) command: SecretCommand,
}

/// The `reqflux secret` subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum SecretCommand {
    /// Store the value of a secret variable, read from stdin.
    #[command(after_help = SECRET_SET_AFTER_HELP)]
    Set(SecretKeyArgs),
    /// Delete the value of a secret variable. Deleting a value that is not set succeeds.
    #[command(after_help = SECRET_UNSET_AFTER_HELP)]
    Unset(SecretKeyArgs),
    /// List every secret variable with its scope and `set`, `not set` or `unknown`.
    #[command(after_help = SECRET_LIST_AFTER_HELP)]
    List(SecretListArgs),
}

const SECRET_SET_AFTER_HELP: &str = concat!(
    "\
The value is read from stdin, never from an argument. When stdin is a terminal, set
prompts `Value for <KEY>: ` on stderr and reads without echo; otherwise it reads all of
stdin and strips one trailing newline. An empty value is a usage error. The value never
appears in any output.

",
    "\
Scopes:
  global      a workspace global
  <slug>      a variable of the collection with this slug
  env:<slug>  a variable of the environment with this slug

",
    "\
Exit codes:
  0  the value was stored
  1  the OS keyring failed or is not available
  2  usage error: bad arguments, unknown scope, a key that is not a secret variable in
     that scope, an empty value, workspace that fails to load"
);

const SECRET_UNSET_AFTER_HELP: &str = concat!(
    "\
Nothing is read from stdin; the value is never shown and never needed.

",
    "\
Scopes:
  global      a workspace global
  <slug>      a variable of the collection with this slug
  env:<slug>  a variable of the environment with this slug

",
    "\
Exit codes:
  0  the value was deleted, or was not set
  1  the OS keyring failed or is not available
  2  usage error: bad arguments, unknown scope, a key that is not a secret variable in
     that scope, workspace that fails to load"
);

const SECRET_LIST_AFTER_HELP: &str = "\
Output:
  stdout  one `<scope>\\t<key>\\t<set|not set|unknown>` line per secret variable: globals,
          then collections and environments (env:<slug>) by slug; values never appear
  stderr  a `warning:` line saying why when the OS keyring cannot be read, and the
          lines are then `unknown`

Exit codes:
  0  the list was printed, even when the keyring is not available
  2  usage error: bad arguments, workspace that fails to load";

/// The scope, key and workspace of `reqflux secret set` and `unset`.
#[derive(Debug, Args)]
pub(crate) struct SecretKeyArgs {
    /// `global`, a collection slug, or `env:<slug>` for an environment.
    #[arg(value_name = "SCOPE")]
    pub(crate) scope: String,
    /// The key of a secret variable declared in that scope.
    #[arg(value_name = "KEY")]
    pub(crate) key: String,
    /// The workspace directory. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
}

/// The workspace of `reqflux secret list`.
#[derive(Debug, Args)]
pub(crate) struct SecretListArgs {
    /// The workspace directory. Defaults to the current directory.
    #[arg(value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
}

const MCP_AFTER_HELP: &str = "\
The server speaks MCP on stdout, so stdout carries only protocol frames; logs go to stderr.

Engine tools, on the open workspace (the files, through the same engine as the window):
  workspace_open, workspace_create, workspace_info
  collection_create, folder_create, request_create
  item_rename, item_duplicate, item_move, item_delete
  request_get, request_update, request_execute
  history_list, history_get
  environment_list, environment_create, environment_rename, environment_delete
  variables_get, variable_set, variable_delete

Secret values never appear in tool output: secret variables show set or not set; basic
passwords, bearer tokens, API key values and Authorization, Proxy-Authorization and Cookie
header values show as a mask; history is masked with every secret of the workspace.
request_update takes the base_hash request_get returned, so a file changed since that read is
a conflict, never overwritten.

UI tools, driving a running ReqFlux window through its gpui-mcp bridge:
  ui_list_targets, ui_select_target, ui_ping
  ui_tree, ui_find, ui_element, ui_element_bounds, ui_wait_for_element
  ui_click, ui_type, ui_key, ui_focus, ui_scroll

Policy: every engine tool and every ui_* tool except ui_list_targets, ui_select_target and
ui_ping has a class. By default read tools run and write, execute and destructive tools \"ask\":
the running ReqFlux window lists the call in its MCP approvals view (Ctrl+Shift+M), where you
Allow once, Allow for this session or Deny with an optional reason the agent gets back; with no
window or no answer within 120 s the call is refused. ui_* reads are read, ui_* input is execute.
Enable them in ~/.config/reqflux/config.toml ($XDG_CONFIG_HOME/reqflux/config.toml):
  [mcp]
  write = \"allow\"                    # allow, ask or deny
  execute = \"allow\"
  destructive = \"deny\"
  hosts = [\"api.example.com\", \"*.example.com\", \"localhost:8080\"]   # request_execute only
  environments = [\"dev\"]             # absent: any; no environment is always allowed
[mcp.workspaces.\"<workspace id>\"] in that file and [mcp] in a workspace's reqflux.toml can
only restrict: the strictest rule wins and every hosts or environments list must allow. The
file is read before every call. hosts and environments do not apply to the ui_* tools. While
an approval is pending the window pauses its automation bridge, so ui_* tools refuse until it
is answered: an agent cannot click its own approval.

Audit: every write, execute and destructive call, allowed or refused, appends one JSON line to
~/.local/state/reqflux/audit.log ($XDG_STATE_HOME/reqflux/audit.log) with the tool, the
decision (with the approval answer), the ids it targets and the masked URL of an execute,
never argument values or typed text. A call
whose line cannot be written is refused.

Without --workspace and a reqflux.toml in the current directory, the engine tools start on the
most recent workspace the window opened that still loads.

Exit codes:
  0  the client closed the connection
  1  the server could not start or the bridge could not be served
  2  usage error: bad arguments, an invalid application id, or a --workspace that does not
     load";

/// Arguments of `reqflux mcp`.
#[derive(Debug, Args)]
#[command(after_help = MCP_AFTER_HELP)]
pub(crate) struct McpArgs {
    /// The workspace the engine tools open at startup; the current directory when it
    /// holds a reqflux.toml, else none until workspace_open.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// The application id of the windows to drive.
    #[arg(long, value_name = "ID", default_value = DEFAULT_APP_ID)]
    pub(crate) app_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        match Cli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(error) => panic!("{args:?}: {error}"),
        }
    }

    #[test]
    fn no_arguments_opens_the_window_without_a_directory() {
        let cli = parse(&["reqflux"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.workspace, None);
    }

    #[test]
    fn a_positional_directory_opens_that_workspace() {
        let cli = parse(&["reqflux", "some/dir"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.workspace, Some(PathBuf::from("some/dir")));
    }

    #[test]
    fn run_is_still_the_subcommand() {
        let cli = parse(&["reqflux", "run", "x"]);
        assert_eq!(cli.workspace, None);
        let Some(Command::Run(args)) = cli.command else {
            panic!("expected reqflux run");
        };
        assert_eq!(args.request, "x");
    }

    #[test]
    fn import_takes_a_file_a_workspace_and_environments() {
        let cli = parse(&[
            "reqflux",
            "import",
            "c.json",
            "--workspace",
            "w",
            "--environment",
            "dev.json",
            "--environment",
            "prod.json",
        ]);
        let Some(Command::Import(args)) = cli.command else {
            panic!("expected reqflux import");
        };
        assert_eq!(args.file, PathBuf::from("c.json"));
        assert_eq!(args.workspace, Some(PathBuf::from("w")));
        assert_eq!(
            args.environments,
            [PathBuf::from("dev.json"), PathBuf::from("prod.json")]
        );
    }

    #[test]
    fn import_needs_a_file_and_documents_its_exit_codes() {
        assert!(Cli::try_parse_from(["reqflux", "import"]).is_err());
        let help = Cli::command()
            .find_subcommand_mut("import")
            .map_or_else(String::new, |command| {
                command.render_long_help().to_string()
            });
        for expected in ["Exit codes", "never overwrites", "FILE"] {
            assert!(help.contains(expected), "{expected} missing: {help}");
        }
    }

    #[test]
    fn mcp_defaults_to_the_reqflux_app_id() {
        let cli = parse(&["reqflux", "mcp"]);
        let Some(Command::Mcp(args)) = cli.command else {
            panic!("expected reqflux mcp");
        };
        assert_eq!(args.app_id, DEFAULT_APP_ID);
        assert_eq!(args.workspace, None);
    }

    #[test]
    fn mcp_takes_an_app_id_and_a_workspace() {
        let cli = parse(&["reqflux", "mcp", "--app-id", "other", "--workspace", "w"]);
        let Some(Command::Mcp(args)) = cli.command else {
            panic!("expected reqflux mcp");
        };
        assert_eq!(args.app_id, "other");
        assert_eq!(args.workspace, Some(PathBuf::from("w")));
    }

    #[test]
    fn mcp_help_documents_its_tools_and_stream() {
        let help = Cli::command()
            .find_subcommand_mut("mcp")
            .map_or_else(String::new, |command| {
                command.render_long_help().to_string()
            });
        for expected in ["ui_tree", "ui_select_target", "stdout", "Exit codes"] {
            assert!(help.contains(expected), "{expected} missing: {help}");
        }
    }

    #[test]
    fn mcp_help_documents_the_policy_and_the_audit_log() {
        let help = Cli::command()
            .find_subcommand_mut("mcp")
            .map_or_else(String::new, |command| {
                command.render_long_help().to_string()
            });
        for expected in [
            "By default read tools run and write, execute and",
            "~/.config/reqflux/config.toml",
            "write = \"allow\"",
            "execute = \"allow\"",
            "can\nonly restrict",
            "$XDG_STATE_HOME/reqflux/audit.log",
            "most recent workspace",
        ] {
            assert!(help.contains(expected), "{expected} missing: {help}");
        }
        assert!(!help.contains("not implemented yet"), "{help}");
    }

    /// A directory literally named `run` is the subcommand; `./run` opens it.
    #[test]
    fn a_directory_named_run_needs_a_path_prefix() {
        assert!(Cli::try_parse_from(["reqflux", "run"]).is_err());
        let cli = parse(&["reqflux", "./run"]);
        assert_eq!(cli.workspace, Some(PathBuf::from("./run")));
    }

    #[test]
    fn help_documents_the_window_startup() {
        let help = Cli::command().render_long_help().to_string();
        for expected in [
            "Without DIR, ReqFlux opens the last workspace you used",
            "a new workspace named Personal",
            "With DIR, it opens that directory",
            "reqflux .",
        ] {
            assert!(help.contains(expected), "{expected} missing: {help}");
        }
        assert!(!help.contains("current directory when it holds"), "{help}");
    }

    #[test]
    fn directory_tools_say_they_use_the_current_directory() {
        let mut command = Cli::command();
        for name in ["run", "secret", "mcp"] {
            let Some(sub) = command.find_subcommand_mut(name) else {
                panic!("{name} is missing");
            };
            let help = sub.render_long_help().to_string();
            assert!(
                help.contains("the workspace in the current directory, or --workspace"),
                "{name}: {help}"
            );
        }
    }

    #[test]
    fn help_documents_the_workspace_directory() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("[DIR]"), "{help}");
        assert!(help.contains("./run"), "{help}");
    }
}
