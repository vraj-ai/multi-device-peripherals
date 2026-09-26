//! `mdp`: one binary for both Peers (Source and Sink are roles, not builds).

mod config;
mod discovery;
mod platform;
mod ui;

use mdp_core::{Desktop, FakePlatform, InputEvent, Platform};
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help_text() -> &'static str {
    "mdp - share one mouse and keyboard between two machines\n\
     \n\
     Usage: mdp <COMMAND>\n\
     \n\
     Commands:\n\
     \x20 run       Run the peer (capture, share, and inject input)\n\
     \x20 ui        Open the arrangement and pairing window\n\
     \x20 pair      Pair with the other peer (confirm the 6-digit code)\n\
     \x20 selftest  Run the local capture loopback self-test\n\
     \n\
     Options:\n\
     \x20 -h, --help     Print help\n\
     \x20 -V, --version  Print version\n"
}

fn version_text() -> String {
    format!("mdp {VERSION}\n")
}

fn run_stub(command: &str) -> ExitCode {
    println!("{command}: not yet implemented (T1 stub)");
    ExitCode::SUCCESS
}

fn selftest() -> ExitCode {
    let mut platform = FakePlatform::new(Desktop::new(0.0, 0.0, 1920.0, 1080.0));
    let capture = match platform.start_capture() {
        Ok(capture) => capture,
        Err(err) => {
            eprintln!("selftest: capture failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    let probe = InputEvent::MouseMove { x: 1.0, y: 1.0 };
    if let Err(err) = platform.feed_physical_event(probe.clone()) {
        eprintln!("selftest: feed failed: {err}");
        return ExitCode::FAILURE;
    }
    match capture.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(seen) if seen == probe => {
            println!("selftest: capture loopback ok");
            ExitCode::SUCCESS
        }
        Ok(seen) => {
            eprintln!("selftest: unexpected event: {seen:?}");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("selftest: no event observed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("-h") | Some("--help") => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            print!("{}", version_text());
            ExitCode::SUCCESS
        }
        Some("ui") => match ui::run_demo() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("ui: {err}");
                ExitCode::FAILURE
            }
        },
        Some(command @ ("run" | "pair")) => run_stub(command),
        Some("selftest") => selftest(),
        Some(flag) if flag.starts_with('-') => {
            eprintln!("error: unexpected flag '{flag}'\n");
            eprint!("{}", help_text());
            ExitCode::from(2)
        }
        Some(subcommand) => {
            eprintln!("error: unrecognized subcommand '{subcommand}'\n");
            eprint!("{}", help_text());
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_all_subcommands() {
        let help = help_text();
        for subcommand in ["run", "ui", "pair", "selftest"] {
            assert!(help.contains(subcommand), "help should list `{subcommand}`");
        }
    }
}
