//! Gemini CLI provider implementation

use super::{Provider, ProviderKind, SessionInfo, SessionStatus};
use crate::pricing::SessionCost;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct GeminiProvider;

impl GeminiProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for GeminiProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    fn detect(&self, tty: &str, pane_title: &str, content: &str) -> bool {
        // Check pane title for Gemini marker
        if pane_title.contains("gemini") || pane_title.contains("Gemini") {
            return true;
        }

        // Check if Gemini process is running on this TTY
        if find_gemini_pid_by_tty(tty).is_some() {
            return true;
        }

        // Fallback: check screen content for Gemini UI elements
        is_gemini_session(content)
    }

    fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo {
        let mut info = SessionInfo {
            provider: ProviderKind::Gemini,
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
// Gemini-specific detection helpers
// ============================================================================

#[derive(Debug, Clone, Default)]
struct RawSessionInfo {
    #[allow(dead_code)]
    session_id: Option<String>,
    #[allow(dead_code)]
    project_hash: Option<String>,
    last_user_prompt: Option<String>,
    current_tool: Option<String>,
    tool_detail: Option<String>,
    model: Option<String>,
    is_streaming: bool,
    session_cost: Option<SessionCost>,
}

/// Find Gemini process PID by TTY (it's a Node.js process)
fn find_gemini_pid_by_tty(tty: &str) -> Option<u32> {
    let tty_name = tty.trim_start_matches("/dev/");

    // First get all node processes on this TTY
    let output = Command::new("ps")
        .args(["-t", tty_name, "-o", "pid=,command="])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Check if it's a gemini node process
        if line.contains("gemini") || line.contains("@anthropic") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                return parts[0].parse().ok();
            }
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

fn gemini_tmp_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".gemini").join("tmp"))
}

/// Find the most recent session for a project
fn find_latest_session_for_project(project_hash: &str) -> Option<PathBuf> {
    let tmp_dir = gemini_tmp_dir()?;
    let project_dir = tmp_dir.join(project_hash).join("chats");

    if !project_dir.exists() {
        return None;
    }

    let mut sessions: Vec<PathBuf> = fs::read_dir(&project_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|ext| ext == "json").unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();

    sessions.sort_by_key(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    sessions.pop()
}

/// Find the most recently modified project directory
fn find_latest_project() -> Option<String> {
    let tmp_dir = gemini_tmp_dir()?;

    let mut projects: Vec<(String, std::time::SystemTime)> = fs::read_dir(&tmp_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name != "bin" && name.len() > 10 // Project hashes are long
        })
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((e.file_name().to_string_lossy().to_string(), modified))
        })
        .collect();

    projects.sort_by_key(|(_, time)| *time);
    projects.pop().map(|(hash, _)| hash)
}

#[derive(Deserialize)]
struct GeminiSession {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "projectHash")]
    project_hash: Option<String>,
    messages: Option<Vec<GeminiMessage>>,
}

#[derive(Deserialize)]
struct GeminiMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    content: Option<String>,
    tokens: Option<GeminiTokens>,
    model: Option<String>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<GeminiToolCall>>,
}

#[derive(Deserialize)]
struct GeminiTokens {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    thoughts: Option<u64>,
    #[allow(dead_code)]
    tool: Option<u64>,
}

#[derive(Deserialize)]
struct GeminiToolCall {
    name: Option<String>,
    args: Option<serde_json::Value>,
}

fn parse_session_json(json_path: &PathBuf) -> Option<RawSessionInfo> {
    let content = fs::read_to_string(json_path).ok()?;
    let session: GeminiSession = serde_json::from_str(&content).ok()?;

    let mut info = RawSessionInfo {
        session_id: session.session_id,
        project_hash: session.project_hash,
        ..Default::default()
    };

    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut total_cached: u64 = 0;

    if let Some(messages) = session.messages {
        // Process messages from end for latest state
        for msg in messages.iter().rev() {
            // Get model from assistant messages
            if info.model.is_none() {
                if let Some(model) = &msg.model {
                    info.model = Some(model.clone());
                }
            }

            // Get last user prompt
            if info.last_user_prompt.is_none() && msg.msg_type.as_deref() == Some("user") {
                if let Some(content) = &msg.content {
                    if !content.starts_with('<') {
                        info.last_user_prompt = Some(content.chars().take(150).collect());
                    }
                }
            }

            // Get current tool from tool calls
            if info.current_tool.is_none() {
                if let Some(tool_calls) = &msg.tool_calls {
                    if let Some(tool_call) = tool_calls.last() {
                        if let Some(name) = &tool_call.name {
                            info.current_tool = Some(name.clone());
                            if let Some(args) = &tool_call.args {
                                info.tool_detail = extract_tool_detail(name, args);
                            }
                        }
                    }
                }
            }

            // Accumulate tokens
            if let Some(tokens) = &msg.tokens {
                total_input += tokens.input.unwrap_or(0);
                total_output += tokens.output.unwrap_or(0) + tokens.thoughts.unwrap_or(0);
                total_cached += tokens.cached.unwrap_or(0);
            }
        }
    }

    // Calculate cost (using Gemini 2.0 Pro pricing approximation)
    // Input: $1.25/1M tokens, Output: $5/1M tokens, Cached: $0.3125/1M
    if total_input > 0 || total_output > 0 {
        let input_cost = (total_input.saturating_sub(total_cached) as f64) * 1.25 / 1_000_000.0;
        let cached_cost = (total_cached as f64) * 0.3125 / 1_000_000.0;
        let output_cost = (total_output as f64) * 5.0 / 1_000_000.0;

        info.session_cost = Some(SessionCost {
            model: info.model.clone().unwrap_or_else(|| "gemini-2.0-pro".to_string()),
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
        "shell" | "run_shell_command" => args
            .get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.lines().next().unwrap_or("").chars().take(60).collect()),
        "read_file" | "write_file" | "edit_file" => args
            .get("file_path")
            .or(args.get("path"))
            .and_then(|p| p.as_str())
            .and_then(|p| p.rsplit('/').next())
            .map(String::from),
        "search_files" | "grep" => args
            .get("pattern")
            .or(args.get("query"))
            .and_then(|p| p.as_str())
            .map(|p| p.chars().take(40).collect()),
        _ => None,
    }
}

fn get_session_info_by_tty(tty: &str) -> Option<RawSessionInfo> {
    // Try to find the Gemini process
    let pid = find_gemini_pid_by_tty(tty);
    let cwd = pid.and_then(get_process_cwd);

    // Find the latest project (could be improved by matching cwd to project)
    let project_hash = find_latest_project()?;
    let session_path = find_latest_session_for_project(&project_hash)?;

    let mut info = parse_session_json(&session_path)?;
    info.project_hash = Some(project_hash);

    // Check if session was recently modified (indicates active session)
    if let Ok(metadata) = session_path.metadata() {
        if let Ok(modified) = metadata.modified() {
            let age = std::time::SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default();
            if age.as_secs() < 60 {
                info.is_streaming = true;
            }
        }
    }

    let _ = cwd; // May use later for project matching

    Some(info)
}

// ============================================================================
// Content-based detection (fallback)
// ============================================================================

fn is_gemini_session(content: &str) -> bool {
    let indicators = [
        "Gemini CLI",
        "gemini>",
        "✦",           // Gemini's thinking indicator
        "gemini-",     // Model name prefix
        "Google AI",
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
    if content.contains("Thinking") || content.contains("Running") || content.contains("...") {
        return SessionStatus::Working;
    }

    SessionStatus::WaitingForInput
}

fn detect_status_from_content(content: &str) -> SessionStatus {
    if content.contains("Thinking") || content.contains("Running") || content.contains("...") {
        SessionStatus::Working
    } else if is_permission_prompt(content) {
        SessionStatus::PermissionRequired
    } else {
        SessionStatus::WaitingForInput
    }
}

fn is_permission_prompt(content: &str) -> bool {
    // Gemini CLI permission prompts appear at the bottom with specific formatting
    let last_lines: Vec<&str> = content.lines().rev().take(10).collect();
    let last_content = last_lines.join("\n");

    // Look for Gemini-specific permission patterns
    // Gemini shows tool execution prompts with specific formatting
    let permission_patterns = [
        "Do you want to allow",
        "Allow this action",
        "execute this command",
        "run this command",
        "(y/n)",
        "[Y/n]",
        "[y/N]",
    ];

    permission_patterns
        .iter()
        .any(|p| last_content.to_lowercase().contains(&p.to_lowercase()))
}

fn extract_permission_detail(content: &str) -> Option<String> {
    for line in content.lines().rev().take(10) {
        let trimmed = line.trim();
        if trimmed.contains("shell")
            || trimmed.contains("write")
            || trimmed.contains("read")
            || trimmed.contains("execute")
        {
            return Some(trimmed.chars().take(60).collect());
        }
    }
    Some("Tool permission".to_string())
}

fn extract_task_from_title(title: &str) -> Option<String> {
    if title.is_empty() || title == "zsh" || title == "bash" || title == "node" {
        return None;
    }
    Some(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider_kind() {
        let provider = GeminiProvider::new();
        assert_eq!(provider.kind(), ProviderKind::Gemini);
    }

    #[test]
    fn test_detect_gemini_session() {
        assert!(is_gemini_session("Gemini CLI v1.0"));
        assert!(is_gemini_session("gemini> help"));
        assert!(!is_gemini_session("$ ls -la"));
    }
}
