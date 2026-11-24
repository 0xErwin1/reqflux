//! The ReqFlux binary: the GUI when started without a subcommand, on the workspace given as
//! `reqflux [<DIR>]`, `reqflux run` to send one request or run a collection or folder from
//! the command line, and
//! `reqflux code` to print a request as curl, HTTPie, Python or fetch code,
//! `reqflux mcp` to serve the agent tools, `reqflux import` to bring in a Postman
//! collection, `reqflux secret` to manage secret values in the OS keyring, and
//! `reqflux export`, `reqflux bundle` and `reqflux docs` to take a workspace out as
//! Postman JSON, a native bundle or API docs.

#[cfg(all(test, target_os = "linux"))]
mod child_environment_parity;
mod cli;
mod code;
mod gui;
mod import;
mod mcp;
mod run;
mod run_collection;
mod secret;
mod signals;
mod transfer;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => {
            // Before any thread exists: it may re-execute the process with the boot GPU's
            // Vulkan driver selected. Only the window needs a GPU, so the CLI skips it.
            #[cfg(target_os = "linux")]
            gpui_linux::select_vulkan_driver();
            gui::open(cli.workspace)
        }
        Some(Command::Run(args)) => run::run(args),
        Some(Command::Code(args)) => code::code(args),
        Some(Command::Mcp(args)) => mcp::mcp(args),
        Some(Command::Import(args)) => import::import(args),
        Some(Command::Secret(args)) => secret::secret(args),
        Some(Command::Export(args)) => transfer::export(args),
        Some(Command::Bundle(args)) => transfer::bundle(args),
        Some(Command::Docs(args)) => transfer::docs(args),
    }
}
