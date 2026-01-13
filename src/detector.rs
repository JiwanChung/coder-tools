//! AI coding session detection
//!
//! This module provides a unified interface for detecting AI coding sessions
//! across different providers (Claude, OpenAI, Gemini, etc.)

use crate::pricing::SessionCost;
use crate::providers::{ProviderKind, ProviderRegistry, SessionInfo, SessionStatus};
use std::fmt;

/// Status of an AI coding session (for backwards compatibility)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting for user input
    WaitingForInput,
    /// Waiting for permission approval
    PermissionRequired,
    /// Actively working (thinking, tool execution)
    Working,
    /// Not a recognized AI session
    NotDetected,
}

impl Status {
    pub fn icon(&self) -> &'static str {
        match self {
            Status::WaitingForInput => ">_",
            Status::PermissionRequired => "⚠",
            Status::Working => "◐",
            Status::NotDetected => "--",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Status::WaitingForInput => "Waiting for input",
            Status::PermissionRequired => "Permission required",
            Status::Working => "Working",
            Status::NotDetected => "Not detected",
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Status::NotDetected
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.icon(), self.label())
    }
}

impl From<SessionStatus> for Status {
    fn from(status: SessionStatus) -> Self {
        match status {
            SessionStatus::WaitingForInput => Status::WaitingForInput,
            SessionStatus::PermissionRequired => Status::PermissionRequired,
            SessionStatus::Working => Status::Working,
            SessionStatus::NotDetected => Status::NotDetected,
        }
    }
}

/// Detection result with status and context
#[derive(Debug, Clone, Default)]
pub struct DetectionResult {
    pub status: Status,
    #[allow(dead_code)]
    pub provider: ProviderKind,
    pub detail: Option<String>,
    pub tokens: Option<String>,
    // Rich context from session files
    pub last_user_prompt: Option<String>,
    pub current_tool: Option<String>,
    pub tool_detail: Option<String>,
    #[allow(dead_code)]
    pub model: Option<String>,
    // Task from pane title
    pub pane_task: Option<String>,
    // Session usage and cost
    pub session_cost: Option<SessionCost>,
}

impl From<SessionInfo> for DetectionResult {
    fn from(info: SessionInfo) -> Self {
        DetectionResult {
            status: info.status.into(),
            provider: info.provider,
            detail: info.detail,
            tokens: None, // Set separately from screen scraping
            last_user_prompt: info.last_user_prompt,
            current_tool: info.current_tool,
            tool_detail: info.tool_detail,
            model: info.model,
            pane_task: info.task,
            session_cost: info.cost,
        }
    }
}

/// Global provider registry (lazy initialized)
fn get_registry() -> &'static ProviderRegistry {
    use std::sync::OnceLock;
    static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ProviderRegistry::new)
}

/// Detect AI coding session status using the provider system
pub fn detect_status_from_session(
    tty: &str,
    content: &str,
    pane_title: Option<&str>,
) -> DetectionResult {
    let registry = get_registry();
    let title = pane_title.unwrap_or("");

    // Use provider registry to detect and get session info
    let info = registry.get_session_info(tty, title, content);

    let mut result = DetectionResult::from(info);

    // Extract tokens from screen content (provider-agnostic)
    result.tokens = extract_tokens(content);

    result
}

/// Detect status from screen content only (fallback, provider-agnostic)
#[allow(dead_code)]
pub fn detect_status(content: &str) -> DetectionResult {
    // Check for common AI CLI indicators
    if is_ai_session(content) {
        if content.contains("esc to interrupt") {
            return DetectionResult {
                status: Status::Working,
                tokens: extract_tokens(content),
                detail: extract_last_user_command(content),
                ..Default::default()
            };
        }

        if is_permission_prompt(content) {
            return DetectionResult {
                status: Status::PermissionRequired,
                detail: extract_permission_detail(content),
                ..Default::default()
            };
        }

        return DetectionResult {
            status: Status::WaitingForInput,
            detail: extract_last_action(content),
            ..Default::default()
        };
    }

    DetectionResult {
        status: Status::NotDetected,
        ..Default::default()
    }
}

// ============================================================================
// Screen content extraction helpers (provider-agnostic)
// ============================================================================

#[allow(dead_code)]
fn is_ai_session(content: &str) -> bool {
    // Common AI CLI indicators
    let indicators = [
        "⏺ ",        // Claude tool marker
        "⎿",         // Claude output marker
        "✢",         // Claude thinking
        "⏵⏵",       // Claude permission mode
        "Claude Code",
        // Add more provider indicators here
    ];
    indicators.iter().any(|i| content.contains(i))
}

#[allow(dead_code)]
fn is_permission_prompt(content: &str) -> bool {
    let last_lines: String = content.lines().rev().take(20).collect::<Vec<_>>().join("\n");
    let patterns = [
        "Allow",
        "Deny",
        "Yes, allow",
        "allow this",
        "Yes, proceed",
        "allow once",
        "allow always",
    ];
    patterns.iter().any(|p| last_lines.contains(p))
}

#[allow(dead_code)]
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

fn extract_tokens(content: &str) -> Option<String> {
    for line in content.lines().rev() {
        if line.contains("tokens") && line.contains("↓") {
            if let Some(pos) = line.find("↓") {
                let after = &line[pos..];
                let arrow_len = "↓".len();
                if after.len() > arrow_len {
                    let rest = &after[arrow_len..].trim_start();
                    if let Some(end) = rest.find("tokens") {
                        let token_part = rest[..end].trim();
                        return Some(format!("{}tokens", token_part));
                    }
                }
            }
        }
    }
    None
}

fn extract_last_user_command(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut command_lines: Vec<&str> = Vec::new();
    let mut in_command = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.chars().all(|c| c == '─') {
            if in_command && !command_lines.is_empty() {
                break;
            }
            continue;
        }

        if trimmed == ">" || trimmed.starts_with("> ") {
            command_lines.clear();
            in_command = true;
            if trimmed.len() > 2 {
                command_lines.push(&trimmed[2..]);
            }
            continue;
        }

        if in_command && (trimmed.starts_with("⏺") || trimmed.starts_with("✢")) {
            break;
        }

        if in_command {
            command_lines.push(trimmed);
        }
    }

    if command_lines.is_empty() {
        return None;
    }

    let result = command_lines.join(" ");
    let truncated: String = result.chars().take(80).collect();

    if truncated.len() < result.len() {
        Some(format!("{}...", truncated))
    } else {
        Some(truncated)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_not_ai_session() {
        let content = "$ ls -la\ntotal 0\ndrwxr-xr-x  2 user  staff  64 Dec 23 10:00 .";
        let result = detect_status(content);
        assert_eq!(result.status, Status::NotDetected);
    }

    #[test]
    fn test_detect_working() {
        let content = "⏺ Read(file.txt)\n✢ Mulling… (esc to interrupt · 1m 30s)";
        let result = detect_status(content);
        assert_eq!(result.status, Status::Working);
    }

    #[test]
    fn test_detect_waiting() {
        let content = "⏺ Done with the task.\n─────────────────────────────────────\n> \n─────────────────────────────────────";
        let result = detect_status(content);
        assert_eq!(result.status, Status::WaitingForInput);
    }

    #[test]
    fn test_status_conversion() {
        assert_eq!(Status::from(SessionStatus::Working), Status::Working);
        assert_eq!(Status::from(SessionStatus::WaitingForInput), Status::WaitingForInput);
    }
}
