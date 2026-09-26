//! `mdp`: one binary for both Peers (Source and Sink are roles, not builds).

mod config;
mod discovery;
mod platform;
mod selftest;
mod ui;

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

fn finish_selftest(result: Result<selftest::SelftestReport, String>) -> ExitCode {
    match result {
        Ok(report) => {
            let bounds = report.desktop;
            println!(
                "selftest: Desktop x={} y={} w={} h={} ok ({:?})",
                bounds.x, bounds.y, bounds.width, bounds.height, report.elapsed
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("selftest: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn selftest_platform() -> ExitCode {
    let mut platform = platform::Native::new();
    finish_selftest(selftest::run_selftest(&mut platform, selftest_home()))
}

#[cfg(target_os = "windows")]
fn selftest_home() -> (f64, f64) {
    platform::windows::WindowsPlatform::cursor_position()
}

#[cfg(not(target_os = "windows"))]
fn selftest_home() -> (f64, f64) {
    // Only Windows exposes a cursor getter so far; the neutral core still
    // runs everywhere, and other OSes get a true home once they add one.
    (0.0, 0.0)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn selftest_platform() -> ExitCode {
    run_stub("selftest")
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
        Some("selftest") => selftest_platform(),
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
