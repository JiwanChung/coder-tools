//! Token counting and cost calculation for Claude sessions
//!
//! Reads JSONL session files from ~/.claude/projects/{path_hash}/*.jsonl

use std::fs;
use std::path::{Path, PathBuf};

/// Token usage for a session
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    /// Calculate cost in USD based on Claude pricing
    /// Pricing as of 2024: Sonnet 3.5
    /// - Input: $3 / 1M tokens
    /// - Output: $15 / 1M tokens
    /// - Cache read: $0.30 / 1M tokens
    /// - Cache write: $3.75 / 1M tokens
    pub fn cost_usd(&self) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * 3.0;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * 15.0;
        let cache_read_cost = (self.cache_read_tokens as f64 / 1_000_000.0) * 0.30;
        let cache_write_cost = (self.cache_write_tokens as f64 / 1_000_000.0) * 3.75;
        input_cost + output_cost + cache_read_cost + cache_write_cost
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Get Claude projects directory
fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// Hash a path the way Claude does (simple replacement)
fn hash_path(path: &str) -> String {
    path.replace('/', "-")
}

/// Find JSONL session files for a given working directory
fn find_session_files(working_dir: &str) -> Vec<PathBuf> {
    let projects_dir = match claude_projects_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };

    let path_hash = hash_path(working_dir);
    let session_dir = projects_dir.join(&path_hash);

    if !session_dir.exists() {
        return Vec::new();
    }

    fs::read_dir(&session_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse token usage from a single JSONL file
fn parse_jsonl_tokens(path: &Path) -> TokenUsage {
    let mut usage = TokenUsage::default();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return usage,
    };

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON and extract usage field
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // Look for usage in response
            if let Some(usage_obj) = json.get("usage") {
                if let Some(input) = usage_obj.get("input_tokens").and_then(|v| v.as_u64()) {
                    usage.input_tokens += input;
                }
                if let Some(output) = usage_obj.get("output_tokens").and_then(|v| v.as_u64()) {
                    usage.output_tokens += output;
                }
                if let Some(cache_read) = usage_obj.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
                    usage.cache_read_tokens += cache_read;
                }
                if let Some(cache_write) = usage_obj.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
                    usage.cache_write_tokens += cache_write;
                }
            }
        }
    }

    usage
}

/// Get token usage for a Claude session in the given working directory
pub fn get_claude_usage(working_dir: &str) -> TokenUsage {
    let files = find_session_files(working_dir);

    // Get the most recent session file (by modification time)
    let most_recent = files
        .into_iter()
        .filter_map(|p| {
            fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (p, t))
        })
        .max_by_key(|(_, t)| *t)
        .map(|(p, _)| p);

    match most_recent {
        Some(path) => parse_jsonl_tokens(&path),
        None => TokenUsage::default(),
    }
}

/// Format tokens for display (e.g., "12.3k" or "1.2M")
pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Format cost for display
pub fn format_cost(cost: f64) -> String {
    if cost >= 1.0 {
        format!("${:.2}", cost)
    } else if cost >= 0.01 {
        format!("${:.2}", cost)
    } else if cost > 0.0 {
        format!("${:.3}", cost)
    } else {
        String::from("$0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_cost() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 500_000,
            cache_write_tokens: 0,
        };
        // 1M input @ $3/M = $3
        // 100k output @ $15/M = $1.5
        // 500k cache read @ $0.30/M = $0.15
        let expected = 3.0 + 1.5 + 0.15;
        assert!((usage.cost_usd() - expected).abs() < 0.001);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }
}
