#![allow(dead_code)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════════
// NVM Selector — Node Version Manager integration
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct NvmState {
    pub installed: Vec<String>,
    pub current: Option<String>,
    pub available: bool,
}

pub fn detect_nvm() -> NvmState {
    // Check if nvm is available
    let available = Command::new("bash")
        .args(["-c", "type nvm &>/dev/null || [ -s \"$NVM_DIR/nvm.sh\" ] && echo yes || echo no"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
        .unwrap_or(false);

    if !available { return NvmState { installed: vec![], current: None, available: false }; }

    let installed = Command::new("bash")
        .args(["-c", ". \"$NVM_DIR/nvm.sh\" 2>/dev/null; nvm ls --no-colors 2>/dev/null | grep -oP 'v?\\d+\\.\\d+\\.\\d+' || echo ''"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default();

    let current = Command::new("bash")
        .args(["-c", "node --version 2>/dev/null || echo ''"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .ok().filter(|s| !s.is_empty());

    NvmState { installed, current, available: true }
}

pub fn nvm_use(version: &str) -> Result<String> {
    let output = Command::new("bash")
        .args(["-c", &format!(". \"$NVM_DIR/nvm.sh\" 2>/dev/null && nvm use {} 2>&1 && node --version", version)])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn nvm_install(version: &str) -> Result<String> {
    let output = Command::new("bash")
        .args(["-c", &format!(". \"$NVM_DIR/nvm.sh\" 2>/dev/null && nvm install {} 2>&1", version)])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// Script Runner — package.json scripts + Makefile targets
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptDef {
    pub name: String,
    pub command: String,
    pub source: ScriptSource,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptSource { PackageJson, Makefile, CargoToml }

pub fn detect_scripts(dir: &Path) -> Vec<ScriptDef> {
    let mut scripts = Vec::new();

    // package.json
    if let Ok(content) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(obj) = json.get("scripts").and_then(|s| s.as_object()) {
                for (name, cmd) in obj {
                    if let Some(cmd_str) = cmd.as_str() {
                        scripts.push(ScriptDef {
                            name: name.clone(), command: cmd_str.to_string(),
                            source: ScriptSource::PackageJson, description: None,
                        });
                    }
                }
            }
        }
    }

    // Makefile
    if let Ok(content) = std::fs::read_to_string(dir.join("Makefile")) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(target) = trimmed.strip_suffix(':') {
                let target = target.trim();
                if !target.is_empty() && !target.starts_with('.') && !target.contains(' ') {
                    scripts.push(ScriptDef {
                        name: target.to_string(), command: format!("make {}", target),
                        source: ScriptSource::Makefile, description: None,
                    });
                }
            }
        }
    }

    // Cargo.toml (bin targets)
    if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        if let Ok(toml) = content.parse::<toml::Value>() {
            if let Some(bins) = toml.get("bin").and_then(|b| b.as_array()) {
                for bin in bins {
                    if let Some(name) = bin.get("name").and_then(|n| n.as_str()) {
                        scripts.push(ScriptDef {
                            name: format!("run-{}", name), command: format!("cargo run --bin {}", name),
                            source: ScriptSource::CargoToml, description: None,
                        });
                    }
                }
            }
        }
    }

    scripts
}

pub fn run_script(dir: &Path, command: &str) -> Result<String> {
    let shell = if cfg!(target_os = "windows") { "cmd" } else { "bash" };
    let shell_arg = if cfg!(target_os = "windows") { "/C" } else { "-c" };
    let output = Command::new(shell).args([shell_arg, command]).current_dir(dir).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(if stderr.is_empty() { stdout } else { format!("{stdout}\n{stderr}") })
}

// ═══════════════════════════════════════════════════════════════════════════
// Spaces — multi-project workspace state 
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub root: String,
    pub open_files: Vec<String>,
    pub created_at: u64,
}

impl Space {
    pub fn new(name: &str, root: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            root: root.to_string(),
            open_files: Vec::new(),
            created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacesState {
    pub spaces: Vec<Space>,
    pub active_id: Option<String>,
}

impl SpacesState {
    pub fn new() -> Self { Self { spaces: Vec::new(), active_id: None } }
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() { return Ok(Self::new()); }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
