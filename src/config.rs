use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents::{Agent, default_agents};
use crate::error::{Error, Result};

/// Top-level configuration persisted to `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema: u32,
    pub store: StoreConfig,
    pub default_agents: Vec<String>,
    pub agents: Vec<Agent>,
}

/// Where master skill data lives on disk (one for global, one for project).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    pub global: String,
    pub project: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            store: StoreConfig {
                global: "${XDG_DATA_HOME:-~/.local/share}/smartcrab-skills/store".to_string(),
                project: ".smartcrab-skills/store".to_string(),
            },
            default_agents: vec!["claude-code".to_string()],
            agents: default_agents(),
        }
    }
}

impl Config {
    /// Path to the config file (`$XDG_CONFIG_HOME/smartcrab-skills/config.json`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigError`] if the user's config directory cannot be determined.
    pub fn path() -> Result<PathBuf> {
        let base = config_dir()?;
        Ok(base.join("smartcrab-skills").join("config.json"))
    }

    /// Load config from disk, creating a default file on first run.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] / [`Error::Json`] if reading or parsing fails.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let text = fs::read_to_string(&path)?;
        let cfg: Self = serde_json::from_str(&text)?;
        Ok(cfg)
    }

    /// Atomically write config to disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] / [`Error::Json`] on serialization or I/O failure.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        write_atomic(&path, &serde_json::to_vec_pretty(self)?)
    }

    /// Expand `store.global` to an absolute path.
    #[must_use]
    pub fn expand_global_store(&self) -> PathBuf {
        expand_path(&self.store.global)
    }

    /// Expand `store.project`, rooted at `project_root` if relative.
    #[must_use]
    pub fn expand_project_store(&self, project_root: &Path) -> PathBuf {
        let expanded = expand_path(&self.store.project);
        if expanded.is_absolute() {
            expanded
        } else {
            project_root.join(expanded)
        }
    }

    /// Find an agent by name.
    #[must_use]
    pub fn agent(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Names of the configured default agents.
    #[must_use]
    pub fn default_agent_names(&self) -> Vec<String> {
        self.default_agents.clone()
    }
}

/// Resolve `XDG_CONFIG_HOME`, falling back to `~/.config` (XDG-pure even on macOS).
///
/// # Errors
///
/// Returns [`Error::ConfigError`] if no home directory can be detected.
pub fn config_dir() -> Result<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Resolve `XDG_DATA_HOME`, falling back to `~/.local/share`.
///
/// # Errors
///
/// Returns [`Error::ConfigError`] if no home directory can be detected.
pub fn data_dir() -> Result<PathBuf> {
    xdg_dir("XDG_DATA_HOME", ".local/share")
}

fn xdg_dir(env_var: &str, home_relative_fallback: &str) -> Result<PathBuf> {
    if let Ok(p) = std::env::var(env_var)
        && !p.is_empty()
    {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir()
        .ok_or_else(|| Error::ConfigError("could not determine home dir".to_string()))?;
    Ok(home.join(home_relative_fallback))
}

/// Expand `~`, `$VAR`, `${VAR:-default}` references.
///
/// Performs `shellexpand::full` (env vars + leading `~`) then a second pass
/// of tilde expansion so that `${VAR:-~/foo}` works even when the variable
/// is unset (the default text is substituted verbatim and would otherwise
/// keep its literal `~`).
///
/// Falls back to the literal string if expansion fails.
#[must_use]
pub fn expand_path(s: &str) -> PathBuf {
    let first = shellexpand::full(s).map_or_else(|_| s.to_string(), std::borrow::Cow::into_owned);
    let second = shellexpand::tilde(&first).into_owned();
    PathBuf::from(second)
}

/// Atomic file write: write to a sibling temp file and rename.
///
/// # Errors
///
/// Returns [`Error::Io`] on any I/O failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = match path.file_name() {
        Some(name) => path.with_file_name(format!(
            ".{}.tmp.{}",
            name.to_string_lossy(),
            std::process::id()
        )),
        None => {
            return Err(Error::ConfigError(format!(
                "invalid path: {}",
                path.display()
            )));
        }
    };
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_claude_code() {
        let cfg = Config::default();
        assert!(cfg.agent("claude-code").is_some());
        assert_eq!(cfg.default_agents, vec!["claude-code".to_string()]);
    }

    #[test]
    fn expand_path_handles_tilde() {
        let p = expand_path("~/foo");
        assert!(p.is_absolute());
    }
}
