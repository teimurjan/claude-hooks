# claude-hooks

A single Rust binary that implements [Claude Code hooks](https://docs.anthropic.com/en/docs/claude-code/hooks) — PreToolUse, PostToolUse, and Lifecycle event handlers for enforcing workflow guardrails, code quality gates, and session persistence.

## Build

```bash
cargo build --release
```

The binary lands at `target/release/claude-hooks`. It's optimized for size (~600KB stripped with LTO).

## Setup

Add the hooks to `~/.claude/settings.json`. Each hook is invoked as:

```
claude-hooks <hook-id>
```

It reads JSON from stdin (piped by Claude Code), writes warnings to stderr, and exits with code 0 (allow) or 2 (block).

### Full settings.json example

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks pre-bash-dev-server-block"
          }
        ]
      },
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks pre-bash-tmux-reminder"
          }
        ]
      },
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks pre-bash-git-push-reminder"
          }
        ]
      },
      {
        "matcher": "Write",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks doc-file-warning"
          }
        ]
      },
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks suggest-compact"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks post-bash-pr-created"
          }
        ]
      },
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks post-bash-build-complete"
          }
        ]
      },
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks quality-gate",
            "timeout": 30,
            "async": true
          }
        ]
      },
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks post-edit-format"
          }
        ]
      },
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks post-edit-typecheck"
          }
        ]
      },
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks post-edit-console-warn"
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks session-start"
          }
        ]
      }
    ],
    "PreCompact": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks pre-compact"
          }
        ]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks check-console-log"
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks session-end",
            "timeout": 10,
            "async": true
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks evaluate-session",
            "timeout": 10,
            "async": true
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks cost-tracker",
            "timeout": 10,
            "async": true
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/.claude/teimurjan@claude-hooks/target/release/claude-hooks session-end-marker"
          }
        ]
      }
    ]
  }
}
```

You can cherry-pick individual hooks — each one is independent.

## Hooks

### PreToolUse

| Hook ID | Matcher | What it does |
|---|---|---|
| `pre-bash-dev-server-block` | `Bash` | **Blocks** dev servers (`npm run dev`, `pnpm dev`, etc.) unless wrapped in tmux |
| `pre-bash-tmux-reminder` | `Bash` | Warns when running long commands outside tmux (strict-only) |
| `pre-bash-git-push-reminder` | `Bash` | Warns before `git push` (strict-only) |
| `doc-file-warning` | `Write` | Warns when creating non-standard doc files (outside `docs/`, `README.md`, etc.) |
| `suggest-compact` | `Edit\|Write` | Suggests `/compact` after a configurable number of tool calls |

### PostToolUse

| Hook ID | Matcher | What it does |
|---|---|---|
| `post-bash-pr-created` | `Bash` | Logs PR URL after `gh pr create` |
| `post-bash-build-complete` | `Bash` | Notifies when build commands finish |
| `quality-gate` | `Edit\|Write` | Runs biome/prettier/ruff/gofmt on edited files |
| `post-edit-format` | `Edit` | Auto-formats JS/TS files with detected formatter (biome or prettier) |
| `post-edit-typecheck` | `Edit` | Runs `tsc --noEmit` and shows errors for the edited file |
| `post-edit-console-warn` | `Edit` | Warns about `console.log` statements in edited JS/TS files |

### Lifecycle

| Hook ID | Event | What it does |
|---|---|---|
| `session-start` | `SessionStart` | Loads previous session state, detects project type and package manager |
| `pre-compact` | `PreCompact` | Saves state before context compaction |
| `check-console-log` | `Stop` | Scans git-modified JS/TS files for leftover `console.log` |
| `session-end` | `Stop` | Parses transcript and persists session summary to `~/.claude/sessions/` |
| `evaluate-session` | `Stop` | Flags long sessions for pattern extraction |
| `cost-tracker` | `Stop` | Appends token usage to `~/.claude/metrics/costs.jsonl` |

## Profiles

Control which hooks run via `TEIMURJAN_HOOK_PROFILE`:

| Profile | Hooks enabled |
|---|---|
| `minimal` | Lifecycle only (session-start, session-end, cost-tracker, pre-compact) |
| `standard` (default) | All except tmux-reminder and git-push-reminder |
| `strict` | Everything |

Disable individual hooks with `TEIMURJAN_DISABLED_HOOKS` (comma-separated):

```bash
export TEIMURJAN_DISABLED_HOOKS="suggest-compact,post-edit-typecheck"
```

## Tests

```bash
cargo test
```

95 tests: 82 unit tests covering pure logic in each module + 13 integration tests exercising the binary end-to-end with stdin/stdout/stderr/exit-code assertions.
