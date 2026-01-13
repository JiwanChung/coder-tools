# coder-tools

**A CLI toolkit for monitoring and managing AI coding assistant sessions.**

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Overview

`coder-tools` helps you manage multiple AI coding sessions across tmux panes:

- **Monitor** Claude, Gemini, and Codex sessions in real-time
- **Track** token usage and costs
- **Resume** previous Claude Code sessions
- **Sync** your `CLAUDE.md` guidelines across projects
- **Budget** token usage with daily/weekly/monthly limits

```
┌──────────────────────────────────────────────────────────────┐
│  Agent Monitor  | 5 sessions                                 │
│  >_ 2 waiting   ◐ 2 working   ⚠ 1 permission                │
├──────────────────────────────────────────────────────────────┤
│  >_ coder-tools [claude] base:1.0  ~/projects/               │
│     Waiting for input 5m23s  125.3k tokens  $0.42            │
│                                                              │
│  ◐  myapp [claude] base:2.1  ~/projects/                     │
│     Working 2m15s                                            │
│     > Add dark mode toggle to settings page                  │
│                                                              │
│  ⚠  api-server [gemini] base:3.0  ~/projects/                │
│     Permission required 30s                                  │
└──────────────────────────────────────────────────────────────┘
│  q quit   ↑↓ nav   ⏎ jump   y yes   $ cost   s stats        │
└──────────────────────────────────────────────────────────────┘
```

## Installation

```bash
# From source
git clone https://github.com/JiwanChung/coder-tools.git
cd coder-tools
cargo install --path .
```

### Requirements

- Rust 1.70+
- tmux
- macOS or Linux

## Quick Start

### 1. Configure Hooks

The monitor uses tmux pane options published by agent hooks. Add to your settings:

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
    "Stop": [{ "hooks": [{ "type": "command", "command": "tmux set -p @agent_status waiting 2>/dev/null" }] }],
    "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "tmux set -p @agent_status permission 2>/dev/null" }] }]
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

### 2. Run Monitor

```bash
coder-tools monitor
```

## Commands

### `monitor` — Real-time Dashboard

```bash
coder-tools monitor              # Default: 2s refresh
coder-tools monitor -a           # Show all panes
coder-tools monitor -n           # Enable notifications
coder-tools monitor -j           # Auto-jump to ready panes
```

**Keybindings:**
| Key | Action |
|-----|--------|
| `q` | Quit |
| `↑↓` / `jk` | Navigate |
| `Enter` | Jump to pane |
| `y` | Approve permission (sends 'y' + Enter) |
| `$` | Fetch token/cost data |
| `s` | Toggle stats view |
| `g` | Group by session |
| `w` / `i` | Filter working / waiting |
| `a` | Show all panes |
| `c` | Compact mode |

---

### `budget` — Token Usage Tracking

```bash
coder-tools budget status                    # Current usage
coder-tools budget set --daily 100k          # Set limits
coder-tools budget report                    # Detailed breakdown
```

---

### `resume` — Session History

```bash
coder-tools resume list          # List recent sessions
coder-tools resume show 1        # Show session details
```

---

### `sync` — CLAUDE.md Management

```bash
coder-tools sync push ~/projects/*    # Sync guidelines
coder-tools sync status ~/projects/*  # Check sync status
```

## How It Works

`coder-tools` is entirely local—no API calls:

- **Monitor**: Reads tmux pane options (`@agent_provider`, `@agent_status`) published by hooks
- **Cost**: Parses JSONL session logs from `~/.claude/projects/`
- **Budget**: Aggregates token usage from session logs

### Why Hooks?

Previous versions used screen scraping and process detection, which caused tmux server issues. Hook-based detection is:
- **Fast**: Single `tmux list-panes` call vs 40+ subprocess calls
- **Accurate**: Agents report their own state
- **Safe**: No risk of wedging tmux

## License

MIT License - see [LICENSE](LICENSE) for details.

---

<p align="center">
  <sub>Built for Claude Code, Gemini CLI, and Codex</sub>
</p>
