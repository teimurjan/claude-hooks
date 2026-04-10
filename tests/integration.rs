use std::process::{Command, Stdio};
use std::io::Write;

fn run_hook(hook_id: &str, json: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claude-hooks"))
        .arg(hook_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

fn bash_json(cmd: &str) -> String {
    format!(r#"{{"tool_input":{{"command":"{cmd}"}}}}"#)
}

fn file_json(path: &str) -> String {
    format!(r#"{{"tool_input":{{"file_path":"{path}"}}}}"#)
}

// --- No hook ID: passthrough ---

#[test]
fn no_hook_id_passes_through() {
    let json = r#"{"test": true}"#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_claude-hooks"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), json);
}

// --- Unknown hook: passthrough ---

#[test]
fn unknown_hook_passes_through() {
    let json = r#"{"data": 1}"#;
    let (code, stdout, _) = run_hook("nonexistent-hook", json);
    assert_eq!(code, 0);
    assert_eq!(stdout, json);
}

// --- dev_server_block ---

#[test]
fn binary_blocks_dev_server() {
    let (code, _, stderr) = run_hook("pre-bash-dev-server-block", &bash_json("npm run dev"));
    assert_eq!(code, 2);
    assert!(stderr.contains("BLOCKED"));
}

#[test]
fn binary_allows_dev_in_tmux() {
    let (code, _, stderr) = run_hook(
        "pre-bash-dev-server-block",
        &bash_json(r#"tmux new-session -d -s dev \"npm run dev\""#),
    );
    assert_eq!(code, 0);
    assert!(!stderr.contains("BLOCKED"));
}

#[test]
fn binary_allows_non_dev() {
    let (code, stdout, _) = run_hook("pre-bash-dev-server-block", &bash_json("ls -la"));
    assert_eq!(code, 0);
    assert!(stdout.contains("ls -la"));
}

// --- doc_file_warning ---

#[test]
fn binary_doc_warning_warns_random_md() {
    let (code, _, stderr) = run_hook("doc-file-warning", &file_json("random-notes.md"));
    assert_eq!(code, 0);
    assert!(stderr.contains("Non-standard documentation"));
}

#[test]
fn binary_doc_warning_allows_readme() {
    let (code, _, stderr) = run_hook("doc-file-warning", &file_json("README.md"));
    assert_eq!(code, 0);
    assert!(!stderr.contains("Non-standard"));
}

// --- git_push_reminder ---

#[test]
fn binary_git_push_warns_in_strict() {
    let json = bash_json("git push origin main");
    let mut child = Command::new(env!("CARGO_BIN_EXE_claude-hooks"))
        .arg("pre-bash-git-push-reminder")
        .env("TEIMURJAN_HOOK_PROFILE", "strict")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Review changes before push"));
}

#[test]
fn binary_git_push_skipped_in_standard() {
    let json = bash_json("git push origin main");
    let (code, stdout, stderr) = run_hook("pre-bash-git-push-reminder", &json);
    assert_eq!(code, 0);
    assert_eq!(stdout, json);
    assert!(!stderr.contains("Review changes before push"));
}

// --- build_complete ---

#[test]
fn binary_build_complete_fires() {
    let (code, _, stderr) = run_hook("post-bash-build-complete", &bash_json("npm run build"));
    assert_eq!(code, 0);
    assert!(stderr.contains("Build completed"));
}

// --- Disabled hook passthrough ---

#[test]
fn binary_disabled_hook_passes_through() {
    let json = bash_json("npm run dev");
    let mut child = Command::new(env!("CARGO_BIN_EXE_claude-hooks"))
        .arg("pre-bash-dev-server-block")
        .env("TEIMURJAN_DISABLED_HOOKS", "pre-bash-dev-server-block")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), json);
}

// --- Minimal profile skips standard hooks ---

#[test]
fn binary_minimal_profile_skips_standard_hooks() {
    let json = bash_json("npm run dev");
    let mut child = Command::new(env!("CARGO_BIN_EXE_claude-hooks"))
        .arg("pre-bash-dev-server-block")
        .env("TEIMURJAN_HOOK_PROFILE", "minimal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    // In minimal profile, standard hooks are skipped → passthrough, exit 0
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), json);
}
