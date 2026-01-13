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
    /// Requires @agent_provider to be set for detection.
    /// This prevents false positives from stale @agent_status values.
    pub fn from_pane(
        agent_provider: Option<&str>,
        agent_status: Option<&str>,
        agent_task: Option<String>,
    ) -> Self {
        // Only detect if provider is set (hooks are properly configured)
        let has_provider = agent_provider
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false);

        if !has_provider {
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
        // With provider set, status is detected
        let result = DetectionResult::from_pane(
            Some("claude"),
            Some("working"),
            Some("fix the bug".to_string()),
        );
        assert_eq!(result.status, Status::Working);
        assert_eq!(result.task, Some("fix the bug".to_string()));

        // Without provider, always NotDetected (prevents false positives)
        let result = DetectionResult::from_pane(None, Some("working"), Some("task".to_string()));
        assert_eq!(result.status, Status::NotDetected);
        assert_eq!(result.task, None);

        // Empty provider also means NotDetected
        let result = DetectionResult::from_pane(Some(""), Some("working"), None);
        assert_eq!(result.status, Status::NotDetected);
    }
}
