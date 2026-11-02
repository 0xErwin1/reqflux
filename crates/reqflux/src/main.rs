//! The ReqFlux binary: the GUI when started without a subcommand, on the workspace given as
//! `reqflux [<DIR>]`, `reqflux run` to send one request or run a collection or folder from
//! the command line, and
//! `reqflux code` to print a request as curl, HTTPie, Python or fetch code,
//! `reqflux mcp` to serve the agent tools, `reqflux import` to bring in a Postman
//! collection, and `reqflux secret` to manage secret values in the OS keyring.

mod cli;
mod code;
mod gui;
mod import;
mod mcp;
mod run;
mod run_collection;
mod secret;
mod signals;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => gui::open(cli.workspace),
        Some(Command::Run(args)) => run::run(args),
        Some(Command::Code(args)) => code::code(args),
        Some(Command::Mcp(args)) => mcp::mcp(args),
        Some(Command::Import(args)) => import::import(args),
        Some(Command::Secret(args)) => secret::secret(args),
    }
}
