use std::env;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

// --- Paths ---

pub fn home_dir() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn claude_dir() -> PathBuf {
    home_dir().join(".claude")
}

// --- File operations ---

pub fn ensure_dir(path: &Path) {
    if !path.exists() {
        let _ = fs::create_dir_all(path);
    }
}

pub fn read_file(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn append_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        ensure_dir(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(content.as_bytes());
    }
}

// --- File search ---

// --- JSON helpers ---

pub fn parse_input(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

pub fn get_command(input: &Value) -> String {
    input
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn get_file_path(input: &Value) -> String {
    input
        .pointer("/tool_input/file_path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn get_tool_output(input: &Value) -> String {
    input
        .pointer("/tool_output/output")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

// --- Output ---

pub fn warn(msg: &str) {
    eprintln!("{msg}");
}

// --- Time (local via libc) ---

struct LocalTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    min: u32,
    sec: u32,
}

fn local_now() -> LocalTime {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
        }
    }
}

pub fn iso_timestamp() -> String {
    let t = local_now();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.min, t.sec
    )
}

// --- Git ---

pub fn is_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn git_modified_files(extension_patterns: &[&str]) -> Vec<String> {
    if !is_git_repo() {
        return vec![];
    }
    let Ok(output) = Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }

    let all_files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    if extension_patterns.is_empty() {
        return all_files;
    }

    all_files
        .into_iter()
        .filter(|f| extension_patterns.iter().any(|ext| f.ends_with(ext)))
        .collect()
}

// --- Project root detection ---

pub fn find_project_root(start: &Path, marker: &str) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..20 {
        if dir.join(marker).exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_valid_json() {
        let val = parse_input(r#"{"tool_input":{"command":"ls"}}"#);
        assert_eq!(get_command(&val), "ls");
    }

    #[test]
    fn parse_input_invalid_json() {
        let val = parse_input("not json");
        assert!(val.is_null());
    }

    #[test]
    fn get_command_missing() {
        let val = parse_input(r#"{"other":"field"}"#);
        assert_eq!(get_command(&val), "");
    }

    #[test]
    fn get_file_path_present() {
        let val = parse_input(r#"{"tool_input":{"file_path":"/tmp/foo.ts"}}"#);
        assert_eq!(get_file_path(&val), "/tmp/foo.ts");
    }

    #[test]
    fn get_file_path_missing() {
        let val = parse_input(r#"{}"#);
        assert_eq!(get_file_path(&val), "");
    }

    #[test]
    fn get_tool_output_present() {
        let val = parse_input(r#"{"tool_output":{"output":"hello"}}"#);
        assert_eq!(get_tool_output(&val), "hello");
    }

    #[test]
    fn iso_timestamp_format() {
        let ts = iso_timestamp();
        assert!(
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$")
                .unwrap()
                .is_match(&ts),
            "iso_timestamp() = {ts}"
        );
    }

    #[test]
    fn find_project_root_found() {
        let tmp = env::temp_dir().join("claude-hooks-test-fpr");
        let nested = tmp.join("a").join("b");
        let _ = fs::create_dir_all(&nested);
        let _ = fs::write(tmp.join("Cargo.toml"), "");
        assert_eq!(find_project_root(&nested, "Cargo.toml"), Some(tmp.clone()));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_project_root_not_found() {
        assert_eq!(find_project_root(Path::new("/nonexistent/deep/path"), "XXXNOTEXIST"), None);
    }

    #[test]
    fn append_file_creates_and_appends() {
        let tmp = env::temp_dir().join("claude-hooks-test-af");
        let _ = fs::create_dir_all(&tmp);
        let file = tmp.join("append.txt");
        let _ = fs::remove_file(&file);
        append_file(&file, "one\n");
        append_file(&file, "two\n");
        assert_eq!(read_file(&file), Some("one\ntwo\n".into()));
        let _ = fs::remove_dir_all(&tmp);
    }
}
