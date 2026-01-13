//! Claude Code provider implementation

use super::{Provider, ProviderKind, SessionInfo, SessionStatus};
use crate::pricing::{self, SessionCost};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;

pub struct ClaudeProvider;

impl ClaudeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for ClaudeProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Claude
    }

    fn detect(&self, tty: &str, pane_title: &str, content: &str) -> bool {
        // Primary detection: Claude process with matching JSONL session file
        if let Some(pid) = find_claude_pid_by_tty(tty) {
            if let Some(cwd) = get_process_cwd(pid) {
                if find_session_jsonl(&cwd).is_some() {
                    return true;
                }
            }
        }

        // Secondary: pane title has Claude marker AND screen has Claude UI elements
        if pane_title.contains("✳") && is_claude_code_session(content) {
            return true;
        }

        false
    }

    fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo {
        let mut info = SessionInfo {
            provider: ProviderKind::Claude,
            ..Default::default()
        };

        // Extract task from pane title
        info.task = extract_task_from_title(pane_title);

        // Try to get detailed info from session files
        if let Some(raw_info) = get_session_info_by_tty(tty) {
            // Determine status from debug log signals
            info.status = if raw_info.is_streaming {
                SessionStatus::Working
            } else if raw_info.is_idle {
                if is_permission_prompt(content) {
                    SessionStatus::PermissionRequired
                } else {
                    SessionStatus::WaitingForInput
                }
            } else {
                // Fallback to stop_reason
                match raw_info.stop_reason.as_deref() {
                    Some("tool_use") => {
                        if is_permission_prompt(content) {
                            SessionStatus::PermissionRequired
                        } else {
                            SessionStatus::Working
                        }
                    }
                    Some("end_turn") => SessionStatus::WaitingForInput,
                    _ => detect_status_from_content(content),
                }
            };

            // Build detail string
            info.detail = if info.status == SessionStatus::Working {
                match (&raw_info.current_tool, &raw_info.tool_detail) {
                    (Some(tool), Some(detail)) => Some(format!("{}: {}", tool, detail)),
                    (Some(tool), None) => Some(tool.clone()),
                    _ => raw_info.last_user_prompt.clone(),
                }
            } else if info.status == SessionStatus::PermissionRequired {
                extract_permission_detail(content)
            } else {
                raw_info.last_user_prompt.clone().or_else(|| extract_last_action(content))
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
            } else {
                info.detail = extract_last_action(content);
            }
        }

        info
    }
}

// ============================================================================
// Claude-specific detection helpers
// ============================================================================

/// Raw session info from Claude files
#[derive(Debug, Clone, Default)]
struct RawSessionInfo {
    session_id: Option<String>,
    #[allow(dead_code)]
    cwd: Option<String>,
    last_user_prompt: Option<String>,
    current_tool: Option<String>,
    tool_detail: Option<String>,
    model: Option<String>,
    stop_reason: Option<String>,
    is_streaming: bool,
    is_idle: bool,
    session_cost: Option<SessionCost>,
}

/// Find Claude process PID by TTY
fn find_claude_pid_by_tty(tty: &str) -> Option<u32> {
    let tty_name = tty.trim_start_matches("/dev/");

    let output = Command::new("ps")
        .args(["-t", tty_name, "-o", "pid=,comm="])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "claude" {
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

/// Encode a path to Claude's project folder format
fn encode_path(path: &str) -> String {
    path.chars()
        .map(|c| match c {
            '/' | '_' | '.' | ' ' => '-',
            _ => c,
        })
        .collect()
}

fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

fn claude_debug_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("debug"))
}

fn claude_history_file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("history.jsonl"))
}

fn find_session_jsonl(cwd: &str) -> Option<PathBuf> {
    let projects_dir = claude_projects_dir()?;
    let encoded = encode_path(cwd);
    let project_dir = projects_dir.join(&encoded);

    if !project_dir.exists() {
        return None;
    }

    let mut jsonl_files: Vec<_> = fs::read_dir(&project_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.ends_with(".jsonl") && !name_str.starts_with("agent-")
        })
        .collect();

    jsonl_files.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    jsonl_files.last().map(|e| e.path())
}

fn get_debug_file(session_id: &str) -> Option<PathBuf> {
    let debug_dir = claude_debug_dir()?;
    let debug_file = debug_dir.join(format!("{}.txt", session_id));
    if debug_file.exists() {
        Some(debug_file)
    } else {
        None
    }
}

#[derive(Deserialize)]
struct JsonlEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    message: Option<JsonlMessage>,
}

#[derive(Deserialize)]
struct HistoryEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    display: Option<String>,
}

#[derive(Deserialize)]
struct JsonlMessage {
    role: Option<String>,
    model: Option<String>,
    stop_reason: Option<String>,
    content: Option<serde_json::Value>,
}

fn get_last_prompt_from_history(session_id: &str) -> Option<String> {
    let history_file = claude_history_file()?;
    let file = fs::File::open(&history_file).ok()?;
    let file_size = file.metadata().ok()?.len();

    let mut reader = BufReader::new(file);
    let start_pos = file_size.saturating_sub(50_000);
    reader.seek(SeekFrom::Start(start_pos)).ok()?;

    if start_pos > 0 {
        let mut skip = String::new();
        reader.read_line(&mut skip).ok()?;
    }

    let mut last_prompt = None;
    for line in reader.lines().flatten() {
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
            if entry.session_id.as_deref() == Some(session_id) {
                if let Some(display) = entry.display {
                    last_prompt = Some(display);
                }
            }
        }
    }

    last_prompt
}

fn parse_session_context(jsonl_path: &PathBuf) -> Result<RawSessionInfo> {
    let file = fs::File::open(jsonl_path).context("Failed to open JSONL file")?;
    let file_size = file.metadata()?.len();

    let mut reader = BufReader::new(file);
    let start_pos = file_size.saturating_sub(100_000);
    reader.seek(SeekFrom::Start(start_pos))?;

    if start_pos > 0 {
        let mut skip = String::new();
        reader.read_line(&mut skip)?;
    }

    let mut info = RawSessionInfo::default();
    let mut entries: Vec<JsonlEntry> = Vec::new();

    for line in reader.lines().flatten() {
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<JsonlEntry>(&line) {
            entries.push(entry);
        }
    }

    for entry in entries.iter().rev() {
        if info.session_id.is_none() {
            info.session_id = entry.session_id.clone();
        }

        if let Some(msg) = &entry.message {
            if info.model.is_none() {
                info.model = msg.model.clone();
            }
            if info.stop_reason.is_none() {
                info.stop_reason = msg.stop_reason.clone();
            }

            if let Some(content) = &msg.content {
                if let Some(arr) = content.as_array() {
                    for item in arr {
                        let item_type = item.get("type").and_then(|t| t.as_str());

                        if info.last_user_prompt.is_none()
                            && item_type == Some("text")
                            && msg.role.as_deref() == Some("user")
                        {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                if !text.starts_with('<') {
                                    info.last_user_prompt =
                                        Some(text.chars().take(150).collect());
                                }
                            }
                        }

                        if info.current_tool.is_none() && item_type == Some("tool_use") {
                            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                info.current_tool = Some(name.to_string());

                                if let Some(input) = item.get("input") {
                                    info.tool_detail = match name {
                                        "Bash" => input
                                            .get("command")
                                            .and_then(|c| c.as_str())
                                            .map(|c| {
                                                c.lines().next().unwrap_or("").chars().take(60).collect()
                                            }),
                                        "Read" | "Edit" | "Write" => input
                                            .get("file_path")
                                            .and_then(|p| p.as_str())
                                            .and_then(|p| p.rsplit('/').next())
                                            .map(String::from),
                                        "Grep" => input
                                            .get("pattern")
                                            .and_then(|p| p.as_str())
                                            .map(|p| p.chars().take(40).collect()),
                                        "Task" => input
                                            .get("description")
                                            .and_then(|d| d.as_str())
                                            .map(|d| d.chars().take(40).collect()),
                                        _ => None,
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }

        if info.session_id.is_some()
            && info.last_user_prompt.is_some()
            && info.current_tool.is_some()
        {
            break;
        }
    }

    Ok(info)
}

fn parse_debug_state(debug_path: &PathBuf) -> Result<(bool, bool)> {
    let file = fs::File::open(debug_path).context("Failed to open debug file")?;
    let file_size = file.metadata()?.len();

    let mut reader = BufReader::new(file);
    let start_pos = file_size.saturating_sub(10_000);
    reader.seek(SeekFrom::Start(start_pos))?;

    if start_pos > 0 {
        let mut skip = String::new();
        reader.read_line(&mut skip)?;
    }

    let mut last_stream_time: Option<String> = None;
    let mut last_idle_time: Option<String> = None;
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

    for line in lines.iter().rev().take(50) {
        if last_stream_time.is_none() && line.contains("Stream started") {
            if line.len() >= 24 {
                last_stream_time = Some(line[..24].to_string());
            }
        }
        if last_idle_time.is_none() && line.contains("idle_prompt") {
            if line.len() >= 24 {
                last_idle_time = Some(line[..24].to_string());
            }
        }
        if last_stream_time.is_some() && last_idle_time.is_some() {
            break;
        }
    }

    let is_streaming = match (&last_stream_time, &last_idle_time) {
        (Some(stream), Some(idle)) => stream > idle,
        (Some(_), None) => true,
        _ => false,
    };

    let is_idle = match (&last_stream_time, &last_idle_time) {
        (Some(stream), Some(idle)) => idle > stream,
        (None, Some(_)) => true,
        _ => false,
    };

    Ok((is_streaming, is_idle))
}

fn get_session_info_by_tty(tty: &str) -> Option<RawSessionInfo> {
    let pid = find_claude_pid_by_tty(tty)?;
    let cwd = get_process_cwd(pid)?;
    let jsonl_path = find_session_jsonl(&cwd)?;

    let mut info = parse_session_context(&jsonl_path).ok()?;
    info.cwd = Some(cwd);
    info.session_cost = pricing::calculate_session_cost(&jsonl_path);

    if let Some(session_id) = &info.session_id {
        if let Some(debug_path) = get_debug_file(session_id) {
            if let Ok((is_streaming, is_idle)) = parse_debug_state(&debug_path) {
                info.is_streaming = is_streaming;
                info.is_idle = is_idle;
            }
        }

        if info.last_user_prompt.is_none() {
            info.last_user_prompt = get_last_prompt_from_history(session_id);
        }
    }

    Some(info)
}

// ============================================================================
// Content-based detection (fallback)
// ============================================================================

fn is_claude_code_session(content: &str) -> bool {
    let indicators = ["⏺ ", "⎿", "✢", "⏵⏵", "Claude Code"];
    indicators.iter().any(|i| content.contains(i))
}

fn detect_status_from_content(content: &str) -> SessionStatus {
    if content.contains("esc to interrupt") {
        SessionStatus::Working
    } else if is_permission_prompt(content) {
        SessionStatus::PermissionRequired
    } else {
        SessionStatus::WaitingForInput
    }
}

fn is_permission_prompt(content: &str) -> bool {
    // Only check the last 5 lines - permission prompts are at the very bottom
    let last_lines: Vec<&str> = content.lines().rev().take(5).collect();
    let last_content = last_lines.join("\n");

    // Look for actual interactive permission buttons at the bottom
    // These are the selectable options Claude shows
    let button_patterns = [
        "Yes, allow once",
        "Yes, allow always",
        "Yes, proceed",
        "No, deny",
        "Don't allow",
        "Allow once",
        "Allow always",
    ];

    button_patterns.iter().any(|p| last_content.contains(p))
}

fn extract_permission_detail(content: &str) -> Option<String> {
    for line in content.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("⏺") {
            let action = trimmed.trim_start_matches("⏺").trim();
            if !action.is_empty() {
                return Some(action.chars().take(60).collect());
            }
        }
    }
    Some("Tool permission".to_string())
}

fn extract_last_action(content: &str) -> Option<String> {
    let last_marker_pos = content.rfind("⏺")?;
    let after_marker = &content[last_marker_pos..];

    let cleaned: Vec<&str> = after_marker
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.chars().all(|c| c == '─'))
        .filter(|l| !l.starts_with('>'))
        .filter(|l| !l.contains("bypass permissions"))
        .take(5)
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    let mut result = cleaned.join("\n");
    if result.starts_with("⏺") {
        result = result.trim_start_matches("⏺").trim().to_string();
    }

    Some(result)
}

fn extract_task_from_title(title: &str) -> Option<String> {
    if !title.contains("✳") {
        return None;
    }
    let task = title.trim_start_matches("✳").trim();

    // Remove version number at end
    if let Some(space_pos) = task.rfind(' ') {
        let potential_version = &task[space_pos + 1..];
        if potential_version.chars().all(|c| c.is_ascii_digit() || c == '.') {
            let trimmed = task[..space_pos].trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if task.is_empty() {
        None
    } else {
        Some(task.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_path() {
        assert_eq!(
            encode_path("/Users/jiwan/projects/test"),
            "-Users-jiwan-projects-test"
        );
        assert_eq!(
            encode_path("/Users/jiwan/projects/cua_project"),
            "-Users-jiwan-projects-cua-project"
        );
    }

    #[test]
    fn test_detect_claude_session() {
        assert!(is_claude_code_session("⏺ Read(file.txt)"));
        assert!(!is_claude_code_session("$ ls -la"));
    }
}
