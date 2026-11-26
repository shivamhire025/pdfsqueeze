mod agent;
mod batch;
mod cli;
mod commands;
mod compress;
mod convert;
mod engine;
mod fix;
mod imgpdf;
mod licence;
mod ocr;
mod pages;
mod pdfout;
mod progress;
mod security;
mod stamp;
mod ui;
mod util;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{Cli, Cmd};
use commands::{command_name, envelope, execute, Ctx, EXIT_USAGE};
use serde_json::json;

/// Should a bare launch open the app window? Yes when it was started from the desktop: no
/// arguments, and either
/// - neither input nor output is a terminal (macOS and Linux double-click), or
/// - the system opened a console just for this process (Windows double-click or Start menu, where
///   input and output *are* terminals, so the first rule never matches).
///
/// Never from a shared terminal or a script, so `pdfsqueeze` there still prints help.
/// `PDFSQUEEZE_NO_GUI=1` forces help.
fn opens_window_on_launch(
    arg_count: usize,
    stdin_tty: bool,
    stdout_tty: bool,
    own_console: bool,
    no_gui: bool,
    can_window: bool,
) -> bool {
    let no_terminal = !stdin_tty && !stdout_tty;
    let fresh_console = own_console && stdin_tty && stdout_tty;
    arg_count <= 1 && (no_terminal || fresh_console) && !no_gui && can_window
}

/// Is there a console that exists only for this process? That is what Windows creates when a
/// console program is double-clicked. A console shared with a shell has two or more processes.
#[cfg(windows)]
fn has_own_console() -> bool {
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut pids = [0u32; 4];
    // SAFETY: plain Win32 call with a valid buffer and its length.
    unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) == 1 }
}
#[cfg(not(windows))]
fn has_own_console() -> bool {
    false
}

/// Windows builds are console programs (so scripts get output). When the console is only ours,
/// close it so only the app window shows. A shared console is left alone.
#[cfg(windows)]
fn release_own_console() {
    extern "system" {
        fn FreeConsole() -> i32;
    }
    if has_own_console() {
        // SAFETY: plain Win32 call without arguments.
        unsafe {
            FreeConsole();
        }
    }
}
#[cfg(not(windows))]
fn release_own_console() {}

fn main() {
    use std::io::IsTerminal;
    if opens_window_on_launch(
        std::env::args().count(),
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        has_own_console(),
        std::env::var_os("PDFSQUEEZE_NO_GUI").is_some(),
        ui::window_available(),
    ) {
        release_own_console();
        if let Err(e) = ui::serve(None, ui::Mode::Window) {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

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

    if let Cmd::Ui { port, no_open, browser } = &cli.cmd {
        release_own_console();
        let mode = if *no_open {
            ui::Mode::ServerOnly
        } else if *browser || !ui::window_available() {
            ui::Mode::Browser
        } else {
            ui::Mode::Window
        };
        if let Err(e) = ui::serve(*port, mode) {
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

#[cfg(test)]
mod launch_tests {
    use super::opens_window_on_launch as f;

    #[test]
    fn only_a_bare_desktop_launch_opens_the_window() {
        // args, stdin tty, stdout tty, own console, no_gui, can_window
        assert!(f(1, false, false, false, false, true)); // macOS double-click
        assert!(f(1, true, true, true, false, true)); // Windows double-click or Start menu
        assert!(!f(1, true, true, false, false, true)); // typed in a terminal
        assert!(!f(1, false, true, false, false, true)); // output is a terminal
        assert!(!f(1, false, true, true, false, true)); // own console but input is piped in
        assert!(!f(1, true, false, true, false, true)); // own console but output is redirected
        assert!(!f(2, true, true, true, false, true)); // has arguments
        assert!(!f(1, true, true, true, true, true)); // PDFSQUEEZE_NO_GUI
        assert!(!f(1, false, false, false, false, false)); // no window on this platform (Linux)
    }
}
