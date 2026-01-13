# Coder Tools

CLI utilities for monitoring and managing AI coding assistant sessions.

## Supported Providers

- **Claude Code** - Full support (status, tokens, cost)
- **Gemini CLI** - Status detection via hooks
- **OpenAI Codex** - Status detection via hooks/wrapper

## Commands

### coder-tools monitor

TUI dashboard for monitoring AI coding sessions across tmux panes.

**Features:**
- Real-time status detection (waiting, working, permission required)
- Token usage and cost tracking (press `$` to fetch)
- Auto-refresh with configurable interval
- Filter by status, group by session
- Keyboard navigation and quick actions

**Usage:**
```bash
coder-tools monitor              # Default: 2s refresh
coder-tools monitor -a           # Show all panes
coder-tools monitor -i 5         # Refresh every 5 seconds
coder-tools monitor -n           # Enable desktop notifications
coder-tools monitor --help       # Full options
```

**Keyboard shortcuts:**
- `q` - Quit
- `↑↓` / `jk` - Navigate
- `Enter` - Jump to pane
- `y` - Send 'y' + Enter (approve permission)
- `$` - Fetch token/cost data
- `s` - Toggle stats view
- `g` - Group by session
- `w` / `i` - Filter working / waiting
- `a` - Show all panes
- `c` - Compact mode

**Status indicators:**
- `>_` Green - Waiting for input
- `◐` Yellow - Working/thinking
- `⚠` Red - Permission required
- `--` Gray - Not an AI session

### coder-tools budget

Track and manage token usage budgets.

```bash
coder-tools budget status        # Show current usage
coder-tools budget set --daily 100k --monthly 10m
coder-tools budget report        # Detailed breakdown
coder-tools budget reset         # Clear counters
```

### coder-tools resume

List and restore previous Claude Code sessions.

### coder-tools sync

Sync CLAUDE.md files across projects.

## Hook-Based Detection

Detection uses tmux pane options published by agent hooks. No screen scraping or process detection.

### Setup

**Claude Code** (`~/.claude/settings.json`):
```json
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "bash -c 'TASK=$(jq -r \".prompt // empty\" | tr \"\\n\" \" \" | head -c 100); tmux set -p @agent_provider claude \\; set -p @agent_task \"$TASK\" \\; set -p @agent_status working 2>/dev/null'"
      }]
    }],
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": "tmux set -p @agent_status waiting 2>/dev/null || true"
      }]
    }],
    "PermissionRequest": [{
      "hooks": [{
        "type": "command",
        "command": "tmux set -p @agent_status permission 2>/dev/null || true"
      }]
    }]
  }
}
```

**Gemini CLI** (`~/.gemini/settings.json`):
```json
{
  "experiments": { "enableHooks": true },
  "hooks": {
    "BeforeAgent": [{ "hooks": [{ "type": "command", "command": "tmux set -p @agent_provider gemini \\; set -p @agent_status working 2>/dev/null" }] }],
    "AfterAgent": [{ "hooks": [{ "type": "command", "command": "tmux set -p @agent_status waiting 2>/dev/null" }] }]
  }
}
```

**Codex CLI**: Use wrapper script (`~/.local/bin/codex-wrapper`).

### tmux Pane Options

| Option | Values | Description |
|--------|--------|-------------|
| `@agent_provider` | `claude`, `gemini`, `codex` | Which agent |
| `@agent_status` | `working`, `waiting`, `permission` | Current state |
| `@agent_task` | User's prompt (truncated) | What it's working on |

## Project Structure

```
coder-tools/
├── src/
│   ├── main.rs           # CLI args, event loop, terminal setup
│   ├── app.rs            # Application state management
│   ├── detector.rs       # Status detection from pane options
│   ├── cost.rs           # Token counting and cost calculation
│   ├── tmux.rs           # tmux interaction
│   ├── ui.rs             # TUI rendering
│   ├── budget.rs         # Budget tracking
│   ├── resume.rs         # Session restoration
│   ├── sync.rs           # CLAUDE.md syncing
│   └── notify.rs         # Desktop notifications
├── Cargo.toml
└── CLAUDE.md
```

## Cost Tracking

Reads Claude session files at `~/.claude/projects/{path_hash}/*.jsonl`.

Path hash: Replace `/` and `_` with `-` (e.g., `/Users/foo/my_project` → `-Users-foo-my-project`)

Pricing (Sonnet 3.5):
- Input: $3/M tokens
- Output: $15/M tokens
- Cache read: $0.30/M tokens
- Cache write: $3.75/M tokens

## Development

```bash
cargo build --release
cargo install --path .
cargo test
```

## Dependencies

- `ratatui` + `crossterm` - TUI framework
- `clap` - CLI argument parsing
- `anyhow` - Error handling
- `serde` / `serde_json` - Serialization
- `dirs` - Home directory detection
