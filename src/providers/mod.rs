//! Provider abstraction for different AI coding assistants
//!
//! Each provider (Claude, OpenAI, Gemini, etc.) implements the `Provider` trait
//! to detect sessions, parse status, and calculate costs.

pub mod claude;
pub mod gemini;
pub mod openai;

use crate::pricing::SessionCost;

/// Status of an AI coding session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionStatus {
    /// Waiting for user input
    WaitingForInput,
    /// Waiting for permission approval
    PermissionRequired,
    /// Actively working (thinking, tool execution)
    Working,
    /// Not a recognized AI session
    #[default]
    NotDetected,
}

impl SessionStatus {
    #[allow(dead_code)]
    pub fn icon(&self) -> &'static str {
        match self {
            SessionStatus::WaitingForInput => ">_",
            SessionStatus::PermissionRequired => "⚠",
            SessionStatus::Working => "◐",
            SessionStatus::NotDetected => "--",
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            SessionStatus::WaitingForInput => "Waiting for input",
            SessionStatus::PermissionRequired => "Permission required",
            SessionStatus::Working => "Working",
            SessionStatus::NotDetected => "Not detected",
        }
    }
}

/// Information detected about an AI session
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    /// Which provider this session belongs to
    pub provider: ProviderKind,
    /// Current status
    pub status: SessionStatus,
    /// Status detail (e.g., current tool, last prompt)
    pub detail: Option<String>,
    /// Last user prompt/message
    pub last_user_prompt: Option<String>,
    /// Current tool being executed (if any)
    pub current_tool: Option<String>,
    /// Tool detail (e.g., file path, command)
    pub tool_detail: Option<String>,
    /// Model being used
    pub model: Option<String>,
    /// Task description (from pane title or similar)
    pub task: Option<String>,
    /// Token usage and cost
    pub cost: Option<SessionCost>,
}

/// Supported AI coding assistant providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderKind {
    Claude,
    OpenAI,
    Gemini,
    #[default]
    Unknown,
}

impl ProviderKind {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            ProviderKind::Claude => "Claude",
            ProviderKind::OpenAI => "OpenAI",
            ProviderKind::Gemini => "Gemini",
            ProviderKind::Unknown => "Unknown",
        }
    }

    #[allow(dead_code)]
    pub fn icon(&self) -> &'static str {
        match self {
            ProviderKind::Claude => "✳",
            ProviderKind::OpenAI => "◆",
            ProviderKind::Gemini => "✦",
            ProviderKind::Unknown => "?",
        }
    }
}

/// Trait for AI coding assistant providers
pub trait Provider: Send + Sync {
    /// Get the provider kind
    fn kind(&self) -> ProviderKind;

    /// Check if this provider can handle the given pane
    /// Returns true if the pane appears to be running this provider's CLI
    fn detect(&self, tty: &str, pane_title: &str, content: &str) -> bool;

    /// Get detailed session information
    fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo;
}

/// Registry of all available providers
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: Vec::new(),
        };

        // Register all providers (order matters - first match wins)
        registry.register(Box::new(claude::ClaudeProvider::new()));
        registry.register(Box::new(openai::OpenAIProvider::new()));
        registry.register(Box::new(gemini::GeminiProvider::new()));

        registry
    }

    pub fn register(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    /// Detect which provider (if any) is running in the given pane
    #[allow(dead_code)]
    pub fn detect(&self, tty: &str, pane_title: &str, content: &str) -> Option<ProviderKind> {
        for provider in &self.providers {
            if provider.detect(tty, pane_title, content) {
                return Some(provider.kind());
            }
        }
        None
    }

    /// Get session info from the appropriate provider
    pub fn get_session_info(&self, tty: &str, pane_title: &str, content: &str) -> SessionInfo {
        for provider in &self.providers {
            if provider.detect(tty, pane_title, content) {
                return provider.get_session_info(tty, pane_title, content);
            }
        }

        // No provider detected
        SessionInfo {
            provider: ProviderKind::Unknown,
            status: SessionStatus::NotDetected,
            ..Default::default()
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
