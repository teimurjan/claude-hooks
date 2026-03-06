use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::util::{find_project_root, get_command, get_file_path, get_tool_output, parse_input, read_file, warn};
use crate::HookResult;

// --- PR created logger ---

pub fn pr_created(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let cmd = get_command(&input);

    if !Regex::new(r"\bgh\s+pr\s+create\b")
        .unwrap()
        .is_match(&cmd)
    {
        return HookResult::ok();
    }

    let output = get_tool_output(&input);
    let url_re = Regex::new(r"https://github\.com/([^/]+/[^/]+)/pull/(\d+)").unwrap();

    if let Some(caps) = url_re.captures(&output) {
        let repo = &caps[1];
        let pr_num = &caps[2];
        let url = &caps[0];
        warn(&format!("[Hook] PR created: {url}"));
        warn(&format!("[Hook] To review: gh pr review {pr_num} --repo {repo}"));
    }

    HookResult::ok()
}

// --- Build complete notification ---

pub fn build_complete(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let cmd = get_command(&input);

    let build_re = Regex::new(r"(npm run build|pnpm build|yarn build)").unwrap();
    if build_re.is_match(&cmd) {
        warn("[Hook] Build completed - async analysis running in background");
    }

    HookResult::ok()
}

// --- Quality gate ---

pub fn quality_gate(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let file_path = get_file_path(&input);
    if file_path.is_empty() || !Path::new(&file_path).exists() {
        return HookResult::ok();
    }

    let ext = Path::new(&file_path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let fix = std::env::var("TEIMURJAN_QUALITY_GATE_FIX")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");
    let strict = std::env::var("TEIMURJAN_QUALITY_GATE_STRICT")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");

    match ext.as_str() {
        "ts" | "tsx" | "js" | "jsx" | "json" | "md" => {
            let cwd = std::env::current_dir().unwrap_or_default();
            if cwd.join("biome.json").exists() || cwd.join("biome.jsonc").exists() {
                let mut args = vec!["biome", "check", &file_path];
                if fix {
                    args.push("--write");
                }
                let result = Command::new("npx").args(&args).output();
                if strict {
                    if let Ok(o) = result {
                        if !o.status.success() {
                            warn(&format!("[QualityGate] Biome check failed for {file_path}"));
                        }
                    }
                }
            } else {
                let verb = if fix { "--write" } else { "--check" };
                let result = Command::new("npx")
                    .args(["prettier", verb, &file_path])
                    .output();
                if strict {
                    if let Ok(o) = result {
                        if !o.status.success() {
                            warn(&format!(
                                "[QualityGate] Prettier check failed for {file_path}"
                            ));
                        }
                    }
                }
            }
        }
        "go" if fix => {
            let _ = Command::new("gofmt").args(["-w", &file_path]).output();
        }
        "py" => {
            let mut args: Vec<&str> = vec!["format"];
            if !fix {
                args.push("--check");
            }
            args.push(&file_path);
            let result = Command::new("ruff").args(&args).output();
            if strict {
                if let Ok(o) = result {
                    if !o.status.success() {
                        warn(&format!("[QualityGate] Ruff check failed for {file_path}"));
                    }
                }
            }
        }
        _ => {}
    }

    HookResult::ok()
}

// --- Auto-format JS/TS files ---

fn detect_formatter(project_root: &Path) -> Option<&'static str> {
    let biome_configs = ["biome.json", "biome.jsonc"];
    for cfg in biome_configs {
        if project_root.join(cfg).exists() {
            return Some("biome");
        }
    }

    let prettier_configs = [
        ".prettierrc",
        ".prettierrc.json",
        ".prettierrc.js",
        ".prettierrc.cjs",
        ".prettierrc.mjs",
        ".prettierrc.yml",
        ".prettierrc.yaml",
        ".prettierrc.toml",
        "prettier.config.js",
        "prettier.config.cjs",
        "prettier.config.mjs",
    ];
    for cfg in prettier_configs {
        if project_root.join(cfg).exists() {
            return Some("prettier");
        }
    }

    None
}

pub fn edit_format(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let file_path = get_file_path(&input);

    let is_js_ts = file_path.ends_with(".ts")
        || file_path.ends_with(".tsx")
        || file_path.ends_with(".js")
        || file_path.ends_with(".jsx");

    if !is_js_ts || file_path.is_empty() {
        return HookResult::ok();
    }

    let resolved = fs::canonicalize(&file_path).unwrap_or_else(|_| file_path.clone().into());
    let Some(file_dir) = resolved.parent() else {
        return HookResult::ok();
    };

    let Some(project_root) = find_project_root(file_dir, "package.json") else {
        return HookResult::ok();
    };

    let Some(formatter) = detect_formatter(&project_root) else {
        return HookResult::ok();
    };

    let args: Vec<&str> = match formatter {
        "biome" => vec!["@biomejs/biome", "format", "--write", &file_path],
        "prettier" => vec!["prettier", "--write", &file_path],
        _ => return HookResult::ok(),
    };

    let _ = Command::new("npx")
        .args(&args)
        .current_dir(&project_root)
        .output();

    HookResult::ok()
}

// --- TypeScript check ---

pub fn edit_typecheck(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let file_path = get_file_path(&input);

    if !file_path.ends_with(".ts") && !file_path.ends_with(".tsx") {
        return HookResult::ok();
    }

    let resolved = match fs::canonicalize(&file_path) {
        Ok(p) => p,
        Err(_) => return HookResult::ok(),
    };

    let Some(file_dir) = resolved.parent() else {
        return HookResult::ok();
    };

    let Some(ts_root) = find_project_root(file_dir, "tsconfig.json") else {
        return HookResult::ok();
    };

    let result = Command::new("npx")
        .args(["tsc", "--noEmit", "--pretty", "false"])
        .current_dir(&ts_root)
        .output();

    if let Ok(output) = result {
        if !output.status.success() {
            let combined =
                String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);

            let rel_path = pathdiff(&resolved, &ts_root);
            let candidates: Vec<String> = vec![
                file_path.clone(),
                resolved.to_string_lossy().into_owned(),
                rel_path,
            ];

            let relevant: Vec<&str> = combined
                .lines()
                .filter(|line| candidates.iter().any(|c| line.contains(c.as_str())))
                .take(10)
                .collect();

            if !relevant.is_empty() {
                let basename = Path::new(&file_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(file_path.clone());
                warn(&format!("[Hook] TypeScript errors in {basename}:"));
                for line in relevant {
                    warn(line);
                }
            }
        }
    }

    HookResult::ok()
}

fn pathdiff(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

// --- Console.log warning after edit ---

pub fn edit_console_warn(raw: &str) -> HookResult {
    let input = parse_input(raw);
    let file_path = get_file_path(&input);

    let is_js_ts = file_path.ends_with(".ts")
        || file_path.ends_with(".tsx")
        || file_path.ends_with(".js")
        || file_path.ends_with(".jsx");

    if !is_js_ts || file_path.is_empty() {
        return HookResult::ok();
    }

    let Some(content) = read_file(Path::new(&file_path)) else {
        return HookResult::ok();
    };

    let matches: Vec<String> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("console.log"))
        .take(5)
        .map(|(i, line)| format!("{}: {}", i + 1, line.trim()))
        .collect();

    if !matches.is_empty() {
        warn(&format!("[Hook] WARNING: console.log found in {file_path}"));
        for m in &matches {
            warn(m);
        }
        warn("[Hook] Remove console.log before committing");
    }

    HookResult::ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn bash_json(cmd: &str) -> String {
        format!(r#"{{"tool_input":{{"command":"{cmd}"}}}}"#)
    }

    fn bash_json_with_output(cmd: &str, output: &str) -> String {
        let escaped = output.replace('"', r#"\""#);
        format!(
            r#"{{"tool_input":{{"command":"{cmd}"}},"tool_output":{{"output":"{escaped}"}}}}"#
        )
    }

    fn file_json(path: &str) -> String {
        format!(r#"{{"tool_input":{{"file_path":"{path}"}}}}"#)
    }

    // --- pr_created ---

    #[test]
    fn pr_created_detects_url() {
        let raw = bash_json_with_output(
            "gh pr create --title test",
            "https://github.com/user/repo/pull/42",
        );
        let r = pr_created(&raw);
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn pr_created_ignores_non_pr_commands() {
        let r = pr_created(&bash_json("git push"));
        assert_eq!(r.exit_code, 0);
    }

    // --- build_complete ---

    #[test]
    fn build_complete_fires_on_npm_build() {
        let r = build_complete(&bash_json("npm run build"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn build_complete_fires_on_pnpm_build() {
        let r = build_complete(&bash_json("pnpm build"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn build_complete_ignores_other() {
        let r = build_complete(&bash_json("cargo build"));
        assert_eq!(r.exit_code, 0);
    }

    // --- quality_gate ---

    #[test]
    fn quality_gate_ignores_missing_file() {
        let r = quality_gate(&file_json("/nonexistent/file.ts"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn quality_gate_ignores_empty_path() {
        let r = quality_gate(&file_json(""));
        assert_eq!(r.exit_code, 0);
    }

    // --- pathdiff ---

    #[test]
    fn pathdiff_strips_prefix() {
        assert_eq!(
            pathdiff(Path::new("/a/b/c.ts"), Path::new("/a/b")),
            "c.ts"
        );
    }

    #[test]
    fn pathdiff_no_common_prefix() {
        assert_eq!(
            pathdiff(Path::new("/x/y.ts"), Path::new("/a/b")),
            "/x/y.ts"
        );
    }

    // --- detect_formatter ---

    #[test]
    fn detect_formatter_biome() {
        let tmp = env::temp_dir().join("claude-hooks-test-df-biome");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::write(tmp.join("biome.json"), "{}");
        assert_eq!(detect_formatter(&tmp), Some("biome"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_formatter_prettier() {
        let tmp = env::temp_dir().join("claude-hooks-test-df-prettier");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::write(tmp.join(".prettierrc"), "{}");
        assert_eq!(detect_formatter(&tmp), Some("prettier"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_formatter_none() {
        let tmp = env::temp_dir().join("claude-hooks-test-df-none");
        let _ = std::fs::create_dir_all(&tmp);
        assert_eq!(detect_formatter(&tmp), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // --- edit_format ---

    #[test]
    fn edit_format_ignores_non_js_ts() {
        let r = edit_format(&file_json("/tmp/file.rs"));
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn edit_format_ignores_empty() {
        let r = edit_format(&file_json(""));
        assert_eq!(r.exit_code, 0);
    }

    // --- edit_typecheck ---

    #[test]
    fn edit_typecheck_ignores_non_ts() {
        let r = edit_typecheck(&file_json("/tmp/file.js"));
        assert_eq!(r.exit_code, 0);
    }

    // --- edit_console_warn ---

    #[test]
    fn console_warn_detects_console_log() {
        let tmp = env::temp_dir().join("claude-hooks-test-cw");
        let _ = std::fs::create_dir_all(&tmp);
        let file = tmp.join("test.ts");
        let _ = std::fs::write(&file, "const x = 1;\nconsole.log(x);\n");
        let r = edit_console_warn(&file_json(&file.to_string_lossy()));
        assert_eq!(r.exit_code, 0); // warns via stderr, doesn't block
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn console_warn_clean_file() {
        let tmp = env::temp_dir().join("claude-hooks-test-cw-clean");
        let _ = std::fs::create_dir_all(&tmp);
        let file = tmp.join("clean.ts");
        let _ = std::fs::write(&file, "const x = 1;\n");
        let r = edit_console_warn(&file_json(&file.to_string_lossy()));
        assert_eq!(r.exit_code, 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn console_warn_ignores_non_js_ts() {
        let r = edit_console_warn(&file_json("/tmp/file.py"));
        assert_eq!(r.exit_code, 0);
    }
}
