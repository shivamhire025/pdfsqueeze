mod agent;
mod batch;
mod cli;
mod commands;
mod compress;
mod convert;
mod engine;
mod fix;
mod imgpdf;
mod ocr;
mod pages;
mod pdfout;
mod security;
mod stamp;
mod util;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{Cli, Cmd};
use commands::{command_name, envelope, execute, Ctx, EXIT_USAGE};
use serde_json::json;

fn main() {
    let wants_json = std::env::args().any(|a| a == "--json");

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let informational = matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            );
            if wants_json && !informational {
                let v = json!({
                    "ok": false,
                    "command": null,
                    "error": { "code": "usage", "message": e.to_string().trim() },
                    "exit_code": EXIT_USAGE,
                });
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                std::process::exit(EXIT_USAGE);
            }
            e.exit();
        }
    };

    if let Cmd::Mcp {
        allow_dir,
        allow_any_path,
    } = &cli.cmd
    {
        if allow_dir.is_empty() && !allow_any_path {
            eprintln!(
                "error: `mcp` needs to know which folders tools may use. Pass --allow-dir <folder> \
                 (repeatable), or --allow-any-path if the agent is fully trusted."
            );
            std::process::exit(EXIT_USAGE);
        }
        let result = agent::PathPolicy::new(allow_dir, *allow_any_path).and_then(agent::serve);
        if let Err(e) = result {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let ctx = Ctx::from_flag(cli.json);
    let name = command_name(&cli.cmd);
    let result = execute(cli.cmd, &ctx);

    if ctx.json {
        let (v, code) = envelope(name, result);
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        std::process::exit(code);
    }

    match result {
        Ok(done) => {
            if !done.human.is_empty() {
                println!("{}", done.human);
            }
            for w in &done.warnings {
                eprintln!("warning: {w}");
            }
            std::process::exit(done.exit_code);
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}
