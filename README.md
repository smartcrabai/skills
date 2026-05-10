# skills

A Rust port of [`vercel-labs/skills`](https://github.com/vercel-labs/skills) — a CLI for installing and managing agent skills (`SKILL.md` collections) across local development tooling.

## Install

Homebrew (macOS / Linux):

```bash
brew install smartcrabai/tap/skills
```

Shell installer (macOS / Linux):

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/smartcrabai/skills/releases/latest/download/skills-installer.sh | sh
```

From source:

```bash
cargo install --git https://github.com/smartcrabai/skills
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

# Install from a full GitHub `tree` URL (ref + sub-path are extracted)
skills add https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines -g -y

# Install from any other host or via SSH
skills add https://gitlab.com/org/repo -g -y
skills add git@github.com:expo/skills.git -g -y

# Install from a local directory containing SKILL.md
skills add ./my-local-skill -g -y

# List, search, refresh, remove
skills list -g
skills find slides
skills update -g
skills remove find-skills -g -y

# Scaffold a new skill in ./my-skill/SKILL.md
skills init my-skill

# Generate a new skill from a natural-language description via an AI agent
skills create "summarize PRs into release notes" -g -y
```

## Commands

| Command | Description |
|---|---|
| `skills add <source>` | Install a skill from a git repo or a local directory (see [`add`](#add) for the supported source formats) |
| `skills list` | List installed skills as a table or JSON |
| `skills find [query]` | Search the remote registry, falling back to the local one |
| `skills remove [skills...]` | Uninstall one or more skills |
| `skills update [skills...]` | Re-fetch and relink skills from upstream |
| `skills init [name]` | Scaffold a new `SKILL.md` template |
| `skills create <description>` | Generate a new skill via an AI creator agent and install it |
| `skills config <key> <op> [value...]` | Read or modify `config.json` |

### `add`

Installs a skill into the master store and wires it into one or more agents' skill directories.

```bash
skills add <source> [-g|--global | -p|--project] [--copy] [-a <agent>]... [-y]
```

| Flag | Description |
|---|---|
| `-g`, `--global` | Install to the user-global store |
| `-p`, `--project` | Install into the current project |
| `--copy` | Deep-copy into agent dirs instead of symlinks |
| `-a`, `--agent <name>` | Specific agent to wire up (repeatable) |
| `-y`, `--yes` | Skip interactive prompts |

`<source>` accepts the following formats:

| Format | Example |
|---|---|
| GitHub shorthand | `expo/skills` |
| GitHub shorthand + sub-path | `vercel-labs/agent-skills/skills/web-design-guidelines` |
| Any of the above with a ref | `vercel-labs/agent-skills#main` |
| GitHub URL | `https://github.com/expo/skills` |
| GitHub `tree` URL (extracts ref + sub-path) | `https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines` |
| GitLab / any HTTPS git URL | `https://gitlab.com/org/repo` |
| SSH git URL | `git@github.com:expo/skills.git` |
| Local directory | `./my-local-skill`, `/abs/path/to/skill`, `~/skills/foo` |

Local sources must point at a directory containing `SKILL.md` (the skill is copied into the master store; updates are picked up on `skills update`). Without `-g`/`-p` on a TTY, `add` prompts for the scope; in non-TTY mode one of the two flags is required. Without `-a` it falls back to `default_agents` from `config.json`.

### `list`

```bash
skills list [-g|--global | -p|--project] [--json]
```

By default lists everything in the registry. `-p` filters to skills installed for the current working directory. `--json` emits the full registry rows.

### `find`

```bash
skills find [query] [--json]
```

Calls the remote search API (default `https://skills.sh/api/search`, overridable via `SKILLS_SEARCH_API`) and renders a table. If the API is unreachable, falls back to a case-insensitive grep over the local registry. On a TTY, omitting `[query]` opens an interactive prompt.

### `remove`

```bash
skills remove [skills...] [-g|--global | -p|--project] [-y]
```

Without names, opens an interactive multiselect of installed skills (TTY only). With names, resolves each against the chosen scope, prompts for confirmation, then unlinks agent dirs, deletes the master copy, and removes the registry entry. `-g` / `-p` narrow the search; if a name is ambiguous, project (matching cwd) wins, then global.

### `update`

```bash
skills update [skills...] [-g|--global | -p|--project] [-y]
```

Re-fetches each target from its recorded source/ref. If the upstream commit is unchanged it prints `up-to-date`; otherwise it overwrites the master copy and re-links agent dirs (preserving the original `Method`). For global skills, any newly-added entry in `default_agents` is also wired up.

### `init`

```bash
skills init [name]
```

Writes a `SKILL.md` template with frontmatter and section scaffolding. With a name, creates `./<name>/SKILL.md`; without one, uses the basename of the cwd and writes `./SKILL.md`. Refuses to overwrite an existing file.

### `create`

```bash
skills create <description> [-c|--creator <agent>] [-n|--name <name>] [-g|--global | -p|--project] [--copy] [-a <agent>]... [-y]
```

| Flag | Description |
|---|---|
| `-c`, `--creator <agent>` | Creator agent to invoke (defaults to `default_creator` in `config.json`, currently only `claude-code`) |
| `-n`, `--name <name>` | Override the auto-generated kebab-case skill name |
| `-g`, `--global` | Install to the user-global store |
| `-p`, `--project` | Install into the current project |
| `--copy` | Deep-copy into agent dirs instead of symlinks |
| `-a`, `--agent <name>` | Specific agent to wire up (repeatable) |
| `-y`, `--yes` | Skip interactive prompts |

Spawns the creator agent (e.g. `claude -p <prompt>`) in a temp directory, expects it to emit a `SKILL.md`, and installs the result via the same pipeline as `add`. The skill is registered with `source: "local"` so it appears in `list`/`remove` like any other entry. Without `--name`, the description is slugified into a kebab-case name; duplicate names are rejected before the creator is invoked.

### `config`

```bash
skills config <key> <op> [value...]
```

| Key + op | Effect |
|---|---|
| `show` | Pretty-print the entire config as JSON |
| `default_agents list` | Print each default agent name |
| `default_agents add <agent>` | Append an agent (must already be defined under `agents`) |
| `default_agents remove <agent>` | Drop an agent from the defaults |
| `default_agents set <a,b,c>` or `set <a> <b> <c>` | Replace the default list |
| `store.global set <path>` | Override the global store path |
| `store.project set <path>` | Override the per-project store path |
| `agents list` | Render the agent table |
| `agents add <name> <global_dir> <project_dir>` | Register a new agent |
| `agents remove <name>` | Unregister; cascades to `default_agents` if present |

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
  "$schema": "https://raw.githubusercontent.com/smartcrabai/skills/main/schemas/config.schema.json",
  "schema": 1,
  "store": {
    "global": "~/.local/share/smartcrab-skills/store",
    "project": ".smartcrab-skills/store"
  },
  "default_agents": ["claude-code"],
  "default_creator": "claude-code",
  "agents": [
    { "name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills" },
    { "name": "opencode",    "global_dir": "${XDG_CONFIG_HOME:-~/.config}/opencode/skills", "project_dir": ".agents/skills" }
  ]
}
```

Path expansion supports `~`, `$VAR`, and `${VAR:-default}`.

### JSON Schema

Both `config.json` and `skills.json` ship with JSON Schema definitions for editor auto-completion and validation:

| File | Schema |
|---|---|
| `config.json` | [`schemas/config.schema.json`](schemas/config.schema.json) |
| `skills.json` | [`schemas/skills.schema.json`](schemas/skills.schema.json) |

Add the `$schema` key to your `config.json` to enable IDE support:

```json
{
  "$schema": "https://raw.githubusercontent.com/smartcrabai/skills/main/schemas/config.schema.json",
  "schema": 1,
  ...
}
```

## Environment variables

| Variable | Effect |
|---|---|
| `XDG_CONFIG_HOME` | Where `config.json` and `skills.json` live (default: `~/.config/smartcrab-skills`) |
| `XDG_DATA_HOME` | Where the master store lives (default: `~/.local/share/smartcrab-skills/store`) |
| `SKILLS_SEARCH_API` | Override the `find` endpoint (default: `https://skills.sh/api/search`) |

## License

MIT
