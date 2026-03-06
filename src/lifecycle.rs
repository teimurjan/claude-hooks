use std::collections::HashSet;
use std::env;
use std::path::Path;

use serde_json::Value;

use crate::util::{
    append_file, claude_dir, date_string, datetime_string, ensure_dir, find_files,
    git_modified_files, is_git_repo, iso_timestamp, learned_skills_dir, parse_input, read_file,
    session_id_short, sessions_dir, time_string, warn, write_file,
};
use crate::HookResult;

// --- Session start ---

pub fn session_start(_raw: &str) -> HookResult {
    let sess_dir = sessions_dir();
    let learned_dir = learned_skills_dir();
    ensure_dir(&sess_dir);
    ensure_dir(&learned_dir);

    let recent = find_files(&sess_dir, "-session.tmp", Some(7));
    if !recent.is_empty() {
        warn(&format!(
            "[SessionStart] Found {} recent session(s)",
            recent.len()
        ));
        if let Some(content) = read_file(&recent[0].path) {
            if !content.contains("[Session context goes here]") {
                println!("Previous session summary:\n{content}");
            }
        }
    }

    let skills = find_files(&learned_dir, ".md", None);
    if !skills.is_empty() {
        warn(&format!(
            "[SessionStart] {} learned skill(s) available in {}",
            skills.len(),
            learned_dir.display()
        ));
    }

    let pm = detect_package_manager();
    warn(&format!(
        "[SessionStart] Package manager: {} ({})",
        pm.name, pm.source
    ));

    let project = detect_project_type();
    if !project.languages.is_empty() || !project.frameworks.is_empty() {
        let mut parts = Vec::new();
        if !project.languages.is_empty() {
            parts.push(format!("languages: {}", project.languages.join(", ")));
        }
        if !project.frameworks.is_empty() {
            parts.push(format!("frameworks: {}", project.frameworks.join(", ")));
        }
        warn(&format!(
            "[SessionStart] Project detected — {}",
            parts.join("; ")
        ));
    }

    HookResult::custom_output()
}

struct PackageManager {
    name: &'static str,
    source: &'static str,
}

fn detect_package_manager() -> PackageManager {
    if let Ok(pm) = env::var("CLAUDE_PACKAGE_MANAGER") {
        let name = match pm.to_lowercase().as_str() {
            "bun" => "bun",
            "pnpm" => "pnpm",
            "yarn" => "yarn",
            _ => "npm",
        };
        return PackageManager {
            name,
            source: "env",
        };
    }

    let cwd = env::current_dir().unwrap_or_default();
    let lockfiles: &[(&str, &str)] = &[
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("package-lock.json", "npm"),
    ];

    for (file, name) in lockfiles {
        if cwd.join(file).exists() {
            return PackageManager {
                name,
                source: "lockfile",
            };
        }
    }

    PackageManager {
        name: "npm",
        source: "default",
    }
}

struct ProjectInfo {
    languages: Vec<&'static str>,
    frameworks: Vec<&'static str>,
}

fn detect_project_type() -> ProjectInfo {
    let cwd = env::current_dir().unwrap_or_default();
    let mut languages = Vec::new();
    let mut frameworks = Vec::new();

    let lang_markers: &[(&str, &str)] = &[
        ("Cargo.toml", "Rust"),
        ("go.mod", "Go"),
        ("pyproject.toml", "Python"),
        ("requirements.txt", "Python"),
        ("package.json", "JavaScript/TypeScript"),
        ("Gemfile", "Ruby"),
        ("build.gradle", "Java"),
        ("build.gradle.kts", "Kotlin"),
        ("pom.xml", "Java"),
        ("mix.exs", "Elixir"),
        ("Package.swift", "Swift"),
    ];

    for (marker, lang) in lang_markers {
        if cwd.join(marker).exists() && !languages.contains(lang) {
            languages.push(*lang);
        }
    }

    let framework_markers: &[(&str, &str)] = &[
        ("next.config.js", "Next.js"),
        ("next.config.mjs", "Next.js"),
        ("next.config.ts", "Next.js"),
        ("nuxt.config.ts", "Nuxt"),
        ("angular.json", "Angular"),
        ("svelte.config.js", "Svelte"),
        ("astro.config.mjs", "Astro"),
        ("vite.config.ts", "Vite"),
        ("vite.config.js", "Vite"),
        ("tailwind.config.js", "Tailwind"),
        ("tailwind.config.ts", "Tailwind"),
        ("django-admin.py", "Django"),
        ("manage.py", "Django"),
    ];

    for (marker, fw) in framework_markers {
        if cwd.join(marker).exists() && !frameworks.contains(fw) {
            frameworks.push(*fw);
        }
    }

    // Check tsconfig for TypeScript specifically
    if cwd.join("tsconfig.json").exists()
        && !languages.contains(&"JavaScript/TypeScript")
    {
        languages.push("JavaScript/TypeScript");
    }

    ProjectInfo {
        languages,
        frameworks,
    }
}

// --- Pre-compact ---

pub fn pre_compact(_raw: &str) -> HookResult {
    let sess_dir = sessions_dir();
    let compaction_log = sess_dir.join("compaction-log.txt");
    ensure_dir(&sess_dir);

    let timestamp = datetime_string();
    append_file(
        &compaction_log,
        &format!("[{timestamp}] Context compaction triggered\n"),
    );

    let sessions = find_files(&sess_dir, "-session.tmp", None);
    if let Some(active) = sessions.first() {
        let t = time_string();
        append_file(
            &active.path,
            &format!("\n---\n**[Compaction occurred at {t}]** - Context was summarized\n"),
        );
    }

    warn("[PreCompact] State saved before compaction");
    HookResult::ok()
}

// --- Check console.log (Stop hook) ---

pub fn check_console_log(_raw: &str) -> HookResult {
    if !is_git_repo() {
        return HookResult::ok();
    }

    let excluded = [
        ".test.", ".spec.", ".config.", "scripts/", "__tests__/", "__mocks__/",
    ];

    let files = git_modified_files(&[".ts", ".tsx", ".js", ".jsx"]);
    let mut found_any = false;

    for file in &files {
        if excluded.iter().any(|pat| file.contains(pat)) {
            continue;
        }
        if !Path::new(file).exists() {
            continue;
        }
        if let Some(content) = read_file(Path::new(file)) {
            if content.contains("console.log") {
                warn(&format!("[Hook] WARNING: console.log found in {file}"));
                found_any = true;
            }
        }
    }

    if found_any {
        warn("[Hook] Remove console.log statements before committing");
    }

    HookResult::ok()
}

// --- Session end (persist state) ---

const SUMMARY_START: &str = "<!-- ECC:SUMMARY:START -->";
const SUMMARY_END: &str = "<!-- ECC:SUMMARY:END -->";

struct SessionSummary {
    user_messages: Vec<String>,
    tools_used: Vec<String>,
    files_modified: Vec<String>,
    total_messages: usize,
}

fn extract_session_summary(transcript_path: &str) -> Option<SessionSummary> {
    let content = read_file(Path::new(transcript_path))?;
    let mut user_messages = Vec::new();
    let mut tools_used = HashSet::new();
    let mut files_modified = HashSet::new();

    for line in content.lines().filter(|l| !l.is_empty()) {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        // User messages
        let msg_type = entry
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("");
        let msg_role = entry
            .pointer("/message/role")
            .and_then(Value::as_str)
            .unwrap_or("");

        if msg_type == "user" || msg_role == "user" {
            let raw_content = entry
                .pointer("/message/content")
                .or_else(|| entry.get("content"));

            let text = extract_text_content(raw_content);
            if !text.is_empty() {
                let truncated: String = text.chars().take(200).collect();
                user_messages.push(truncated);
            }
        }

        // Direct tool_use entries
        if msg_type == "tool_use" || entry.get("tool_name").is_some() {
            let tool_name = entry
                .get("tool_name")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if !tool_name.is_empty() {
                tools_used.insert(tool_name.to_string());
            }

            let fp = entry
                .pointer("/tool_input/file_path")
                .or_else(|| entry.pointer("/input/file_path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if !fp.is_empty() && (tool_name == "Edit" || tool_name == "Write") {
                files_modified.insert(fp.to_string());
            }
        }

        // Tool uses inside assistant message content blocks
        if msg_type == "assistant" {
            if let Some(blocks) = entry.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    if block_type != "tool_use" {
                        continue;
                    }
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                    if !name.is_empty() {
                        tools_used.insert(name.to_string());
                    }
                    let fp = block
                        .pointer("/input/file_path")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !fp.is_empty() && (name == "Edit" || name == "Write") {
                        files_modified.insert(fp.to_string());
                    }
                }
            }
        }
    }

    if user_messages.is_empty() {
        return None;
    }

    let total = user_messages.len();
    Some(SessionSummary {
        user_messages: user_messages.into_iter().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect(),
        tools_used: tools_used.into_iter().take(20).collect(),
        files_modified: files_modified.into_iter().take(30).collect(),
        total_messages: total,
    })
}

fn extract_text_content(value: Option<&Value>) -> String {
    let Some(v) = value else {
        return String::new();
    };
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn build_summary_block(summary: &SessionSummary) -> String {
    let mut section = String::from("## Session Summary\n\n### Tasks\n");

    for msg in &summary.user_messages {
        let sanitized = msg.replace('\n', " ").replace('`', "\\`");
        section.push_str(&format!("- {sanitized}\n"));
    }
    section.push('\n');

    if !summary.files_modified.is_empty() {
        section.push_str("### Files Modified\n");
        for f in &summary.files_modified {
            section.push_str(&format!("- {f}\n"));
        }
        section.push('\n');
    }

    if !summary.tools_used.is_empty() {
        section.push_str(&format!(
            "### Tools Used\n{}\n\n",
            summary.tools_used.join(", ")
        ));
    }

    section.push_str(&format!(
        "### Stats\n- Total user messages: {}\n",
        summary.total_messages
    ));

    format!("{SUMMARY_START}\n{}\n{SUMMARY_END}", section.trim())
}

pub fn session_end(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let transcript_path = input
        .get("transcript_path")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| env::var("CLAUDE_TRANSCRIPT_PATH").ok());

    let sess_dir = sessions_dir();
    let today = date_string();
    let short_id = session_id_short();
    let session_file = sess_dir.join(format!("{today}-{short_id}-session.tmp"));
    ensure_dir(&sess_dir);

    let current_time = time_string();

    let summary = transcript_path
        .as_deref()
        .filter(|p| Path::new(p).exists())
        .and_then(extract_session_summary);

    if session_file.exists() {
        // Update timestamp
        if let Some(content) = read_file(&session_file) {
            let updated = regex::Regex::new(r"\*\*Last Updated:\*\*.*")
                .unwrap()
                .replace(&content, &format!("**Last Updated:** {current_time}"))
                .into_owned();

            if let Some(ref summary) = summary {
                let block = build_summary_block(summary);
                let final_content = if updated.contains(SUMMARY_START) && updated.contains(SUMMARY_END) {
                    replace_between(&updated, SUMMARY_START, SUMMARY_END, &block)
                } else {
                    updated
                };
                write_file(&session_file, &final_content);
            } else {
                write_file(&session_file, &updated);
            }
        }
        warn(&format!(
            "[SessionEnd] Updated session file: {}",
            session_file.display()
        ));
    } else {
        let summary_section = if let Some(ref summary) = summary {
            let block = build_summary_block(summary);
            format!("{block}\n\n### Notes for Next Session\n-\n\n### Context to Load\n```\n[relevant files]\n```")
        } else {
            "## Current State\n\n[Session context goes here]\n\n### Completed\n- [ ]\n\n### In Progress\n- [ ]\n\n### Notes for Next Session\n-\n\n### Context to Load\n```\n[relevant files]\n```".into()
        };

        let template = format!(
            "# Session: {today}\n**Date:** {today}\n**Started:** {current_time}\n**Last Updated:** {current_time}\n\n---\n\n{summary_section}\n"
        );

        write_file(&session_file, &template);
        warn(&format!(
            "[SessionEnd] Created session file: {}",
            session_file.display()
        ));
    }

    HookResult::ok()
}

fn replace_between(content: &str, start_marker: &str, end_marker: &str, replacement: &str) -> String {
    let Some(start_pos) = content.find(start_marker) else {
        return content.to_string();
    };
    let Some(end_pos) = content.find(end_marker) else {
        return content.to_string();
    };
    let after_end = end_pos + end_marker.len();
    format!("{}{}{}", &content[..start_pos], replacement, &content[after_end..])
}

// --- Evaluate session ---

pub fn evaluate_session(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let transcript_path = input
        .get("transcript_path")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| env::var("CLAUDE_TRANSCRIPT_PATH").ok());

    let min_length: usize = 10;
    let learned_dir = learned_skills_dir();
    ensure_dir(&learned_dir);

    let Some(path) = transcript_path.as_deref() else {
        return HookResult::ok();
    };

    if !Path::new(path).exists() {
        return HookResult::ok();
    }

    let Some(content) = read_file(Path::new(path)) else {
        return HookResult::ok();
    };

    let message_count = content.matches("\"type\"").count();

    if message_count < min_length {
        warn(&format!(
            "[ContinuousLearning] Session too short ({message_count} messages), skipping"
        ));
        return HookResult::ok();
    }

    warn(&format!(
        "[ContinuousLearning] Session has {message_count} messages - evaluate for extractable patterns"
    ));
    warn(&format!(
        "[ContinuousLearning] Save learned skills to: {}",
        learned_dir.display()
    ));

    HookResult::ok()
}

// --- Cost tracker ---

pub fn cost_tracker(raw: &str) -> HookResult {
    let input = parse_input(raw);

    let usage = input
        .get("usage")
        .or_else(|| input.get("token_usage"))
        .cloned()
        .unwrap_or(Value::Null);

    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let model = input
        .get("model")
        .or_else(|| input.pointer("/_cursor/model"))
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| env::var("CLAUDE_MODEL").ok())
        .unwrap_or_else(|| "unknown".into());

    let session_id = env::var("CLAUDE_SESSION_ID").unwrap_or_else(|_| "default".into());

    let metrics_dir = claude_dir().join("metrics");
    ensure_dir(&metrics_dir);

    let estimated_cost = estimate_cost(&model, input_tokens, output_tokens);

    let row = serde_json::json!({
        "timestamp": iso_timestamp(),
        "session_id": session_id,
        "model": model,
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "estimated_cost_usd": estimated_cost,
    });

    append_file(
        &metrics_dir.join("costs.jsonl"),
        &format!("{}\n", row),
    );

    HookResult::ok()
}

fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let normalized = model.to_ascii_lowercase();
    let (rate_in, rate_out) = if normalized.contains("haiku") {
        (0.8, 4.0)
    } else if normalized.contains("opus") {
        (15.0, 75.0)
    } else {
        (3.0, 15.0) // sonnet / default
    };

    let cost =
        (input_tokens as f64 / 1_000_000.0) * rate_in + (output_tokens as f64 / 1_000_000.0) * rate_out;
    (cost * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- estimate_cost ---

    #[test]
    fn cost_sonnet_default() {
        let cost = estimate_cost("claude-sonnet-4-20250514", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.001, "sonnet cost = {cost}");
    }

    #[test]
    fn cost_opus() {
        let cost = estimate_cost("claude-opus-4-20250514", 1_000_000, 1_000_000);
        assert!((cost - 90.0).abs() < 0.001, "opus cost = {cost}");
    }

    #[test]
    fn cost_haiku() {
        let cost = estimate_cost("claude-haiku-3-5-20250101", 1_000_000, 1_000_000);
        assert!((cost - 4.8).abs() < 0.001, "haiku cost = {cost}");
    }

    #[test]
    fn cost_zero_tokens() {
        assert_eq!(estimate_cost("sonnet", 0, 0), 0.0);
    }

    #[test]
    fn cost_unknown_model_uses_sonnet_rates() {
        let cost = estimate_cost("gpt-4o", 1_000_000, 0);
        assert!((cost - 3.0).abs() < 0.001);
    }

    // --- extract_text_content ---

    #[test]
    fn extract_text_from_string() {
        let val = Value::String("hello world".into());
        assert_eq!(extract_text_content(Some(&val)), "hello world");
    }

    #[test]
    fn extract_text_from_array() {
        let val: Value = serde_json::json!([
            {"type": "text", "text": "first"},
            {"type": "text", "text": "second"},
        ]);
        assert_eq!(extract_text_content(Some(&val)), "first second");
    }

    #[test]
    fn extract_text_none() {
        assert_eq!(extract_text_content(None), "");
    }

    #[test]
    fn extract_text_non_string_non_array() {
        let val = Value::Number(42.into());
        assert_eq!(extract_text_content(Some(&val)), "");
    }

    // --- replace_between ---

    #[test]
    fn replace_between_basic() {
        let content = "before<!-- START -->old<!-- END -->after";
        let result = replace_between(content, "<!-- START -->", "<!-- END -->", "[NEW]");
        assert_eq!(result, "before[NEW]after");
    }

    #[test]
    fn replace_between_no_start_marker() {
        let content = "no markers here";
        let result = replace_between(content, "<!-- START -->", "<!-- END -->", "[NEW]");
        assert_eq!(result, "no markers here");
    }

    #[test]
    fn replace_between_no_end_marker() {
        let content = "has <!-- START --> but no end";
        let result = replace_between(content, "<!-- START -->", "<!-- END -->", "[NEW]");
        assert_eq!(result, content);
    }

    // --- build_summary_block ---

    #[test]
    fn build_summary_contains_markers() {
        let summary = SessionSummary {
            user_messages: vec!["task one".into()],
            tools_used: vec!["Read".into()],
            files_modified: vec!["src/main.rs".into()],
            total_messages: 1,
        };
        let block = build_summary_block(&summary);
        assert!(block.contains(SUMMARY_START));
        assert!(block.contains(SUMMARY_END));
        assert!(block.contains("task one"));
        assert!(block.contains("src/main.rs"));
        assert!(block.contains("Read"));
    }

    #[test]
    fn build_summary_no_files() {
        let summary = SessionSummary {
            user_messages: vec!["hello".into()],
            tools_used: vec![],
            files_modified: vec![],
            total_messages: 1,
        };
        let block = build_summary_block(&summary);
        assert!(!block.contains("### Files Modified"));
    }

    // --- cost_tracker ---

    #[test]
    fn cost_tracker_writes_jsonl() {
        let tmp = std::env::temp_dir().join("claude-hooks-test-ct");
        let _ = std::fs::create_dir_all(&tmp);

        unsafe { env::set_var("HOME", tmp.to_string_lossy().as_ref()) };
        unsafe { env::set_var("CLAUDE_SESSION_ID", "test-session-ct") };
        let metrics_dir = tmp.join(".claude").join("metrics");
        let costs_file = metrics_dir.join("costs.jsonl");
        let _ = std::fs::remove_file(&costs_file);

        let raw = r#"{"model":"claude-sonnet-4","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let r = cost_tracker(raw);
        assert_eq!(r.exit_code, 0);

        let content = std::fs::read_to_string(&costs_file).unwrap();
        assert!(content.contains("claude-sonnet-4"));
        assert!(content.contains("test-session-ct"));

        unsafe { env::remove_var("CLAUDE_SESSION_ID") };
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
