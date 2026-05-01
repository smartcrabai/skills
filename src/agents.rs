use serde::{Deserialize, Serialize};

/// Agent definition: name + skills directory locations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub global_dir: String,
    pub project_dir: String,
}

impl Agent {
    fn new(name: &str, global_dir: &str, project_dir: &str) -> Self {
        Self {
            name: name.to_string(),
            global_dir: global_dir.to_string(),
            project_dir: project_dir.to_string(),
        }
    }
}

/// Built-in agent registry.
///
/// Ported from `vercel-labs/skills` `src/agents.ts`. We use literal `~`-rooted
/// paths and a few `${XDG_CONFIG_HOME}` references; `shellexpand` resolves
/// these at use time. We do not embed agent-specific env-var overrides
/// (`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `VIBE_HOME`) because users can edit
/// `config.json` directly to override.
#[must_use]
pub fn default_agents() -> Vec<Agent> {
    vec![
        Agent::new("aider-desk", "~/.aider-desk/skills", ".aider-desk/skills"),
        Agent::new(
            "amp",
            "${XDG_CONFIG_HOME:-~/.config}/agents/skills",
            ".agents/skills",
        ),
        Agent::new(
            "antigravity",
            "~/.gemini/antigravity/skills",
            ".agents/skills",
        ),
        Agent::new("augment", "~/.augment/skills", ".augment/skills"),
        Agent::new("bob", "~/.bob/skills", ".bob/skills"),
        Agent::new("claude-code", "~/.claude/skills", ".claude/skills"),
        Agent::new("openclaw", "~/.openclaw/skills", "skills"),
        Agent::new("cline", "~/.agents/skills", ".agents/skills"),
        Agent::new(
            "codearts-agent",
            "~/.codeartsdoer/skills",
            ".codeartsdoer/skills",
        ),
        Agent::new("codebuddy", "~/.codebuddy/skills", ".codebuddy/skills"),
        Agent::new("codemaker", "~/.codemaker/skills", ".codemaker/skills"),
        Agent::new("codestudio", "~/.codestudio/skills", ".codestudio/skills"),
        Agent::new("codex", "~/.codex/skills", ".agents/skills"),
        Agent::new(
            "command-code",
            "~/.commandcode/skills",
            ".commandcode/skills",
        ),
        Agent::new("continue", "~/.continue/skills", ".continue/skills"),
        Agent::new("cortex", "~/.snowflake/cortex/skills", ".cortex/skills"),
        Agent::new("crush", "~/.config/crush/skills", ".crush/skills"),
        Agent::new("cursor", "~/.cursor/skills", ".agents/skills"),
        Agent::new("deepagents", "~/.deepagents/agent/skills", ".agents/skills"),
        Agent::new(
            "devin",
            "${XDG_CONFIG_HOME:-~/.config}/devin/skills",
            ".devin/skills",
        ),
        Agent::new("dexto", "~/.agents/skills", ".agents/skills"),
        Agent::new("droid", "~/.factory/skills", ".factory/skills"),
        Agent::new("firebender", "~/.firebender/skills", ".agents/skills"),
        Agent::new("forgecode", "~/.forge/skills", ".forge/skills"),
        Agent::new("gemini-cli", "~/.gemini/skills", ".agents/skills"),
        Agent::new("github-copilot", "~/.copilot/skills", ".agents/skills"),
        Agent::new(
            "goose",
            "${XDG_CONFIG_HOME:-~/.config}/goose/skills",
            ".goose/skills",
        ),
        Agent::new("junie", "~/.junie/skills", ".junie/skills"),
        Agent::new("iflow-cli", "~/.iflow/skills", ".iflow/skills"),
        Agent::new("kilo", "~/.kilocode/skills", ".kilocode/skills"),
        Agent::new("kimi-cli", "~/.config/agents/skills", ".agents/skills"),
        Agent::new("kiro-cli", "~/.kiro/skills", ".kiro/skills"),
        Agent::new("kode", "~/.kode/skills", ".kode/skills"),
        Agent::new("mcpjam", "~/.mcpjam/skills", ".mcpjam/skills"),
        Agent::new("mistral-vibe", "~/.vibe/skills", ".vibe/skills"),
        Agent::new("mux", "~/.mux/skills", ".mux/skills"),
        Agent::new(
            "opencode",
            "${XDG_CONFIG_HOME:-~/.config}/opencode/skills",
            ".agents/skills",
        ),
        Agent::new("openhands", "~/.openhands/skills", ".openhands/skills"),
        Agent::new("pi", "~/.pi/agent/skills", ".pi/skills"),
        Agent::new("qoder", "~/.qoder/skills", ".qoder/skills"),
        Agent::new("qwen-code", "~/.qwen/skills", ".qwen/skills"),
        Agent::new(
            "replit",
            "${XDG_CONFIG_HOME:-~/.config}/agents/skills",
            ".agents/skills",
        ),
        Agent::new("rovodev", "~/.rovodev/skills", ".rovodev/skills"),
        Agent::new("roo", "~/.roo/skills", ".roo/skills"),
        Agent::new(
            "tabnine-cli",
            "~/.tabnine/agent/skills",
            ".tabnine/agent/skills",
        ),
        Agent::new("trae", "~/.trae/skills", ".trae/skills"),
        Agent::new("trae-cn", "~/.trae-cn/skills", ".trae/skills"),
        Agent::new("warp", "~/.agents/skills", ".agents/skills"),
        Agent::new("windsurf", "~/.codeium/windsurf/skills", ".windsurf/skills"),
        Agent::new("zencoder", "~/.zencoder/skills", ".zencoder/skills"),
        Agent::new("neovate", "~/.neovate/skills", ".neovate/skills"),
        Agent::new("pochi", "~/.pochi/skills", ".pochi/skills"),
        Agent::new("adal", "~/.adal/skills", ".adal/skills"),
        Agent::new(
            "universal",
            "${XDG_CONFIG_HOME:-~/.config}/agents/skills",
            ".agents/skills",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_agents_includes_claude_code() {
        let all = default_agents();
        assert!(all.iter().any(|a| a.name == "claude-code"));
    }

    #[test]
    fn default_agents_count_matches_upstream() {
        // Upstream `vercel-labs/skills` currently ships 54 agent entries.
        // If you change the list, update this count.
        assert_eq!(default_agents().len(), 54);
    }
}
