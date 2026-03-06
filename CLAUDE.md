# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A single Rust binary (`claude-hooks`) that implements Claude Code hooks — PreToolUse, PostToolUse, and Lifecycle event handlers. It reads JSON from stdin, dispatches to the appropriate handler based on a hook ID passed as the first CLI argument, and exits with a code (0 = allow, 2 = block). Output on stderr is shown to the user as warnings; stdout is passthrough unless suppressed.

## Build

```bash
cargo build --release
```

The release binary is optimized for size (`opt-level = "s"`, LTO, strip, single codegen unit). Requires Rust edition 2024.

## Architecture

**Single-binary dispatcher pattern.** `main.rs` reads stdin once, looks up the hook ID from argv[1], checks if the hook is enabled via the profile system, then dispatches to the matching handler function.

### Modules

- **`main.rs`** — Entry point, `HookResult` type (exit_code + suppress_passthrough), and `dispatch()` match table mapping hook IDs to handler functions.
- **`profile.rs`** — Three-tier profile system (`minimal`/`standard`/`strict`) controlled by `TEIMURJAN_HOOK_PROFILE` env var. Each hook declares which profiles it runs under. Individual hooks can be disabled via `TEIMURJAN_DISABLED_HOOKS` (comma-separated).
- **`pre_tool.rs`** — PreToolUse hooks: blocks dev servers outside tmux, tmux reminders, git push warnings, doc file path validation, compact suggestion counter.
- **`post_tool.rs`** — PostToolUse hooks: PR creation logging, build notifications, quality gate (biome/prettier/ruff/gofmt), auto-format on edit, TypeScript type checking, console.log detection.
- **`lifecycle.rs`** — Session lifecycle: session start (project detection, previous session loading), pre-compact state saving, console.log check on stop, session end (transcript parsing + summary persistence), session evaluation for learning, cost tracking to `~/.claude/metrics/costs.jsonl`.
- **`util.rs`** — Shared helpers: JSON parsing (`parse_input`, `get_command`, `get_file_path`), file I/O, local time via libc, git operations, project root detection.

### Key Patterns

- `HookResult::ok()` = pass-through, `HookResult::block()` = exit code 2 (blocks the tool), `HookResult::custom_output()` = suppresses stdin passthrough.
- All warnings go to stderr via `warn()` (eprintln). Stdout is reserved for passthrough of the original JSON.
- Hook input is always JSON with `tool_input.command` (Bash hooks) or `tool_input.file_path` (Edit/Write hooks).
- No async, no external runtime — pure synchronous Rust with minimal dependencies (`serde_json`, `regex`, `libc`).

### Environment Variables

| Variable | Purpose |
|---|---|
| `TEIMURJAN_HOOK_PROFILE` | `minimal` / `standard` (default) / `strict` |
| `TEIMURJAN_DISABLED_HOOKS` | Comma-separated hook IDs to disable |
| `TEIMURJAN_QUALITY_GATE_FIX` | `true` to auto-fix (write mode) |
| `TEIMURJAN_QUALITY_GATE_STRICT` | `true` to warn on formatter failures |
| `COMPACT_THRESHOLD` | Tool call count before suggesting /compact (default: 50) |
| `CLAUDE_SESSION_ID` | Used for session file naming and cost tracking |
| `CLAUDE_TRANSCRIPT_PATH` | Fallback path to session transcript for summary extraction |

## Adding a New Hook

1. Add the handler function in the appropriate module (`pre_tool.rs`, `post_tool.rs`, or `lifecycle.rs`).
2. Register the hook ID in `dispatch()` in `main.rs`.
3. Add the hook ID to the appropriate profile tier in `profile.rs` (`hook_allowed_profiles`).
