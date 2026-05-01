# skills

A Rust port of [`vercel-labs/skills`](https://github.com/vercel-labs/skills) — a CLI for installing and managing agent skills (`SKILL.md` collections) across local development tooling.

## Install

```bash
cargo install --path .
```

## Quick start

```bash
# Add a skill globally and link it into ~/.claude/skills (default agent)
skills add vercel-labs/skills/skills/find-skills -g -y

# Same, but for the current project
skills add vercel-labs/skills/skills/find-skills -p -y

# Use deep copies instead of symlinks
skills add vercel-labs/skills/skills/find-skills -g --copy

# Pick a specific ref
skills add vercel-labs/skills/skills/find-skills#main -g -y
```

## Commands

| Command | Description |
|---|---|
| `skills add <repo>/<sub_path>` | Install a skill from a GitHub repo (this PR) |
| `skills list` | List installed skills (Phase B) |
| `skills find [query]` | Search for skills (Phase B) |
| `skills remove [skills]` | Uninstall skill(s) (Phase B) |
| `skills update [skills]` | Refresh skill(s) from upstream (Phase B) |
| `skills init [name]` | Scaffold a new `SKILL.md` (Phase B) |
| `skills config <key> <op> [value]` | Read/modify config (Phase B) |

### `add` flags

| Flag | Description |
|---|---|
| `-g`, `--global` | Install to the user-global store |
| `-p`, `--project` | Install into the current project |
| `--copy` | Deep-copy into agent dirs instead of symlinks |
| `-a`, `--agent <name>` | Specific agent to wire up (repeatable) |
| `-y`, `--yes` | Skip interactive prompts |

## Configuration

`skills` honors `XDG_CONFIG_HOME` (defaulting to `~/.config`) and `XDG_DATA_HOME` (defaulting to `~/.local/share`). On first run it writes a default `config.json` populated with the upstream agent registry.

```text
$XDG_CONFIG_HOME/smartcrab-skills/
├── config.json   # editable config (agents, default_agents, store paths)
└── skills.json   # registry of installed skills
$XDG_DATA_HOME/smartcrab-skills/
└── store/
    ├── global/<skill-name>/   # canonical master copy
    └── project/<skill-name>/
```

### `config.json` example

```json
{
  "schema": 1,
  "store": {
    "global": "~/.local/share/smartcrab-skills/store",
    "project": ".smartcrab-skills/store"
  },
  "default_agents": ["claude-code"],
  "agents": [
    { "name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills" },
    { "name": "opencode",    "global_dir": "${XDG_CONFIG_HOME:-~/.config}/opencode/skills", "project_dir": ".agents/skills" }
  ]
}
```

Path expansion supports `~`, `$VAR`, and `${VAR:-default}`.

## License

MIT
