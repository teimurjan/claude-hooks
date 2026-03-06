mod lifecycle;
mod post_tool;
mod pre_tool;
mod profile;
mod util;

use std::io::Read;

const MAX_STDIN: usize = 1024 * 1024;

pub struct HookResult {
    pub exit_code: i32,
    pub suppress_passthrough: bool,
}

impl HookResult {
    pub fn ok() -> Self {
        Self {
            exit_code: 0,
            suppress_passthrough: false,
        }
    }

    pub fn block() -> Self {
        Self {
            exit_code: 2,
            suppress_passthrough: false,
        }
    }

    pub fn custom_output() -> Self {
        Self {
            exit_code: 0,
            suppress_passthrough: true,
        }
    }
}

fn read_stdin() -> String {
    let mut buf = Vec::with_capacity(8192);
    let _ = std::io::stdin()
        .lock()
        .take(MAX_STDIN as u64)
        .read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn main() {
    let raw = read_stdin();

    let Some(hook_id) = std::env::args().nth(1) else {
        print!("{raw}");
        std::process::exit(0);
    };

    if !profile::is_hook_enabled(&hook_id) {
        print!("{raw}");
        std::process::exit(0);
    }

    let result = dispatch(&hook_id, &raw);

    if !result.suppress_passthrough {
        print!("{raw}");
    }

    std::process::exit(result.exit_code);
}

fn dispatch(hook_id: &str, raw: &str) -> HookResult {
    match hook_id {
        // PreToolUse
        "pre-bash-dev-server-block" => pre_tool::dev_server_block(raw),
        "pre-bash-tmux-reminder" => pre_tool::tmux_reminder(raw),
        "pre-bash-git-push-reminder" => pre_tool::git_push_reminder(raw),
        "doc-file-warning" => pre_tool::doc_file_warning(raw),
        "suggest-compact" => pre_tool::suggest_compact(raw),

        // PostToolUse
        "post-bash-pr-created" => post_tool::pr_created(raw),
        "post-bash-build-complete" => post_tool::build_complete(raw),
        "quality-gate" => post_tool::quality_gate(raw),
        "post-edit-format" => post_tool::edit_format(raw),
        "post-edit-typecheck" => post_tool::edit_typecheck(raw),
        "post-edit-console-warn" => post_tool::edit_console_warn(raw),

        // Lifecycle
        "session-start" => lifecycle::session_start(raw),
        "pre-compact" => lifecycle::pre_compact(raw),
        "check-console-log" => lifecycle::check_console_log(raw),
        "session-end" => lifecycle::session_end(raw),
        "evaluate-session" => lifecycle::evaluate_session(raw),
        "cost-tracker" => lifecycle::cost_tracker(raw),
        "session-end-marker" => HookResult::ok(),

        _ => HookResult::ok(),
    }
}
