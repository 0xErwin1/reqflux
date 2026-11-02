//! `reqflux code`: print the code that sends a request (v1-features S2).

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use reqflux_engine::Engine;

use crate::cli::CodeArgs;

/// Runs `reqflux code` and returns its exit code.
pub(crate) fn code(args: CodeArgs) -> ExitCode {
    let root = args.workspace.unwrap_or_else(|| PathBuf::from("."));
    let engine = match Engine::open(root) {
        Ok(engine) => engine,
        Err(error) => return usage(&error.to_string()),
    };
    for warning in engine.warnings() {
        eprintln!("warning: {warning}");
    }
    let mut code = match engine.code(&args.request, args.lang) {
        Ok(code) => code,
        Err(error) => return usage(&error.to_string()),
    };
    // Shell commands end without a newline and scripts with one; print exactly one.
    if !code.ends_with('\n') {
        code.push('\n');
    }
    let mut stdout = io::stdout().lock();
    if let Err(error) = stdout
        .write_all(code.as_bytes())
        .and_then(|()| stdout.flush())
    {
        eprintln!("error: could not write the code: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
}
