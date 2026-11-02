//! Arguments of `reqflux export`, `reqflux bundle` and `reqflux docs` (v1-features S6).

use std::path::PathBuf;

use clap::{Args, Subcommand};

pub(crate) const EXPORT_AFTER_HELP: &str = concat!(
    "\
Targets:
  <collection>                       a collection, by slug or id
  <collection>/<folder>/...          a folder, exported as a collection holding the path
                                     to it, so the auth and scripts it inherits come along
  <collection>/.../<request>         a request, the same way (a request wins over a folder
                                     at the same path)
  env:<slug>                         an environment, as a Postman environment document

",
    "\
Secret variables have no value in workspace files. They are exported with an empty value
unless --include-secrets reads their values from the OS keyring and writes them in clear
text: keep such a file private.

Output:
  stdout  the JSON document, unless --output names a file
  stderr  one `warning:` line per workspace load warning and per secret variable
          exported without its value

",
    "\
Exit codes:
  0  the document was written
  1  the OS keyring could not be read (--include-secrets), a saved example could not be
     read, the output file exists without --force, or it could not be written
  2  usage error: bad arguments, an unknown target, workspace that fails to load"
);

/// Arguments of `reqflux export`.
#[derive(Debug, Args)]
#[command(after_help = EXPORT_AFTER_HELP)]
pub(crate) struct ExportArgs {
    /// What to export: the id or slug path of a collection, folder or request, or
    /// `env:<slug>` for an environment.
    #[arg(value_name = "TARGET")]
    pub(crate) target: String,
    /// The workspace directory. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// Write the document to this file instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
    /// Replace the output file when it exists.
    #[arg(long, requires = "output")]
    pub(crate) force: bool,
    /// Read secret variable values from the OS keyring and write them in clear text.
    #[arg(long)]
    pub(crate) include_secrets: bool,
}

/// Arguments of `reqflux bundle`.
#[derive(Debug, Args)]
pub(crate) struct BundleArgs {
    /// What to do.
    #[command(subcommand)]
    pub(crate) command: BundleCommand,
}

/// The `reqflux bundle` subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum BundleCommand {
    /// Write a workspace, collection, folder, request or environment as one bundle file.
    #[command(after_help = BUNDLE_CREATE_AFTER_HELP)]
    Create(BundleCreateArgs),
    /// Import a bundle file into a workspace, never replacing a file.
    #[command(after_help = BUNDLE_IMPORT_AFTER_HELP)]
    Import(BundleImportArgs),
}

pub(crate) const BUNDLE_CREATE_AFTER_HELP: &str = concat!(
    "\
Without TARGET the whole workspace is bundled: its manifest, environments and collections.
A bundle is one JSON file (format `reqflux-bundle`, version 1) holding the workspace files
as ReqFlux writes them, scripts and saved examples included.

Secret variables have no value in workspace files. They are left out of the bundle unless
--encrypt-secrets reads their values from the OS keyring and encrypts them with a
passphrase (age, scrypt). They are never written in clear.

",
    "\
The passphrase is read from stdin, never from an argument: from a terminal it is prompted
on stderr without echo (twice when creating), otherwise all of stdin is read and one
trailing newline stripped. An empty passphrase is a usage error.

",
    "\
Output:
  stdout  the bundle, unless --output names a file
  stderr  one `warning:` line per workspace load warning and per secret variable left
          out, as `<scope> <key>` (scope: global, a collection slug or env:<slug>)

",
    "\
Exit codes:
  0  the bundle was written
  1  the OS keyring could not be read (--encrypt-secrets), a file could not be read, the
     output file exists without --force, or it could not be written
  2  usage error: bad arguments, an unknown target, an empty or mismatched passphrase,
     workspace that fails to load"
);

/// Arguments of `reqflux bundle create`.
#[derive(Debug, Args)]
pub(crate) struct BundleCreateArgs {
    /// What to bundle: the id or slug path of a collection, folder or request, or
    /// `env:<slug>` for an environment. Without it, the whole workspace.
    #[arg(value_name = "TARGET")]
    pub(crate) target: Option<String>,
    /// The workspace directory. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// Write the bundle to this file instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,
    /// Replace the output file when it exists.
    #[arg(long, requires = "output")]
    pub(crate) force: bool,
    /// Read secret variable values from the OS keyring and encrypt them with a passphrase
    /// read from stdin.
    #[arg(long)]
    pub(crate) encrypt_secrets: bool,
}

pub(crate) const BUNDLE_IMPORT_AFTER_HELP: &str = concat!(
    "\
A workspace bundle needs a directory without a reqflux.toml and becomes a new workspace
with a new id. A collection or environment bundle goes into the workspace, which is created
when the directory has none. A folder or request bundle goes at the end of the collection
or folder --into names. An id the workspace already uses gets a new one.
An import never overwrites: when a target file exists the whole import fails before
anything is written.

",
    "\
Encrypted secrets are stored in the OS keyring only with --decrypt-secrets; without it
they are skipped, and a warning says so.

",
    "\
The passphrase is read from stdin, never from an argument: from a terminal it is prompted
on stderr without echo, otherwise all of stdin is read and one trailing newline stripped.
An empty passphrase is a usage error.

",
    "\
Output:
  stdout  the workspace root, then `<kind> bundle: <n> collections, <n> folders,
          <n> requests, <n> examples, <n> environments`, then how many ids were renewed
          and secrets stored, when any
  stderr  a `warning:` line when encrypted secrets were skipped

",
    "\
Exit codes:
  0  the bundle was imported
  1  the file is not a bundle this ReqFlux reads, the passphrase does not decrypt it, a
     target file exists, a write failed, or the secrets could not be stored in the OS
     keyring (the files stay)
  2  usage error: bad arguments, an unreadable file, a missing, unknown or unexpected
     --into, an empty passphrase, workspace that fails to load"
);

/// Arguments of `reqflux bundle import`.
#[derive(Debug, Args)]
pub(crate) struct BundleImportArgs {
    /// The bundle file.
    #[arg(value_name = "FILE")]
    pub(crate) file: PathBuf,
    /// The workspace directory to import into. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// The id or slug path of the collection or folder a folder or request bundle goes into.
    #[arg(long, value_name = "TARGET")]
    pub(crate) into: Option<String>,
    /// Decrypt the bundle's secrets with a passphrase read from stdin and store them in the
    /// OS keyring.
    #[arg(long)]
    pub(crate) decrypt_secrets: bool,
}

pub(crate) const DOCS_AFTER_HELP: &str = "\
Writes README.md and a self-contained index.html: requests grouped by folder with their
method, URL, description, auth type, params, headers, body and saved examples. Everything
is shown as written: {{variables}} stay as they are, secret values are never read, and a
literal Authorization, Proxy-Authorization or Cookie value is masked.

Output:
  stdout  the path of each file written
  stderr  one `warning:` line per workspace load warning

Exit codes:
  0  the docs were written
  1  a file exists without --force, a saved example could not be read, or a file could
     not be written
  2  usage error: bad arguments, an unknown collection, workspace that fails to load";

/// Arguments of `reqflux docs`.
#[derive(Debug, Args)]
#[command(after_help = DOCS_AFTER_HELP)]
pub(crate) struct DocsArgs {
    /// The id or slug of the collection.
    #[arg(value_name = "COLLECTION")]
    pub(crate) collection: String,
    /// The workspace directory. Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace: Option<PathBuf>,
    /// The directory to write into; created when missing.
    #[arg(short, long, value_name = "DIR")]
    pub(crate) output: PathBuf,
    /// Replace README.md and index.html when they exist.
    #[arg(long)]
    pub(crate) force: bool,
}
