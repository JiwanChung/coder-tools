use anyhow::{Context, Result};
use std::process::Command;

/// Switch to a specific pane (works across sessions)
pub fn switch_to_pane(session: &str, window: u32, pane: u32) -> Result<()> {
    let target = format!("{}:{}.{}", session, window, pane);

    // First, switch the client to the target session (enables cross-session navigation)
    Command::new("tmux")
        .args(["switch-client", "-t", session])
        .output()
        .context("Failed to switch session")?;

    // Then select the window within that session
    Command::new("tmux")
        .args(["select-window", "-t", &format!("{}:{}", session, window)])
        .output()
        .context("Failed to select window")?;

    // Finally select the specific pane
    Command::new("tmux")
        .args(["select-pane", "-t", &target])
        .output()
        .context("Failed to select pane")?;

    Ok(())
}

/// Send keys to a specific pane
pub fn send_keys(pane_id: &str, keys: &str) -> Result<()> {
    Command::new("tmux")
        .args(["send-keys", "-t", pane_id, keys])
        .output()
        .context("Failed to send keys to pane")?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub id: String,
    pub session_name: String,
    pub window_index: u32,
    pub pane_index: u32,
    pub current_path: String,
    /// Agent provider from hook-published @agent_provider option (claude, gemini, codex)
    pub agent_provider: Option<String>,
    /// Agent status from hook-published @agent_status option
    pub agent_status: Option<String>,
    /// Agent task from hook-published @agent_task option
    pub agent_task: Option<String>,
}

impl Pane {
    pub fn display_name(&self) -> String {
        format!(
            "{}:{}.{}",
            self.session_name, self.window_index, self.pane_index
        )
    }
}

/// Format string for list-panes: includes hook-published agent provider, status and task
const PANE_FORMAT: &str = "#{pane_id}\t#{session_name}\t#{window_index}\t#{pane_index}\t#{pane_current_path}\t#{@agent_provider}\t#{@agent_status}\t#{@agent_task}";

pub fn list_panes() -> Result<Vec<Pane>> {
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", PANE_FORMAT])
        .output()
        .context("Failed to execute tmux list-panes")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no server running") || stderr.contains("no current client") {
            return Ok(Vec::new());
        }
        anyhow::bail!("tmux list-panes failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let panes = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                // Parse agent_provider, agent_status, agent_task (may be empty)
                let agent_provider = parts.get(5).and_then(|s| {
                    let s = s.trim();
                    if s.is_empty() { None } else { Some(s.to_string()) }
                });
                let agent_status = parts.get(6).and_then(|s| {
                    let s = s.trim();
                    if s.is_empty() { None } else { Some(s.to_string()) }
                });
                let agent_task = parts.get(7).and_then(|s| {
                    let s = s.trim();
                    if s.is_empty() { None } else { Some(s.to_string()) }
                });

                Some(Pane {
                    id: parts[0].to_string(),
                    session_name: parts[1].to_string(),
                    window_index: parts[2].parse().unwrap_or(0),
                    pane_index: parts[3].parse().unwrap_or(0),
                    current_path: parts[4].to_string(),
                    agent_provider,
                    agent_status,
                    agent_task,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(panes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_display_name() {
        let pane = Pane {
            id: "%0".to_string(),
            session_name: "dev".to_string(),
            window_index: 1,
            pane_index: 0,
            current_path: "/home/user".to_string(),
            agent_provider: Some("claude".to_string()),
            agent_status: Some("working".to_string()),
            agent_task: Some("fix the bug".to_string()),
        };
        assert_eq!(pane.display_name(), "dev:1.0");
    }
}
