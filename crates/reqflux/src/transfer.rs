//! `reqflux export`, `reqflux bundle` and `reqflux docs` (v1-features S6): the engine's
//! Postman export, native bundle and API docs, with their files and passphrases.
//!
//! Secret values come from and go to the OS keyring. A passphrase is read from stdin, never
//! from an argument, and no message carries a value or the passphrase.

use std::fs;
use std::io::{self, IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use reqflux_engine::{
    BundleError, BundleImport, BundleSecrets, BundleSummary, DocsError, Engine, ExportError,
    ExportFileError, KeyringSecretStore, import_bundle, write_export_file,
};

use crate::cli::{
    BundleArgs, BundleCommand, BundleCreateArgs, BundleImportArgs, DocsArgs, ExportArgs,
};

/// Why a command stopped, with its exit code.
#[derive(Debug)]
enum Failure {
    /// Exit code 2: the arguments, the target, the passphrase or the workspace.
    Usage(String),
    /// Exit code 1: the keyring, a file, or a bundle that does not import.
    Failed(String),
}

fn finish(result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
}

/// Runs `reqflux export` and returns its exit code.
pub(crate) fn export(args: ExportArgs) -> ExitCode {
    init_logger();
    finish(run_export(&args))
}

fn run_export(args: &ExportArgs) -> Result<(), Failure> {
    let engine = open(args.workspace.as_deref())?;
    let export = engine
        .export_postman(&args.target, args.include_secrets)
        .map_err(|error| match error {
            ExportError::UnknownTarget(_) => Failure::Usage(error.to_string()),
            ExportError::Secret(_) | ExportError::Store(_) => Failure::Failed(error.to_string()),
        })?;
    for key in &export.empty_secrets {
        if args.include_secrets {
            eprintln!(
                "warning: secret variable {key} has no value in the OS keyring; it is exported empty"
            );
        } else {
            eprintln!(
                "warning: secret variable {key} is exported without its value; pass \
                 --include-secrets to read it from the OS keyring"
            );
        }
    }
    let text = export
        .text()
        .map_err(|error| Failure::Failed(format!("could not write the document: {error}")))?;
    // A file with secret values in clear is readable by its owner only.
    output(
        args.output.as_deref(),
        args.force,
        args.include_secrets,
        &text,
    )
}

/// Runs `reqflux bundle` and returns its exit code.
pub(crate) fn bundle(args: BundleArgs) -> ExitCode {
    init_logger();
    finish(match args.command {
        BundleCommand::Create(args) => create_bundle(&args),
        BundleCommand::Import(args) => import(&args),
    })
}

fn create_bundle(args: &BundleCreateArgs) -> Result<(), Failure> {
    let engine = open(args.workspace.as_deref())?;
    let passphrase = if args.encrypt_secrets {
        Some(passphrase(true)?)
    } else {
        None
    };
    let secrets = match &passphrase {
        Some(passphrase) => BundleSecrets::Encrypt { passphrase },
        None => BundleSecrets::Omit,
    };
    let bundle = engine
        .bundle(args.target.as_deref(), secrets)
        .map_err(bundle_failure)?;
    for (scope, key) in &bundle.secrets_omitted {
        if args.encrypt_secrets {
            eprintln!(
                "warning: secret variable {scope} {key} has no value in the OS keyring; it is not in the bundle"
            );
        } else {
            eprintln!(
                "warning: secret variable {scope} {key} is not in the bundle; pass \
                 --encrypt-secrets to include it"
            );
        }
    }
    output(args.output.as_deref(), args.force, false, &bundle.text)
}

fn import(args: &BundleImportArgs) -> Result<(), Failure> {
    let text = fs::read_to_string(&args.file)
        .map_err(|error| Failure::Usage(format!("cannot read {}: {error}", args.file.display())))?;
    let root = match &args.workspace {
        Some(root) => root.clone(),
        None => std::env::current_dir().map_err(|error| {
            Failure::Usage(format!("cannot read the current directory: {error}"))
        })?,
    };
    let passphrase = if args.decrypt_secrets {
        Some(passphrase(false)?)
    } else {
        None
    };
    let options = BundleImport {
        into: args.into.as_deref(),
        passphrase: passphrase.as_deref(),
    };
    let summary =
        import_bundle(&root, &text, &options, &KeyringSecretStore).map_err(bundle_failure)?;
    print_summary(&summary);
    if summary.secrets_skipped {
        eprintln!(
            "warning: the bundle's encrypted secrets were skipped; pass --decrypt-secrets to store them"
        );
    }
    match summary.secret_problem {
        Some(problem) => Err(Failure::Failed(format!(
            "the files were imported, but the secrets could not be stored in the OS keyring: {problem}"
        ))),
        None => Ok(()),
    }
}

fn print_summary(summary: &BundleSummary) {
    println!("workspace {}", summary.root.display());
    println!(
        "{} bundle: {} collections, {} folders, {} requests, {} examples, {} environments",
        summary.kind.name(),
        summary.collections,
        summary.folders,
        summary.requests,
        summary.examples,
        summary.environments
    );
    if summary.renewed_ids > 0 {
        println!("{} ids already in use were renewed", summary.renewed_ids);
    }
    if summary.secrets_imported > 0 {
        println!(
            "{} secrets stored in the OS keyring",
            summary.secrets_imported
        );
    }
}

fn bundle_failure(error: BundleError) -> Failure {
    match error {
        BundleError::UnknownTarget(_) | BundleError::Parent(_) | BundleError::EmptyPassphrase => {
            Failure::Usage(error.to_string())
        }
        BundleError::Reload(_)
        | BundleError::Secret(_)
        | BundleError::Store(_)
        | BundleError::Encrypt(_)
        | BundleError::Decrypt
        | BundleError::Invalid(_)
        | BundleError::UnsupportedFormat { .. }
        | BundleError::Exists { .. } => Failure::Failed(error.to_string()),
    }
}

/// Runs `reqflux docs` and returns its exit code.
pub(crate) fn docs(args: DocsArgs) -> ExitCode {
    init_logger();
    finish(run_docs(&args))
}

fn run_docs(args: &DocsArgs) -> Result<(), Failure> {
    let engine = open(args.workspace.as_deref())?;
    let docs = engine.docs(&args.collection).map_err(docs_failure)?;
    for path in docs.write(&args.output, args.force).map_err(docs_failure)? {
        println!("{}", path.display());
    }
    Ok(())
}

fn docs_failure(error: DocsError) -> Failure {
    match error {
        DocsError::UnknownCollection(_) => Failure::Usage(error.to_string()),
        DocsError::Exists { path } => Failure::Failed(exists(&path)),
        DocsError::Store(_) | DocsError::Write { .. } => Failure::Failed(error.to_string()),
    }
}

fn exists(path: &Path) -> String {
    format!(
        "{} already exists; pass --force to replace it",
        path.display()
    )
}

/// Loads the workspace at `root`, the current directory by default, with the OS keyring,
/// and prints its load warnings.
fn open(root: Option<&Path>) -> Result<Engine, Failure> {
    let root = root.map_or_else(|| PathBuf::from("."), Path::to_owned);
    let engine = Engine::open_with_secrets(root, Arc::new(KeyringSecretStore))
        .map_err(|error| Failure::Usage(error.to_string()))?;
    for warning in engine.warnings() {
        eprintln!("warning: {warning}");
    }
    Ok(engine)
}

/// Writes `text` to stdout, or to `path` by the engine's rule: a new file unless `force`,
/// owner-only when `private`.
fn output(path: Option<&Path>, force: bool, private: bool, text: &str) -> Result<(), Failure> {
    let Some(path) = path else {
        let mut stdout = io::stdout().lock();
        return stdout
            .write_all(text.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|error| Failure::Failed(format!("could not write to stdout: {error}")));
    };
    write_export_file(path, text, force, private).map_err(|error| match error {
        ExportFileError::Exists { path } => Failure::Failed(exists(&path)),
        ExportFileError::Write { .. } => Failure::Failed(error.to_string()),
    })
}

/// The passphrase: prompted without echo from a terminal (twice when `confirm`), else all of
/// stdin without one trailing newline. Never empty.
fn passphrase(confirm: bool) -> Result<String, Failure> {
    let unreadable =
        |error: io::Error| Failure::Usage(format!("cannot read the passphrase: {error}"));
    let value = if io::stdin().is_terminal() {
        eprint!("Passphrase: ");
        let first = rpassword::read_password().map_err(unreadable)?;
        if confirm && !first.is_empty() {
            eprint!("Repeat passphrase: ");
            let second = rpassword::read_password().map_err(unreadable)?;
            if second != first {
                return Err(Failure::Usage("the passphrases do not match".to_owned()));
            }
        }
        first
    } else {
        crate::secret::read_value(io::stdin().lock()).map_err(|error| match error {
            crate::secret::Failure::Usage(message) | crate::secret::Failure::Store(message) => {
                Failure::Usage(message.replace("the value", "the passphrase"))
            }
        })?
    };
    if value.is_empty() {
        return Err(Failure::Usage("the passphrase is empty".to_owned()));
    }
    Ok(value)
}
