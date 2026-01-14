//! AI coding session detection
//!
//! Status is determined from hook-published tmux pane options (@agent_status, @agent_task).
//! No screen scraping or file parsing required.

use std::fmt;

/// Status of an AI coding session
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

    /// Parse status from hook-published @agent_status option
    pub fn from_agent_status(s: Option<&str>) -> Self {
        match s.map(|s| s.trim()) {
            Some("working") => Status::Working,
            Some("waiting") => Status::WaitingForInput,
            Some("permission") => Status::PermissionRequired,
            _ => Status::NotDetected,
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

/// Detection result with status and context
#[derive(Debug, Clone, Default)]
pub struct DetectionResult {
    pub status: Status,
    /// The task/prompt the agent is working on (from @agent_task)
    pub task: Option<String>,
}

impl DetectionResult {
    /// Create from hook-published pane options
    ///
    /// Requires @agent_provider to be set AND the agent to actually be running.
    /// This prevents false positives from stale pane options after agent exits.
    pub fn from_pane(
        agent_provider: Option<&str>,
        agent_status: Option<&str>,
        agent_task: Option<String>,
        current_command: &str,
    ) -> Self {
        // Only detect if provider is set (hooks are properly configured)
        let provider = match agent_provider {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => {
                return DetectionResult {
                    status: Status::NotDetected,
                    task: None,
                };
            }
        };

        // Validate the agent is actually running by checking current_command
        let is_running = is_agent_running(provider, current_command);
        if !is_running {
            return DetectionResult {
                status: Status::NotDetected,
                task: None,
            };
        }

        DetectionResult {
            status: Status::from_agent_status(agent_status),
            task: agent_task,
        }
    }
}

/// Check if the agent is actually running based on pane_current_command
fn is_agent_running(provider: &str, command: &str) -> bool {
    match provider {
        "claude" => {
            // Claude Code shows as version number (e.g., "2.1.6", "2.1.7")
            // or "claude" or "node" depending on how it was launched
            is_version_string(command) || command == "claude" || command == "node"
        }
        "gemini" => {
            command == "gemini" || command == "node"
        }
        "codex" => {
            // Codex binary may show as "codex" or "codex-aarch64-..." etc.
            command.starts_with("codex") || command == "node"
        }
        _ => false,
    }
}

/// Check if string looks like a version number (e.g., "2.1.6")
fn is_version_string(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Version strings: digits and dots, starting with a digit
    s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_from_agent_status() {
        assert_eq!(Status::from_agent_status(Some("working")), Status::Working);
        assert_eq!(Status::from_agent_status(Some("waiting")), Status::WaitingForInput);
        assert_eq!(Status::from_agent_status(Some("permission")), Status::PermissionRequired);
        assert_eq!(Status::from_agent_status(Some("")), Status::NotDetected);
        assert_eq!(Status::from_agent_status(None), Status::NotDetected);
    }

    #[test]
    fn test_detection_result_from_pane() {
        // With provider set and agent running (version string), status is detected
        let result = DetectionResult::from_pane(
            Some("claude"),
            Some("working"),
            Some("fix the bug".to_string()),
            "2.1.7",
        );
        assert_eq!(result.status, Status::Working);
        assert_eq!(result.task, Some("fix the bug".to_string()));

        // Without provider, always NotDetected
        let result = DetectionResult::from_pane(None, Some("working"), Some("task".to_string()), "2.1.7");
        assert_eq!(result.status, Status::NotDetected);
        assert_eq!(result.task, None);

        // Empty provider also means NotDetected
        let result = DetectionResult::from_pane(Some(""), Some("working"), None, "2.1.7");
        assert_eq!(result.status, Status::NotDetected);

        // Provider set but agent not running (stale options) - NotDetected
        let result = DetectionResult::from_pane(Some("claude"), Some("working"), Some("task".to_string()), "fish");
        assert_eq!(result.status, Status::NotDetected);
        assert_eq!(result.task, None);
    }

    #[test]
    fn test_is_version_string() {
        assert!(is_version_string("2.1.6"));
        assert!(is_version_string("2.1.7"));
        assert!(is_version_string("1.0.0"));
        assert!(!is_version_string("fish"));
        assert!(!is_version_string("node"));
        assert!(!is_version_string(""));
    }

    #[test]
    fn test_is_agent_running() {
        // Claude with version string
        assert!(is_agent_running("claude", "2.1.7"));
        assert!(is_agent_running("claude", "2.1.6"));
        assert!(is_agent_running("claude", "claude"));
        assert!(is_agent_running("claude", "node"));
        assert!(!is_agent_running("claude", "fish"));
        assert!(!is_agent_running("claude", "bat"));

        // Gemini
        assert!(is_agent_running("gemini", "gemini"));
        assert!(is_agent_running("gemini", "node"));
        assert!(!is_agent_running("gemini", "fish"));

        // Codex
        assert!(is_agent_running("codex", "codex"));
        assert!(is_agent_running("codex", "codex-aarch64-a"));
        assert!(is_agent_running("codex", "node"));
        assert!(!is_agent_running("codex", "fish"));
    }
}
