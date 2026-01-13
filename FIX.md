# TMux Server Wedging - Analysis and Fixes

## Problem Summary

The monitor tool can wedge the tmux server by issuing too many synchronous commands without timeouts, causing command pile-up when any single command is slow.

---

## Recommended Redesign: Agent-Published Status

**Philosophy:** Stop scraping pane output. Make the agent publish status explicitly.

### How it works

**Agent side (Claude Code hook or wrapper):**
```bash
# Agent publishes its status to a tmux pane option
tmux set -p @agent_status "working"
tmux set -p @agent_status "waiting"
tmux set -p @agent_status "permission"

# Optional: heartbeat with timestamp
tmux set -p @agent_heartbeat "$(date +%s)"
```

**Monitor side:**
```bash
# Single cheap call gets everything
tmux list-panes -a -F '#{pane_id}\t#{session_name}\t#{window_index}\t#{pane_index}\t#{@agent_status}\t#{@agent_heartbeat}'
```

### Why this is better

| Aspect | Current (Pull/Scrape) | Proposed (Push/Publish) |
|--------|----------------------|------------------------|
| Commands per refresh | 1 + 4N (N = panes) | 1 |
| Risk of wedging | High | Near-zero |
| Latency | Seconds | Milliseconds |
| Accuracy | Heuristic (screen scraping) | Exact (agent knows its state) |
| Dependencies | lsof, ps, file reads | None |

### Hook Support by Provider

All three major coding CLIs support hooks. User configures hooks in settings - agent publishes status automatically.

**We track turns, not tools.** One status change when user submits, one when agent finishes.

---

#### Claude Code Hooks

**Config:** `~/.claude/settings.json`

**Events:**
| Event | When | Status |
|-------|------|--------|
| `UserPromptSubmit` | User submits prompt | "working" + capture task |
| `Stop` | Agent finishes | "waiting" |
| `PermissionRequest` | Needs approval | "permission" |

**Example config:**
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

**Docs:** https://code.claude.com/docs/en/hooks

---

#### Gemini CLI Hooks

**Config:** `~/.gemini/settings.json`

**IMPORTANT:** Hooks are disabled by default. You must enable them:
```json
{
  "experiments": {
    "enableHooks": true
  }
}
```

**Events:**
| Event | When | Status |
|-------|------|--------|
| `BeforeAgent` | User submits prompt | "working" + capture task |
| `AfterAgent` | Agent finishes | "waiting" |
| `Notification` | Needs approval | "permission" |

**Example config:**
```json
{
  "experiments": {
    "enableHooks": true
  },
  "hooks": {
    "BeforeAgent": [{
      "hooks": [{
        "type": "command",
        "command": "bash -c 'TASK=$(jq -r \".prompt // empty\" | tr \"\\n\" \" \" | head -c 100); tmux set -p @agent_provider gemini \\; set -p @agent_task \"$TASK\" \\; set -p @agent_status working 2>/dev/null'"
      }]
    }],
    "AfterAgent": [{
      "hooks": [{
        "type": "command",
        "command": "tmux set -p @agent_status waiting 2>/dev/null || true"
      }]
    }],
    "Notification": [{
      "hooks": [{
        "type": "command",
        "command": "tmux set -p @agent_status permission 2>/dev/null || true"
      }]
    }]
  }
}
```

**Docs:** https://geminicli.com/docs/hooks/

---

#### OpenAI Codex CLI Hooks

**Config:** `~/.codex/config.toml` + wrapper script

**Events:**
| Event | When | Status |
|-------|------|--------|
| Wrapper start | User runs codex | "working" |
| `agent-turn-complete` | Agent finishes | "waiting" |

**config.toml:**
```toml
notify = ["bash", "-c", "tmux set -p @agent_status waiting"]
```

**Wrapper script (codex-monitor):**
```bash
#!/bin/bash
# Capture task from first argument or prompt
TASK="${1:-}"
[ -n "$TASK" ] && tmux set -p @agent_task "$TASK" 2>/dev/null
tmux set -p @agent_status working 2>/dev/null
trap 'tmux set -p @agent_status "" @agent_task "" 2>/dev/null' EXIT
codex "$@"
```

**Docs:** https://developers.openai.com/codex/config-advanced/

---

### Unified tmux pane options

All providers publish to the same options for unified monitoring:

| Option | Values | Description |
|--------|--------|-------------|
| `@agent_provider` | `claude`, `gemini`, `codex` | Which coding agent |
| `@agent_status` | `working`, `waiting`, `permission`, `""` | Current state |
| `@agent_task` | User's prompt (truncated to 100 chars) | What the agent is working on |

**Removed:**
- `@agent_heartbeat` - not needed (status updates imply liveness)

### Cost/Token Tracking

Cost tracking still requires reading session files (no hook publishes this data).

**Options:**

A. **Keep file-based cost tracking, but make it lazy:**
   - Only read JSONL when user requests cost info (e.g., press 'c' to fetch)
   - Don't poll files every refresh cycle

B. **Hook publishes cost (requires CLI support):**
   - Hooks would need to emit cost data (not currently supported)
   - Future enhancement

C. **Drop cost tracking from monitor:**
   - Simplest solution
   - Users check cost via `claude --usage` or similar

**Recommendation:** Option A - lazy fetch on demand.

---

### New Monitor Implementation (Sketch)

```rust
const PANE_FORMAT: &str = "#{pane_id}\t#{session_name}\t#{window_index}\t#{pane_index}\t#{pane_current_path}\t#{@agent_provider}\t#{@agent_status}\t#{@agent_task}";

pub fn refresh(&mut self) -> Result<()> {
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", PANE_FORMAT])
        .output()?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        let provider = parts.get(5).map(|s| s.trim().to_string());
        let status = match parts.get(6).map(|s| s.trim()) {
            Some("working") => Status::Working,
            Some("waiting") => Status::WaitingForInput,
            Some("permission") => Status::PermissionRequired,
            _ => Status::NotDetected,
        };
        let task = parts.get(7).map(|s| s.to_string());
        // Update pane state with provider + status + task...
    }
    Ok(())
}
```

**What gets deleted:**
- `src/providers/claude.rs` - process detection, file parsing (~500 lines)
- `src/providers/gemini.rs`, `src/providers/openai.rs` - stub implementations
- `src/detector.rs` - screen scraping logic (~300 lines)
- `tmux::capture_pane()` - no longer needed
- All `lsof`, `ps` subprocess calls
- JSONL/debug file parsing

**What remains:**
- `tmux::list_panes()` - still needed (but now includes status/task)
- `tmux::switch_to_pane()` - for navigation
- `tmux::send_keys()` - for 'y' to approve
- UI rendering code

---

### Migration path

1. Add `@agent_status` reading to monitor (backwards compatible)
2. Document hook setup for each provider
3. Users configure hooks in their settings
4. Deprecate screen scraping fallback
5. Remove lsof/ps/capture-pane code

---

## Legacy Issues (resolved by redesign)

The issues below are fixed by the redesign above. Documented for reference if incremental fixes are preferred.

## Issue 1: `lsof` is extremely slow

**Location:** `src/providers/claude.rs:151-165`

```rust
fn get_process_cwd(pid: u32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-p", &pid.to_string()])
        .output()
        .ok()?;
```

**Problem:**
- `lsof -p {pid}` lists ALL open files for a process
- Claude processes can have hundreds/thousands of open files
- Takes 1-5+ seconds per call
- Called for every pane on every refresh cycle (default: every 2 seconds)

**Solution:**
Use `pwdx` on Linux or `lsof -a -d cwd` to get only the cwd, not all files:
```rust
// macOS: use lsof with filters
Command::new("lsof")
    .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-F", "n"])
    .output()

// Linux alternative: pwdx or /proc/{pid}/cwd
```

---

## Issue 2: No timeouts on subprocess commands

**Location:** `src/tmux.rs`, `src/providers/claude.rs`

**Problem:**
- All `Command::new().output()` calls are blocking with no timeout
- If tmux is slow or `lsof` hangs, the entire refresh blocks
- Commands pile up, eventually overwhelming tmux server

**Solution:**
Add timeouts to all external commands:
```rust
use std::time::Duration;
use std::process::Stdio;

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => { /* collect output */ }
        Ok(None) => { child.kill().ok(); None }  // timed out
        Err(_) => None
    }
}
```

Or use tokio with async timeout for cleaner handling.

---

## Issue 3: Debug output spam

**Location:** `src/providers/mod.rs:154-156`

```rust
eprintln!("[DETECT] TTY={} title={:?} provider={:?} detected={}",
    tty, pane_title.chars().take(30).collect::<String>(),
    provider.kind(), detected);
```

**Problem:**
- Prints for every provider check (3 providers) for every pane on every refresh
- With 10 panes: 30 eprintln calls every 2 seconds
- Adds I/O overhead and noise

**Solution:**
Remove or gate behind a `--debug` flag:
```rust
#[cfg(debug_assertions)]
eprintln!("[DETECT] ...");

// Or use a debug flag
if self.debug_mode {
    eprintln!("[DETECT] ...");
}
```

---

## Issue 4: Synchronous command storm

**Location:** `src/app.rs:93-209` (refresh function)

**Problem:**
Per refresh cycle with N panes:
- 1× `tmux list-panes -a`
- N× `tmux capture-pane`
- N× `ps -t {tty}`
- N× `lsof -p {pid}`
- N× file reads

All synchronous. 10 panes = 40+ blocking calls every 2 seconds.

**Solution Options:**

A. **Parallelize pane processing:**
```rust
use rayon::prelude::*;
panes.par_iter().map(|pane| process_pane(pane)).collect()
```

B. **Stagger expensive operations:**
- Only run `lsof`/process detection on a subset of panes per cycle
- Cache process CWD with TTL (it rarely changes)

C. **Batch tmux operations:**
```bash
# Instead of N capture-pane calls, use a single script
tmux list-panes -a -F '#{pane_id}' | while read id; do
    tmux capture-pane -t "$id" -p
done
```

---

## Issue 5: No caching of stable data

**Location:** `src/providers/claude.rs`

**Problem:**
- Process CWD rarely changes but is fetched every cycle
- Session file paths are recalculated every cycle
- JSONL file discovery happens every cycle

**Solution:**
Cache with TTL:
```rust
struct CachedCwd {
    cwd: String,
    fetched_at: Instant,
}

impl ClaudeProvider {
    fn get_cwd_cached(&self, pid: u32) -> Option<String> {
        let cache = self.cwd_cache.lock().unwrap();
        if let Some(cached) = cache.get(&pid) {
            if cached.fetched_at.elapsed() < Duration::from_secs(30) {
                return Some(cached.cwd.clone());
            }
        }
        // Fetch and cache
    }
}
```

---

## Priority Order

**Recommended:** Implement the hook-based redesign (eliminates all issues above).

**If incremental fixes preferred:**
1. **High:** Fix `lsof` to use `-d cwd` filter (immediate 10x speedup)
2. **High:** Add timeouts to all Command calls (prevent hangs)
3. **Medium:** Remove debug eprintln or gate behind flag
4. **Medium:** Cache process CWD with 30s TTL
5. **Low:** Parallelize pane processing
