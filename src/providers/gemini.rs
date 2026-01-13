//! Gemini CLI provider implementation (stub)
//!
//! TODO: Implement detection for Gemini CLI tools like:
//! - gemini CLI
//! - Google AI Studio CLI
//! - Other Gemini-based coding assistants

use super::{Provider, ProviderKind, SessionInfo, SessionStatus};

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
        // TODO: Implement Gemini CLI detection
        //
        // Potential signals:
        // 1. Process name: "gemini", etc.
        // 2. Pane title markers
        // 3. Screen content patterns
        // 4. Config files at ~/.config/gemini/

        let _ = (tty, pane_title, content);
        false
    }

    fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo {
        let _ = (tty, pane_title, content);

        // TODO: Implement session info extraction
        //
        // Potential sources:
        // 1. Log files
        // 2. History files
        // 3. Screen scraping

        SessionInfo {
            provider: ProviderKind::Gemini,
            status: SessionStatus::NotDetected,
            ..Default::default()
        }
    }
}

// ============================================================================
// Gemini-specific helpers (to be implemented)
// ============================================================================

/// Find Gemini CLI process by TTY
#[allow(dead_code)]
fn find_gemini_pid_by_tty(tty: &str) -> Option<u32> {
    use std::process::Command;

    let tty_name = tty.trim_start_matches("/dev/");

    let output = Command::new("ps")
        .args(["-t", tty_name, "-o", "pid=,comm="])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let comm = parts[1].to_lowercase();
            if comm.contains("gemini") {
                return parts[0].parse().ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider_kind() {
        let provider = GeminiProvider::new();
        assert_eq!(provider.kind(), ProviderKind::Gemini);
    }
}
