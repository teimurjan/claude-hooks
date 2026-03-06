use regex::Regex;
use std::env;
use std::fs;
use std::path::Path;

use crate::util::{get_command, get_file_path, parse_input, warn};
use crate::HookResult;

// --- Dev server blocker ---

fn split_shell_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            current.push(ch);
            i += 1;
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            current.push(ch);
            i += 1;
            continue;
        }

        let next = chars.get(i + 1).copied().unwrap_or('\0');

        if ch == ';'
            || (ch == '&' && next == '&')
            || (ch == '|' && next == '|')
            || (ch == '&' && next != '&')
        {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current.clear();
            if (ch == '&' && next == '&') || (ch == '|' && next == '|') {
                i += 1;
            }
            i += 1;
            continue;
        }

        current.push(ch);
        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }
    segments
}

pub fn dev_server_block(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let cmd = get_command(&input);
    if cmd.is_empty() {
        return HookResult::ok();
    }

    let dev_re =
        Regex::new(r"\b(npm\s+run\s+dev|pnpm(\s+run)?\s+dev|yarn\s+dev|bun\s+run\s+dev)\b")
            .unwrap();
    let tmux_re = Regex::new(r"^\s*tmux\s+(new|new-session|new-window|split-window)\b").unwrap();

    let segments = split_shell_segments(&cmd);
    let blocked = segments
        .iter()
        .any(|seg| dev_re.is_match(seg) && !tmux_re.is_match(seg));

    if blocked {
        warn("[Hook] BLOCKED: Dev server must run in tmux for log access");
        warn("[Hook] Use: tmux new-session -d -s dev \"npm run dev\"");
        warn("[Hook] Then: tmux attach -t dev");
        return HookResult::block();
    }

    HookResult::ok()
}

// --- Tmux reminder ---

pub fn tmux_reminder(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let cmd = get_command(&input);
    if cmd.is_empty() {
        return HookResult::ok();
    }

    if env::var("TMUX").is_ok() {
        return HookResult::ok();
    }

    let long_running = Regex::new(
        r"(npm (install|test)|pnpm (install|test)|yarn (install|test)?|bun (install|test)|cargo build|make\b|docker\b|pytest|vitest|playwright)"
    ).unwrap();

    if long_running.is_match(&cmd) {
        warn("[Hook] Consider running in tmux for session persistence");
        warn("[Hook] tmux new -s dev  |  tmux attach -t dev");
    }

    HookResult::ok()
}

// --- Git push reminder ---

pub fn git_push_reminder(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let cmd = get_command(&input);

    if Regex::new(r"\bgit\s+push\b").unwrap().is_match(&cmd) {
        warn("[Hook] Review changes before push...");
        warn("[Hook] Continuing with push (remove this hook to add interactive review)");
    }

    HookResult::ok()
}

// --- Doc file warning ---

fn is_allowed_doc_path(file_path: &str) -> bool {
    let normalized = file_path.replace('\\', "/");
    let basename = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    if !file_path.ends_with(".md") && !file_path.ends_with(".txt") {
        return true;
    }

    let allowed_basenames = [
        "README.md",
        "CLAUDE.md",
        "AGENTS.md",
        "CONTRIBUTING.md",
        "CHANGELOG.md",
        "LICENSE.md",
        "SKILL.md",
        "MEMORY.md",
        "WORKLOG.md",
    ];
    let upper = basename.to_uppercase();
    if allowed_basenames.iter().any(|&a| upper == a.to_uppercase()) {
        return true;
    }

    if normalized.contains(".claude/commands/")
        || normalized.contains(".claude/plans/")
        || normalized.contains(".claude/projects/")
    {
        return true;
    }

    let dir_patterns = ["/docs/", "/skills/", "/.history/", "/memory/"];
    if dir_patterns.iter().any(|p| normalized.contains(p))
        || normalized.starts_with("docs/")
        || normalized.starts_with("skills/")
        || normalized.starts_with(".history/")
        || normalized.starts_with("memory/")
    {
        return true;
    }

    if basename.ends_with(".plan.md") {
        return true;
    }

    false
}

pub fn doc_file_warning(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let file_path = get_file_path(&input);

    if !file_path.is_empty() && !is_allowed_doc_path(&file_path) {
        warn("[Hook] WARNING: Non-standard documentation file detected");
        warn(&format!("[Hook] File: {file_path}"));
        warn("[Hook] Consider consolidating into README.md or docs/ directory");
    }

    HookResult::ok()
}

// --- Strategic compact suggester ---

pub fn suggest_compact(_raw: &str) -> HookResult {
    let session_id = env::var("CLAUDE_SESSION_ID").unwrap_or_else(|_| "default".into());
    let counter_file = env::temp_dir().join(format!("claude-tool-count-{session_id}"));

    let threshold: u64 = env::var("COMPACT_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &u64| n > 0 && n <= 10000)
        .unwrap_or(50);

    let count = match fs::read_to_string(&counter_file) {
        Ok(content) => content
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|&n| n > 0 && n <= 1_000_000)
            .map(|n| n + 1)
            .unwrap_or(1),
        Err(_) => 1,
    };

    let _ = fs::write(&counter_file, count.to_string());

    if count == threshold {
        warn(&format!(
            "[StrategicCompact] {threshold} tool calls reached - consider /compact if transitioning phases"
        ));
    }

    if count > threshold && (count - threshold) % 25 == 0 {
        warn(&format!(
            "[StrategicCompact] {count} tool calls - good checkpoint for /compact if context is stale"
        ));
    }

    HookResult::ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_json(cmd: &str) -> String {
        format!(r#"{{"tool_input":{{"command":"{cmd}"}}}}"#)
    }

    fn file_json(path: &str) -> String {
        format!(r#"{{"tool_input":{{"file_path":"{path}"}}}}"#)
    }

    // --- split_shell_segments ---

    #[test]
    fn segments_simple_semicolon() {
        assert_eq!(split_shell_segments("ls; pwd"), vec!["ls", "pwd"]);
    }

    #[test]
    fn segments_and_chain() {
        assert_eq!(split_shell_segments("a && b"), vec!["a", "b"]);
    }

    #[test]
    fn segments_or_chain() {
        assert_eq!(split_shell_segments("a || b"), vec!["a", "b"]);
    }

    #[test]
    fn segments_background() {
        assert_eq!(split_shell_segments("a & b"), vec!["a", "b"]);
    }

    #[test]
    fn segments_preserves_quotes() {
        let segs = split_shell_segments(r#"echo "a && b"; pwd"#);
        assert_eq!(segs, vec![r#"echo "a && b""#, "pwd"]);
    }

    #[test]
    fn segments_single_command() {
        assert_eq!(split_shell_segments("ls -la"), vec!["ls -la"]);
    }

    #[test]
    fn segments_empty() {
        assert!(split_shell_segments("").is_empty());
    }

    // --- dev_server_block ---

    #[test]
    fn blocks_npm_run_dev() {
        let r = dev_server_block(&bash_json("npm run dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn blocks_pnpm_dev() {
        let r = dev_server_block(&bash_json("pnpm dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn blocks_pnpm_run_dev() {
        let r = dev_server_block(&bash_json("pnpm run dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn blocks_bun_run_dev() {
        let r = dev_server_block(&bash_json("bun run dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn blocks_yarn_dev() {
        let r = dev_server_block(&bash_json("yarn dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn allows_dev_in_tmux() {
        let r = dev_server_block(&bash_json(r#"tmux new-session -d -s dev "npm run dev""#));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn allows_non_dev_commands() {
        let r = dev_server_block(&bash_json("npm run build"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn blocks_dev_in_chain() {
        let r = dev_server_block(&bash_json("cd app && npm run dev"));
        assert_eq!(r.exit_code, 2);
    }

    #[test]
    fn allows_empty_command() {
        let r = dev_server_block(&bash_json(""));
        assert_eq!(r.exit_code, 0);
    }

    // --- git_push_reminder ---

    #[test]
    fn push_reminder_fires_on_git_push() {
        let r = git_push_reminder(&bash_json("git push origin main"));
        assert_eq!(r.exit_code, 0); // warns but doesn't block
    }

    #[test]
    fn push_reminder_ignores_other_git() {
        let r = git_push_reminder(&bash_json("git commit -m 'test'"));
        assert_eq!(r.exit_code, 0);
    }

    // --- is_allowed_doc_path ---

    #[test]
    fn allows_non_doc_files() {
        assert!(is_allowed_doc_path("src/main.rs"));
        assert!(is_allowed_doc_path("index.ts"));
    }

    #[test]
    fn allows_standard_doc_names() {
        assert!(is_allowed_doc_path("README.md"));
        assert!(is_allowed_doc_path("CLAUDE.md"));
        assert!(is_allowed_doc_path("CONTRIBUTING.md"));
        assert!(is_allowed_doc_path("CHANGELOG.md"));
        assert!(is_allowed_doc_path("MEMORY.md"));
    }

    #[test]
    fn allows_case_insensitive_doc_names() {
        assert!(is_allowed_doc_path("readme.md"));
        assert!(is_allowed_doc_path("Readme.md"));
    }

    #[test]
    fn allows_docs_directory() {
        assert!(is_allowed_doc_path("/project/docs/guide.md"));
        assert!(is_allowed_doc_path("docs/api.md"));
    }

    #[test]
    fn allows_claude_paths() {
        assert!(is_allowed_doc_path(".claude/commands/test.md"));
        assert!(is_allowed_doc_path(".claude/plans/plan.md"));
        assert!(is_allowed_doc_path(".claude/projects/foo/bar.md"));
    }

    #[test]
    fn allows_plan_suffix() {
        assert!(is_allowed_doc_path("feature.plan.md"));
    }

    #[test]
    fn rejects_random_md_files() {
        assert!(!is_allowed_doc_path("notes.md"));
        assert!(!is_allowed_doc_path("todo.md"));
        assert!(!is_allowed_doc_path("src/random.txt"));
    }

    #[test]
    fn allows_skills_and_memory_dirs() {
        assert!(is_allowed_doc_path("/project/skills/learned.md"));
        assert!(is_allowed_doc_path("memory/patterns.md"));
    }

    // --- doc_file_warning ---

    #[test]
    fn doc_warning_allows_readme() {
        let r = doc_file_warning(&file_json("README.md"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn doc_warning_warns_on_random_md() {
        let r = doc_file_warning(&file_json("notes.md"));
        assert_eq!(r.exit_code, 0); // warns but doesn't block
    }

    #[test]
    fn doc_warning_ignores_empty_path() {
        let r = doc_file_warning(&file_json(""));
        assert_eq!(r.exit_code, 0);
    }
}
