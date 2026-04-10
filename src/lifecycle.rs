use std::env;
use std::path::Path;

use serde_json::Value;

use crate::util::{
    append_file, claude_dir, ensure_dir, git_modified_files, is_git_repo, iso_timestamp,
    parse_input, read_file, warn,
};
use crate::HookResult;

// --- Session start (project detection) ---

pub fn session_start(_raw: &str) -> HookResult {
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

    HookResult::ok()
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
