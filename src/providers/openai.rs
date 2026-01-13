//! OpenAI Codex CLI provider implementation

use super::{Provider, ProviderKind, SessionInfo, SessionStatus};
use crate::pricing::SessionCost;
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;

pub struct OpenAIProvider;

impl OpenAIProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for OpenAIProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
    }

    fn detect(&self, tty: &str, pane_title: &str, content: &str) -> bool {
        // Check pane title for Codex marker
        if pane_title.contains("codex") || pane_title.contains("Codex") {
            return true;
        }

        // Check if Codex process is running on this TTY
        if find_codex_pid_by_tty(tty).is_some() {
            return true;
        }

        // Fallback: check screen content for Codex UI elements
        is_codex_session(content)
    }

    fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo {
        let mut info = SessionInfo {
            provider: ProviderKind::OpenAI,
            ..Default::default()
        };

        // Extract task from pane title
        info.task = extract_task_from_title(pane_title);

        // Try to get detailed info from session files
        if let Some(raw_info) = get_session_info_by_tty(tty) {
            info.status = detect_status_from_signals(&raw_info, content);

            info.detail = if info.status == SessionStatus::Working {
                match (&raw_info.current_tool, &raw_info.tool_detail) {
                    (Some(tool), Some(detail)) => Some(format!("{}: {}", tool, detail)),
                    (Some(tool), None) => Some(tool.clone()),
                    _ => raw_info.last_user_prompt.clone(),
                }
            } else if info.status == SessionStatus::PermissionRequired {
                extract_permission_detail(content)
            } else {
                raw_info.last_user_prompt.clone()
            };

            info.last_user_prompt = raw_info.last_user_prompt;
            info.current_tool = raw_info.current_tool;
            info.tool_detail = raw_info.tool_detail;
            info.model = raw_info.model;
            info.cost = raw_info.session_cost;
        } else {
            // Fallback to content-based detection
            info.status = detect_status_from_content(content);
            if info.status == SessionStatus::PermissionRequired {
                info.detail = extract_permission_detail(content);
            }
        }

        info
    }
}

// ============================================================================
// Codex-specific detection helpers
// ============================================================================

#[derive(Debug, Clone, Default)]
struct RawSessionInfo {
    #[allow(dead_code)]
    session_id: Option<String>,
    #[allow(dead_code)]
    cwd: Option<String>,
    last_user_prompt: Option<String>,
    current_tool: Option<String>,
    tool_detail: Option<String>,
    model: Option<String>,
    is_streaming: bool,
    session_cost: Option<SessionCost>,
}

/// Find Codex process PID by TTY
fn find_codex_pid_by_tty(tty: &str) -> Option<u32> {
    let tty_name = tty.trim_start_matches("/dev/");

    let output = Command::new("ps")
        .args(["-t", tty_name, "-o", "pid=,comm="])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "codex" {
            return parts[0].parse().ok();
        }
    }
    None
}

/// Get the current working directory of a process
fn get_process_cwd(pid: u32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-p", &pid.to_string()])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("cwd") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                return Some(parts[8..].join(" "));
            }
        }
    }
    None
}

fn codex_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("sessions"))
}

fn codex_history_file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("history.jsonl"))
}

/// Find the most recent session JSONL file
fn find_latest_session_jsonl() -> Option<PathBuf> {
    let sessions_dir = codex_sessions_dir()?;

    // Navigate year/month/day structure to find most recent
    let mut all_sessions: Vec<PathBuf> = Vec::new();

    if let Ok(years) = fs::read_dir(&sessions_dir) {
        for year_entry in years.filter_map(|e| e.ok()) {
            if let Ok(months) = fs::read_dir(year_entry.path()) {
                for month_entry in months.filter_map(|e| e.ok()) {
                    if let Ok(days) = fs::read_dir(month_entry.path()) {
                        for day_entry in days.filter_map(|e| e.ok()) {
                            if let Ok(files) = fs::read_dir(day_entry.path()) {
                                for file in files.filter_map(|e| e.ok()) {
                                    let path = file.path();
                                    if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                                        all_sessions.push(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by modification time and return most recent
    all_sessions.sort_by_key(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    all_sessions.pop()
}

#[derive(Deserialize)]
struct CodexSessionMeta {
    id: Option<String>,
    cwd: Option<String>,
    model_provider: Option<String>,
}

#[derive(Deserialize)]
struct CodexEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    #[allow(dead_code)]
    timestamp: Option<String>,
    payload: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct CodexTokenInfo {
    total_token_usage: Option<TokenUsage>,
}

#[derive(Deserialize)]
struct TokenUsage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
}

fn parse_session_jsonl(jsonl_path: &PathBuf) -> Option<RawSessionInfo> {
    let file = fs::File::open(jsonl_path).ok()?;
    let file_size = file.metadata().ok()?.len();

    let mut reader = BufReader::new(file);
    let start_pos = file_size.saturating_sub(150_000);
    reader.seek(SeekFrom::Start(start_pos)).ok()?;

    if start_pos > 0 {
        let mut skip = String::new();
        reader.read_line(&mut skip).ok()?;
    }

    let mut info = RawSessionInfo::default();
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut total_cached: u64 = 0;

    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

    // Parse session meta first
    for line in &lines {
        if let Ok(entry) = serde_json::from_str::<CodexEntry>(line) {
            if entry.entry_type.as_deref() == Some("session_meta") {
                if let Some(payload) = entry.payload {
                    if let Ok(meta) = serde_json::from_value::<CodexSessionMeta>(payload) {
                        info.session_id = meta.id;
                        info.cwd = meta.cwd;
                        if meta.model_provider.as_deref() == Some("openai") {
                            info.model = Some("gpt-4o".to_string()); // Default model
                        }
                    }
                }
            }
        }
    }

    // Parse from end for latest state
    for line in lines.iter().rev() {
        if let Ok(entry) = serde_json::from_str::<CodexEntry>(line) {
            match entry.entry_type.as_deref() {
                Some("token_count") => {
                    if let Some(payload) = entry.payload {
                        if let Some(info_obj) = payload.get("info") {
                            if let Ok(token_info) = serde_json::from_value::<CodexTokenInfo>(info_obj.clone()) {
                                if let Some(usage) = token_info.total_token_usage {
                                    total_input = usage.input_tokens.unwrap_or(0);
                                    total_output = usage.output_tokens.unwrap_or(0) + usage.reasoning_output_tokens.unwrap_or(0);
                                    total_cached = usage.cached_input_tokens.unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                Some("user_message") | Some("response_item") => {
                    if info.last_user_prompt.is_none() {
                        if let Some(payload) = &entry.payload {
                            if payload.get("role").and_then(|r| r.as_str()) == Some("user") {
                                if let Some(content) = payload.get("content") {
                                    if let Some(arr) = content.as_array() {
                                        for item in arr {
                                            if item.get("type").and_then(|t| t.as_str()) == Some("input_text") {
                                                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                                    if !text.starts_with('<') && !text.starts_with('#') {
                                                        info.last_user_prompt = Some(text.chars().take(150).collect());
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some("function_call") | Some("custom_tool_call") => {
                    if info.current_tool.is_none() {
                        if let Some(payload) = &entry.payload {
                            if let Some(name) = payload.get("name").and_then(|n| n.as_str()) {
                                info.current_tool = Some(name.to_string());
                                if let Some(args) = payload.get("arguments").or(payload.get("args")) {
                                    info.tool_detail = extract_tool_detail(name, args);
                                }
                            }
                        }
                    }
                }
                Some("event_msg") => {
                    if let Some(payload) = &entry.payload {
                        if payload.get("type").and_then(|t| t.as_str()) == Some("agent_reasoning") {
                            info.is_streaming = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if info.last_user_prompt.is_some() && info.current_tool.is_some() && total_input > 0 {
            break;
        }
    }

    // Calculate cost (using GPT-4o pricing: $2.50/1M input, $10/1M output, $1.25/1M cached)
    if total_input > 0 || total_output > 0 {
        let input_cost = (total_input.saturating_sub(total_cached) as f64) * 2.50 / 1_000_000.0;
        let cached_cost = (total_cached as f64) * 1.25 / 1_000_000.0;
        let output_cost = (total_output as f64) * 10.0 / 1_000_000.0;

        info.session_cost = Some(SessionCost {
            model: info.model.clone().unwrap_or_else(|| "gpt-4o".to_string()),
            usage: crate::pricing::TokenUsage {
                input_tokens: total_input,
                output_tokens: total_output,
                cache_creation_5m_tokens: 0,
                cache_creation_1h_tokens: 0,
                cache_read_tokens: total_cached,
            },
            cost_usd: input_cost + cached_cost + output_cost,
        });
    }

    Some(info)
}

fn extract_tool_detail(name: &str, args: &serde_json::Value) -> Option<String> {
    match name {
        "shell" | "bash" => args
            .get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.lines().next().unwrap_or("").chars().take(60).collect()),
        "read_file" | "write_file" | "edit_file" => args
            .get("file_path").or(args.get("path"))
            .and_then(|p| p.as_str())
            .and_then(|p| p.rsplit('/').next())
            .map(String::from),
        "grep" | "search" => args
            .get("pattern").or(args.get("query"))
            .and_then(|p| p.as_str())
            .map(|p| p.chars().take(40).collect()),
        _ => None,
    }
}

fn get_session_info_by_tty(tty: &str) -> Option<RawSessionInfo> {
    let pid = find_codex_pid_by_tty(tty)?;
    let cwd = get_process_cwd(pid);

    // Find the most recent session file
    let jsonl_path = find_latest_session_jsonl()?;

    let mut info = parse_session_jsonl(&jsonl_path)?;
    if cwd.is_some() {
        info.cwd = cwd;
    }

    // Try to get last prompt from history if not found
    if info.last_user_prompt.is_none() {
        info.last_user_prompt = get_last_prompt_from_history();
    }

    Some(info)
}

fn get_last_prompt_from_history() -> Option<String> {
    let history_file = codex_history_file()?;
    let file = fs::File::open(&history_file).ok()?;
    let file_size = file.metadata().ok()?.len();

    let mut reader = BufReader::new(file);
    let start_pos = file_size.saturating_sub(50_000);
    reader.seek(SeekFrom::Start(start_pos)).ok()?;

    if start_pos > 0 {
        let mut skip = String::new();
        reader.read_line(&mut skip).ok()?;
    }

    #[derive(Deserialize)]
    struct HistoryEntry {
        text: Option<String>,
    }

    let mut last_prompt = None;
    for line in reader.lines().flatten() {
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
            if let Some(text) = entry.text {
                if !text.starts_with('<') {
                    last_prompt = Some(text.chars().take(150).collect());
                }
            }
        }
    }

    last_prompt
}

// ============================================================================
// Content-based detection (fallback)
// ============================================================================

fn is_codex_session(content: &str) -> bool {
    let indicators = [
        "codex>",
        "Codex CLI",
        "openai/codex",
        "── Codex",
        "sandbox-exec",  // Codex sandbox indicator
    ];
    indicators.iter().any(|i| content.contains(i))
}

fn detect_status_from_signals(info: &RawSessionInfo, content: &str) -> SessionStatus {
    if info.is_streaming {
        return SessionStatus::Working;
    }

    if is_permission_prompt(content) {
        return SessionStatus::PermissionRequired;
    }

    // Check for working indicators in content
    if content.contains("Running") || content.contains("Thinking") {
        return SessionStatus::Working;
    }

    SessionStatus::WaitingForInput
}

fn detect_status_from_content(content: &str) -> SessionStatus {
    if content.contains("Running") || content.contains("Thinking") || content.contains("...") {
        SessionStatus::Working
    } else if is_permission_prompt(content) {
        SessionStatus::PermissionRequired
    } else {
        SessionStatus::WaitingForInput
    }
}

fn is_permission_prompt(content: &str) -> bool {
    let last_lines: String = content.lines().rev().take(20).collect::<Vec<_>>().join("\n");
    let patterns = [
        "Allow",
        "Deny",
        "approve",
        "permission",
        "[y/n]",
        "Yes, allow",
        "allow once",
    ];
    patterns.iter().any(|p| last_lines.to_lowercase().contains(&p.to_lowercase()))
}

fn extract_permission_detail(content: &str) -> Option<String> {
    for line in content.lines().rev().take(10) {
        let trimmed = line.trim();
        if trimmed.contains("shell") || trimmed.contains("write") || trimmed.contains("read") {
            return Some(trimmed.chars().take(60).collect());
        }
    }
    Some("Tool permission".to_string())
}

fn extract_task_from_title(title: &str) -> Option<String> {
    // Codex might set pane title to current task
    if title.is_empty() || title == "zsh" || title == "bash" {
        return None;
    }
    Some(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_kind() {
        let provider = OpenAIProvider::new();
        assert_eq!(provider.kind(), ProviderKind::OpenAI);
    }

    #[test]
    fn test_detect_codex_session() {
        assert!(is_codex_session("codex> help"));
        assert!(is_codex_session("── Codex CLI"));
        assert!(!is_codex_session("$ ls -la"));
    }
}
