use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Ensure hooks are installed for all supported providers.
/// This is called before the TUI starts to auto-inject hooks if missing.
pub fn ensure_hooks_installed() -> Result<()> {
    // Check and inject Claude hooks
    if let Err(e) = check_and_inject_claude_hooks() {
        eprintln!("Warning: Could not set up Claude hooks: {}", e);
    }

    // Check and inject Gemini hooks
    if let Err(e) = check_and_inject_gemini_hooks() {
        eprintln!("Warning: Could not set up Gemini hooks: {}", e);
    }

    Ok(())
}

fn get_claude_settings_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

fn get_gemini_settings_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".gemini").join("settings.json"))
}

fn create_backup(path: &PathBuf) -> Result<()> {
    let backup_path = path.with_extension("json.bak");
    fs::copy(path, &backup_path).context("Failed to create backup")?;
    Ok(())
}

fn claude_hooks() -> Value {
    json!({
        "UserPromptSubmit": [{
            "hooks": [{
                "type": "command",
                "command": "bash -c 'TASK=$(jq -r \".prompt // empty\" | tr \"\\n\" \" \" | head -c 100); tmux set -p @agent_provider claude \\; set -p @agent_task \"$TASK\" \\; set -p @agent_status working 2>/dev/null'"
            }]
        }],
        "Stop": [{
            "hooks": [{
                "type": "command",
                "command": "tmux set -p @agent_status waiting 2>/dev/null || true"
            }]
        }],
        "PermissionRequest": [{
            "hooks": [{
                "type": "command",
                "command": "tmux set -p @agent_status permission 2>/dev/null || true"
            }]
        }]
    })
}

fn gemini_hooks() -> Value {
    json!({
        "BeforeAgent": [{
            "hooks": [{
                "type": "command",
                "command": "tmux set -p @agent_provider gemini \\; set -p @agent_status working 2>/dev/null"
            }]
        }],
        "AfterAgent": [{
            "hooks": [{
                "type": "command",
                "command": "tmux set -p @agent_status waiting 2>/dev/null"
            }]
        }]
    })
}

fn has_claude_hooks(settings: &Value) -> bool {
    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => return false,
    };

    // Check for required hook types
    let required = ["UserPromptSubmit", "Stop", "PermissionRequest"];
    required.iter().all(|key| hooks.get(key).is_some())
}

fn has_gemini_hooks(settings: &Value) -> bool {
    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => return false,
    };

    // Check for required hook types
    let required = ["BeforeAgent", "AfterAgent"];
    required.iter().all(|key| hooks.get(key).is_some())
}

fn check_and_inject_claude_hooks() -> Result<bool> {
    let path = get_claude_settings_path()?;

    // If file doesn't exist, create it with hooks
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let settings = json!({
            "hooks": claude_hooks()
        });
        fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
        return Ok(true);
    }

    // Read existing settings
    let content = fs::read_to_string(&path)?;
    let mut settings: Value = serde_json::from_str(&content)?;

    // Check if hooks already exist
    if has_claude_hooks(&settings) {
        return Ok(false);
    }

    // Create backup before modifying
    create_backup(&path)?;

    // Merge hooks into existing settings
    if settings.get("hooks").is_none() {
        settings["hooks"] = json!({});
    }

    let hooks_obj = settings["hooks"].as_object_mut().context("hooks is not an object")?;
    let new_hooks = claude_hooks();
    let new_hooks_obj = new_hooks.as_object().unwrap();

    for (key, value) in new_hooks_obj {
        if !hooks_obj.contains_key(key) {
            hooks_obj.insert(key.clone(), value.clone());
        }
    }

    fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    Ok(true)
}

fn check_and_inject_gemini_hooks() -> Result<bool> {
    let path = get_gemini_settings_path()?;

    // If file doesn't exist, create it with hooks
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let settings = json!({
            "experiments": { "enableHooks": true },
            "hooks": gemini_hooks()
        });
        fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
        return Ok(true);
    }

    // Read existing settings
    let content = fs::read_to_string(&path)?;
    let mut settings: Value = serde_json::from_str(&content)?;

    // Check if hooks already exist
    if has_gemini_hooks(&settings) {
        return Ok(false);
    }

    // Create backup before modifying
    create_backup(&path)?;

    // Ensure experiments.enableHooks is set
    if settings.get("experiments").is_none() {
        settings["experiments"] = json!({});
    }
    settings["experiments"]["enableHooks"] = json!(true);

    // Merge hooks into existing settings
    if settings.get("hooks").is_none() {
        settings["hooks"] = json!({});
    }

    let hooks_obj = settings["hooks"].as_object_mut().context("hooks is not an object")?;
    let new_hooks = gemini_hooks();
    let new_hooks_obj = new_hooks.as_object().unwrap();

    for (key, value) in new_hooks_obj {
        if !hooks_obj.contains_key(key) {
            hooks_obj.insert(key.clone(), value.clone());
        }
    }

    fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    Ok(true)
}
