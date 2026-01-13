# Coder Tools

CLI utilities for monitoring and managing AI coding assistant sessions.

## Supported Providers

- **Claude Code** - Full support (status, tokens, cost)
- **OpenAI/ChatGPT CLI** - Planned
- **Gemini** - Planned

## Tools

### coder-tools monitor

TUI dashboard for monitoring AI coding sessions across tmux panes.

**Features:**
- Real-time status detection (waiting, working, permission required, idle)
- Token usage and cost tracking per session (USD)
- Auto-refresh with configurable interval
- Filter by provider or show all panes
- Keyboard navigation

**Usage:**
```bash
coder-tools monitor              # Default: 2s refresh
coder-tools monitor -a           # Show all panes
coder-tools monitor -i 5         # Refresh every 5 seconds
coder-tools monitor --help       # Full options
```

**Status indicators:**
- `>_` Green - Waiting for input
- `◐` Yellow - Working/thinking
- `⚠` Red - Permission required
- `--` Gray - Not an AI session

## Project Structure

```
coder-tools/
├── src/
│   ├── main.rs           # CLI args, event loop, terminal setup
│   ├── app.rs            # Application state management
│   ├── providers/        # Provider-specific detection (planned)
│   │   ├── mod.rs
│   │   ├── claude.rs     # Claude Code detection
│   │   ├── openai.rs     # OpenAI CLI detection
│   │   └── gemini.rs     # Gemini detection
│   ├── detector.rs       # Provider-agnostic status detection
│   ├── pricing.rs        # Token pricing (multi-provider)
│   ├── tmux.rs           # tmux interaction
│   └── ui.rs             # TUI rendering
├── Cargo.toml
└── CLAUDE.md
```

## Provider Detection

Each provider has unique signals:

### Claude Code
- Pane title contains `✳` marker
- Session files at `~/.claude/projects/{path}/*.jsonl`
- Debug logs at `~/.claude/debug/{session}.txt`

### OpenAI CLI (planned)
- Process name detection
- Config at `~/.config/openai/` or similar

### Gemini (planned)
- Process name detection
- Config at `~/.config/gemini/` or similar

## Cost Tracking

Pricing sources:
- **Claude**: [Anthropic Pricing](https://platform.claude.com/docs/en/about-claude/pricing)
- **OpenAI**: [OpenAI Pricing](https://openai.com/pricing) (planned)
- **Gemini**: [Google AI Pricing](https://ai.google.dev/pricing) (planned)

## Development

### Build & Run
```bash
cargo build --release
cargo install --path .
```

### Testing
```bash
cargo test
cargo clippy
cargo fmt
```

### Adding a New Provider

1. Create `src/providers/{name}.rs`
2. Implement the `Provider` trait
3. Add pricing to `src/pricing.rs`
4. Register in `src/providers/mod.rs`

## Dependencies

- `ratatui` + `crossterm` - TUI framework
- `clap` - CLI argument parsing
- `anyhow` - Error handling
- `tokio` - Async runtime
- `serde` / `serde_json` - Serialization
