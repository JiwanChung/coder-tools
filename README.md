# coder-tools

**A CLI toolkit for monitoring and managing AI coding assistant sessions.**

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Overview

Run multiple AI coding agents (Claude, Gemini, Codex) in tmux and manage them from a single dashboard.

- **See all sessions at a glance** — which are working, waiting, or need permission
- **Jump to any pane** — press Enter to switch to the selected session
- **Approve permissions remotely** — press `y` to send approval without switching panes
- **Get notified** — desktop notifications when an agent finishes or needs attention
- **Track costs** — see token usage and estimated costs per session

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
git clone https://github.com/JiwanChung/coder-tools.git
cd coder-tools
cargo install --path .
```

### Requirements

- Rust 1.70+
- tmux
- macOS or Linux

## Quick Start

```bash
coder-tools monitor
```

On first run, it automatically configures Claude Code and Gemini CLI to report their status. Just run the command and start your agents in tmux panes.

## Commands

### `monitor` — Real-time Dashboard

```bash
coder-tools monitor              # Default: 2s refresh
coder-tools monitor -a           # Show all panes (including non-agent)
coder-tools monitor -n           # Enable desktop notifications
coder-tools monitor -j           # Auto-jump when an agent becomes ready
```

**Keybindings:**
| Key | Action |
|-----|--------|
| `q` | Quit |
| `↑↓` / `jk` | Navigate sessions |
| `Enter` | Jump to selected pane |
| `y` | Approve permission (sends 'y' + Enter) |
| `$` | Fetch token/cost data |
| `s` | Toggle stats view |
| `g` | Group by tmux session |
| `w` / `i` | Filter by working / waiting |
| `a` | Show all panes |
| `c` | Compact mode |

---

### `budget` — Token Usage Tracking

Set limits and track spending across all your sessions.

```bash
coder-tools budget status                    # Current usage
coder-tools budget set --daily 100k          # Set daily limit
coder-tools budget report                    # Detailed breakdown
```

---

### `resume` — Session History

List and restore previous Claude Code sessions.

```bash
coder-tools resume list          # List recent sessions
coder-tools resume show 1        # Show session details
```

---

### `sync` — CLAUDE.md Management

Keep your `CLAUDE.md` guidelines in sync across projects.

```bash
coder-tools sync push ~/projects/*    # Push to all projects
coder-tools sync status ~/projects/*  # Check sync status
```

## Supported Agents

| Agent | Status Detection | Cost Tracking |
|-------|------------------|---------------|
| Claude Code | Working, Waiting, Permission | Yes |
| Gemini CLI | Working, Waiting | No |
| Codex CLI | Via wrapper script | No |

## License

MIT License - see [LICENSE](LICENSE) for details.
