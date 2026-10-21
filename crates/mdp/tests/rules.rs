//! Machine checks for rules that used to live only in prose.
//!
//! `verify-mdp` runs this file. Each failure names the file or type to use
//! instead. `bad_patterns_name_the_fix` rebuilds the old mistakes in a temp
//! tree and requires every check to reject them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name == "target" {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    walk(&root.join("crates"), &mut out);
    out
}

fn toml_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name == "target" {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                out.push(path);
            }
        }
    }
    walk(root, &mut out);
    out
}

fn git_mode(root: &Path, path: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-s", "--", path])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mode = text.split_whitespace().next()?.to_string();
    if mode.len() == 6 && mode.chars().all(|c| c.is_ascii_digit()) {
        Some(mode)
    } else {
        None
    }
}

fn check_progress(root: &Path) -> Vec<String> {
    if root.join("CONTEXT/progress.md").exists() {
        vec!["FAIL progress-md: CONTEXT/progress.md is back. Status lives in git history, issues, and pull requests. Do not recreate that file.".into()]
    } else {
        Vec::new()
    }
}

fn check_latency(root: &Path) -> Vec<String> {
    let arch = read_text(&root.join("CONTEXT/architecture.md"));
    let e2e = read_text(&root.join("crates/mdp/tests/e2e.rs"));
    let mut bullet = String::new();
    let mut capture = false;
    for line in arch.lines() {
        if line.contains("Crossing latency") {
            capture = true;
        } else if capture && line.starts_with("- **") {
            break;
        }
        if capture {
            bullet.push_str(line);
            bullet.push('\n');
        }
    }
    let mut names = Vec::new();
    let mut rest = bullet.as_str();
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else { break };
        let name = &rest[..end];
        rest = &rest[end + 1..];
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && name.contains('_')
        {
            names.push(name.to_string());
        }
    }
    if names.is_empty() {
        return vec!["FAIL latency-name: the Crossing latency bullet names no test. Cite `mouse_move_loopback_latency_p95_under_10ms` in crates/mdp/tests/e2e.rs.".into()];
    }
    names
        .into_iter()
        .filter(|name| !e2e.contains(&format!("fn {name}")))
        .map(|name| {
            format!(
                "FAIL latency-name: `{name}` is not a function in crates/mdp/tests/e2e.rs. Cite `mouse_move_loopback_latency_p95_under_10ms`."
            )
        })
        .collect()
}

fn check_skill(root: &Path) -> Vec<String> {
    let expect = Path::new("../../.agents/skills/verify-mdp");
    let mut errs = Vec::new();
    for link in [".cursor/skills/verify-mdp", ".claude/skills/verify-mdp"] {
        let path = root.join(link);
        let mode = git_mode(root, link);
        if let Some(mode) = &mode {
            if mode != "120000" {
                errs.push(format!(
                    "FAIL skill-home: {link} is git mode {mode}, not a symlink (120000). Edit .agents/skills/verify-mdp and point {link} at {}.",
                    expect.display()
                ));
            }
        }
        match path.symlink_metadata() {
            Ok(meta) if meta.file_type().is_symlink() => match fs::read_link(&path) {
                Ok(target) if target == expect => {}
                Ok(target) => errs.push(format!(
                    "FAIL skill-home: {link} points at {}. Point it at {}.",
                    target.display(),
                    expect.display()
                )),
                Err(err) => errs.push(format!("FAIL skill-home: cannot read {link}: {err}")),
            },
            Ok(_) => {
                // exception: reason = a Windows checkout without symlink privilege materializes the blob; expiry = 2027-04-20
                if cfg!(windows) && mode.as_deref() == Some("120000") {
                    continue;
                }
                errs.push(format!(
                    "FAIL skill-home: {link} is a real file or directory. Edit .agents/skills/verify-mdp only, and keep {link} as a symlink to {}.",
                    expect.display()
                ));
            }
            Err(_) => errs.push(format!(
                "FAIL skill-home: missing {link}. Symlink it to {}.",
                expect.display()
            )),
        }
    }
    errs
}

fn check_deskflow(root: &Path) -> Vec<String> {
    let needles = [
        "deskflow",
        "input-leap",
        "input_leap",
        "mouse_without_borders",
        "mousewithoutborders",
    ];
    let mut errs = Vec::new();
    let mut files = toml_files(root);
    files.extend(rust_files(root));
    for path in files {
        // exception: reason = this test names the forbidden crates in order to reject them; expiry = 2027-04-20
        if rel(root, &path) == "crates/mdp/tests/rules.rs" {
            continue;
        }
        let text = read_text(&path).to_ascii_lowercase();
        for needle in needles {
            if text.contains(needle) {
                errs.push(format!(
                    "FAIL no-deskflow: {} mentions {needle}. ADR 0001: build our own in this workspace. Do not add that crate.",
                    rel(root, &path)
                ));
            }
        }
    }
    errs
}

/// Drop `#` comments, line by line. The YAML and TOML these checks read keep
/// `#` out of their values, so a comment can never satisfy a rule.
fn strip_hash_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runner labels the workflow selects: `os:` matrix lists and literal `runs-on:`.
fn ci_runners(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in strip_hash_comments(text).lines() {
        let line = line.trim().trim_start_matches("- ").trim();
        let value = if let Some(v) = line.strip_prefix("os:") {
            v
        } else if let Some(v) = line.strip_prefix("runs-on:") {
            v
        } else {
            continue;
        };
        for item in value.trim().trim_matches(['[', ']']).split(',') {
            let item = item.trim().trim_matches(['"', '\'']);
            if !item.is_empty() && !item.contains("${{") {
                out.push(item.to_string());
            }
        }
    }
    out
}

fn check_ci(root: &Path) -> Vec<String> {
    let runners = ci_runners(&read_text(&root.join(".github/workflows/ci.yml")));
    let mut errs = Vec::new();
    for os in ["windows-latest", "macos-latest"] {
        if !runners.iter().any(|r| r == os) {
            errs.push(format!(
                "FAIL ci-peers: CI must run a {os} job (in the `os:` matrix or a `runs-on:`, not a comment). Windows and macOS are the Peer OSes; ubuntu-latest may be added as a build/test runner but does not replace them."
            ));
        }
    }
    errs
}

fn check_core(root: &Path) -> Vec<String> {
    let text = read_text(&root.join("crates/mdp-core/Cargo.toml"));
    let banned = [
        "eframe",
        "tray-icon",
        "core-graphics",
        "core-foundation",
        "gtk",
        "objc",
        "windows",
        "x11",
    ];
    banned
        .into_iter()
        .filter(|name| text.contains(name))
        .map(|name| {
            format!(
                "FAIL core-platform-free: crates/mdp-core/Cargo.toml depends on {name}. OS code belongs in crates/mdp/src/platform."
            )
        })
        .collect()
}

fn check_runtime(root: &Path) -> Vec<String> {
    let needle = format!("{}::{}::{}", "tokio", "runtime", "Builder");
    rust_files(root)
        .into_iter()
        .filter(|path| rel(root, path) != "crates/mdp/src/run.rs")
        .filter(|path| read_text(path).contains(&needle))
        .map(|path| {
            format!(
                "FAIL one-loop: {} builds a Tokio runtime. Call run::serve in crates/mdp/src/run.rs.",
                rel(root, &path)
            )
        })
        .collect()
}

fn check_merge(root: &Path) -> Vec<String> {
    let dir = root.join(".github/workflows");
    let Ok(entries) = fs::read_dir(&dir) else {
        return vec!["FAIL self-merge: missing .github/workflows. Self-merge is off (fleet ADR 0003). Do not add a merge job.".into()];
    };
    let mut errs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let text = read_text(&path).to_ascii_lowercase();
        if text.contains("gh pr merge") || text.contains("automerge") {
            errs.push(format!(
                "FAIL self-merge: {} merges pull requests. Self-merge is off (fleet ADR 0003). Open the PR and leave it.",
                rel(root, &path)
            ));
        }
    }
    errs
}

fn check_workspace(root: &Path) -> Vec<String> {
    let workspace = strip_hash_comments(&read_text(&root.join("Cargo.toml")));
    let mdp = strip_hash_comments(&read_text(&root.join("crates/mdp/Cargo.toml")));
    let mut errs = Vec::new();
    if !workspace.contains("members = [\"crates/mdp-core\", \"crates/mdp\"]") {
        errs.push("FAIL one-binary: Cargo.toml members must be exactly [\"crates/mdp-core\", \"crates/mdp\"].".into());
    }
    if mdp.matches("[[bin]]").count() != 1 || !mdp.contains("name = \"mdp\"") {
        errs.push(
            "FAIL one-binary: crates/mdp/Cargo.toml must declare exactly one [[bin]] named mdp."
                .into(),
        );
    }
    errs
}

fn check_linux(root: &Path) -> Vec<String> {
    let main = read_text(&root.join("crates/mdp/src/main.rs"));
    let mut errs = Vec::new();
    if !main.contains("native input hooks exist only on Windows and macOS") {
        errs.push("FAIL linux-not-peer: crates/mdp/src/main.rs must refuse native commands off Windows and macOS with `native input hooks exist only on Windows and macOS`. Do not make Linux a Peer.".into());
    }
    if !root
        .join("crates/mdp/src/platform/unsupported.rs")
        .is_file()
    {
        errs.push("FAIL linux-not-peer: missing crates/mdp/src/platform/unsupported.rs. Native returns PlatformError::Unsupported.".into());
    }
    errs
}

fn check_all(root: &Path) -> Vec<String> {
    let mut errs = Vec::new();
    errs.extend(check_progress(root));
    errs.extend(check_latency(root));
    errs.extend(check_skill(root));
    errs.extend(check_deskflow(root));
    errs.extend(check_ci(root));
    errs.extend(check_core(root));
    errs.extend(check_runtime(root));
    errs.extend(check_merge(root));
    errs.extend(check_workspace(root));
    errs.extend(check_linux(root));
    errs
}

#[test]
fn repo_rules_hold() {
    let errs = check_all(&repo_root());
    assert!(errs.is_empty(), "repo rules failed:\n{}", errs.join("\n"));
}

#[test]
fn bad_patterns_name_the_fix() {
    let dir = std::env::temp_dir().join(format!("mdp-rules-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("crates/mdp-core")).unwrap();
    fs::create_dir_all(dir.join("crates/mdp/src/platform")).unwrap();
    fs::create_dir_all(dir.join("crates/mdp/tests")).unwrap();
    fs::create_dir_all(dir.join("CONTEXT")).unwrap();
    fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    fs::create_dir_all(dir.join(".cursor/skills/verify-mdp")).unwrap();
    fs::create_dir_all(dir.join(".claude/skills")).unwrap();
    fs::write(dir.join("CONTEXT/progress.md"), "Not started\n").unwrap();
    fs::write(
        dir.join("CONTEXT/architecture.md"),
        "- **Crossing latency:** measured by the `latency` loopback bench.\n- **Clipboard:** capped at 1 MiB.\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp-core/Cargo.toml"),
        "eframe = \"0.36\"\nwindows = \"0.61\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "members = [\"crates/mdp-core\", \"crates/extra\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp/Cargo.toml"),
        "[[bin]]\nname = \"mdp\"\n[[bin]]\nname = \"mdp-helper\"\ndeskflow = \"1\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp/tests/e2e.rs"),
        "fn something_else() {}\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp/src/main.rs"),
        "fn main() { println!(\"linux peer\"); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp/src/other_loop.rs"),
        format!(
            "fn start() {{ let _ = {}::{}::{}::new_current_thread(); }}\n",
            "tokio", "runtime", "Builder"
        ),
    )
    .unwrap();
    fs::write(
        dir.join(".github/workflows/ci.yml"),
        "os: [ubuntu-latest, windows-latest]\n- run: gh pr merge --auto\n",
    )
    .unwrap();
    fs::write(
        dir.join(".cursor/skills/verify-mdp/SKILL.md"),
        "a copied skill\n",
    )
    .unwrap();
    fs::write(dir.join(".claude/skills/verify-mdp"), "not a symlink\n").unwrap();

    let errs = check_all(&dir);
    eprintln!("{}", errs.join("\n"));
    let _ = fs::remove_dir_all(&dir);

    for code in [
        "FAIL progress-md",
        "FAIL latency-name",
        "FAIL skill-home",
        "FAIL no-deskflow",
        "FAIL ci-peers",
        "FAIL core-platform-free",
        "FAIL one-loop",
        "FAIL self-merge",
        "FAIL one-binary",
        "FAIL linux-not-peer",
    ] {
        assert!(
            errs.iter().any(|err| err.contains(code)),
            "bad tree did not produce {code}:\n{}",
            errs.join("\n")
        );
    }
}

#[test]
fn comment_only_mentions_do_not_satisfy_rules() {
    let dir = std::env::temp_dir().join(format!("mdp-rules-comment-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    fs::create_dir_all(dir.join("crates/mdp")).unwrap();
    fs::write(
        dir.join(".github/workflows/ci.yml"),
        "jobs:\n  check:\n    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n        # macos-latest temporarily unavailable\n        os: [windows-latest, ubuntu-latest]\n",
    )
    .unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\n# members = [\"crates/mdp-core\", \"crates/mdp\"]\nmembers = [\"crates/mdp-core\", \"crates/mdp\", \"crates/extra\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("crates/mdp/Cargo.toml"),
        "[[bin]]\nname = \"mdp\"\n",
    )
    .unwrap();

    let ci = check_ci(&dir);
    let ws = check_workspace(&dir);
    let _ = fs::remove_dir_all(&dir);

    assert!(
        ci.iter()
            .any(|e| e.contains("FAIL ci-peers") && e.contains("macos-latest")),
        "macos-latest only in a comment must fail ci-peers, got {ci:?}"
    );
    assert!(
        !ci.iter().any(|e| e.contains("windows-latest")),
        "windows-latest is in the matrix, got {ci:?}"
    );
    assert!(
        ws.iter().any(|e| e.contains("FAIL one-binary")),
        "members only in a comment must fail one-binary, got {ws:?}"
    );
}
