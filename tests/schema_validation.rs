use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

const CONFIG_SCHEMA: &str = "config.schema.json";
const SKILLS_SCHEMA: &str = "skills.schema.json";
const SKILLS_LOCK_SCHEMA: &str = "skills-lock.schema.json";

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas")
}

fn load_schema(name: &str) -> Value {
    let path = schemas_dir().join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("Invalid JSON in {}: {e}", path.display()))
}

fn compile_schema(name: &str) -> jsonschema::Validator {
    let schema = load_schema(name);
    jsonschema::draft202012::new(&schema)
        .unwrap_or_else(|e| panic!("Failed to compile schema {name}: {e}"))
}

fn assert_valid(schema: &str, instance: &Value) {
    let validator = compile_schema(schema);
    let errors: Vec<_> = validator.iter_errors(instance).collect();
    assert!(errors.is_empty(), "Expected valid, got: {errors:#?}");
}

fn assert_invalid(schema: &str, instance: &Value, reason: &str) {
    let validator = compile_schema(schema);
    assert!(!validator.is_valid(instance), "{reason}");
}

fn base_config() -> Value {
    json!({
        "schema": 1,
        "store": {
            "global": "~/.local/share/smartcrab-skills/store",
            "project": ".smartcrab-skills/store"
        },
        "default_agents": ["claude-code"],
        "agents": [
            {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"}
        ]
    })
}

#[test]
fn config_schema_is_valid_json_schema() {
    let schema = load_schema(CONFIG_SCHEMA);
    jsonschema::meta::validate(&schema).unwrap_or_else(|e| {
        panic!("{CONFIG_SCHEMA} is not a valid JSON Schema: {e}");
    });
}

#[test]
fn skills_schema_is_valid_json_schema() {
    let schema = load_schema(SKILLS_SCHEMA);
    jsonschema::meta::validate(&schema).unwrap_or_else(|e| {
        panic!("{SKILLS_SCHEMA} is not a valid JSON Schema: {e}");
    });
}

#[test]
fn valid_config_passes_validation() {
    assert_valid(CONFIG_SCHEMA, &base_config());
}

#[test]
fn valid_config_with_multiple_agents_passes_validation() {
    let mut config = base_config();
    config["store"]["global"] = json!("${XDG_DATA_HOME:-~/.local/share}/smartcrab-skills/store");
    config["default_agents"] = json!(["claude-code", "opencode"]);
    config["agents"] = json!([
        {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"},
        {"name": "opencode", "global_dir": "${XDG_CONFIG_HOME:-~/.config}/opencode/skills", "project_dir": ".agents/skills"}
    ]);
    assert_valid(CONFIG_SCHEMA, &config);
}

#[test]
fn valid_config_with_default_creator_passes_validation() {
    let mut config = base_config();
    config["default_creator"] = json!("claude-code");
    assert_valid(CONFIG_SCHEMA, &config);
}

#[test]
fn valid_config_with_schema_key_passes_validation() {
    let mut config = base_config();
    config["$schema"] = json!(
        "https://raw.githubusercontent.com/smartcrabai/skills/main/schemas/config.schema.json"
    );
    assert_valid(CONFIG_SCHEMA, &config);
}

#[test]
fn config_rejects_unknown_top_level_key() {
    let mut config = base_config();
    config["unknown_field"] = json!("should-be-rejected");
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Config with unknown top-level key should produce validation errors",
    );
}

#[test]
fn valid_skills_passes_validation() {
    assert_valid(
        SKILLS_SCHEMA,
        &json!({
            "version": 1,
            "skills": [
                {
                    "name": "example-skill",
                    "source": "owner/repo/sub/path",
                    "ref": null,
                    "commit": "abc123def456789012345678901234567890abcd",
                    "scope": "global",
                    "project_path": null,
                    "method": "symlink",
                    "agents": ["claude-code", "opencode"],
                    "store_path": "/home/user/.local/share/smartcrab-skills/store/example-skill",
                    "installed_at": "2024-01-15T10:30:00Z",
                    "updated_at": "2024-01-15T10:30:00Z"
                }
            ]
        }),
    );
}

#[test]
fn valid_skills_with_project_scope_passes_validation() {
    assert_valid(
        SKILLS_SCHEMA,
        &json!({
            "version": 1,
            "skills": [
                {
                    "name": "project-skill",
                    "source": "owner/repo",
                    "ref": "main",
                    "commit": "fedcba9876543210fedcba9876543210fedcba98",
                    "scope": "project",
                    "project_path": "/home/user/projects/myapp",
                    "method": "copy",
                    "agents": ["claude-code"],
                    "store_path": "/home/user/.local/share/smartcrab-skills/store/project-skill",
                    "installed_at": "2024-06-01T12:00:00+09:00",
                    "updated_at": "2024-06-10T08:45:30.123456789Z"
                }
            ]
        }),
    );
}

#[test]
fn skills_passes_with_only_required_fields() {
    assert_valid(
        SKILLS_SCHEMA,
        &json!({
            "version": 1,
            "skills": [
                {
                    "name": "test",
                    "source": "owner/repo",
                    "commit": "abc123def456789012345678901234567890abcd",
                    "scope": "global",
                    "method": "symlink",
                    "agents": [],
                    "store_path": "/tmp/test",
                    "installed_at": "2024-01-01T00:00:00Z",
                    "updated_at": "2024-01-01T00:00:00Z"
                }
            ]
        }),
    );
}

#[test]
fn config_rejects_missing_required_field() {
    let mut config = base_config();
    let Some(obj) = config.as_object_mut() else {
        panic!("base_config returns object")
    };
    obj.remove("default_agents");
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Config missing 'default_agents' should produce validation errors",
    );
}

#[test]
fn config_rejects_wrong_schema_type() {
    let mut config = base_config();
    config["schema"] = json!("not-a-number");
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Config with string 'schema' field should produce validation errors",
    );
}

#[test]
fn config_rejects_schema_const_violation() {
    let mut config = base_config();
    config["schema"] = json!(2);
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Config with schema != 1 should produce validation errors",
    );
}

#[test]
fn config_rejects_agent_missing_required_field() {
    let mut config = base_config();
    config["agents"] = json!([{"name": "claude-code"}]);
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Agent missing 'global_dir' and 'project_dir' should produce validation errors",
    );
}

#[test]
fn config_rejects_store_missing_required_field() {
    let mut config = base_config();
    config["store"] = json!({"project": ".smartcrab-skills/store"});
    assert_invalid(
        CONFIG_SCHEMA,
        &config,
        "Store missing 'global' should produce validation errors",
    );
}

#[test]
fn config_without_legacy_project_store_passes_validation() {
    let mut config = base_config();
    config["store"] = json!({"global": "~/.local/share/smartcrab-skills/store"});
    assert_valid(CONFIG_SCHEMA, &config);
}

#[test]
fn skills_rejects_missing_version() {
    assert_invalid(
        SKILLS_SCHEMA,
        &json!({
            "skills": []
        }),
        "Skills missing 'version' should produce validation errors",
    );
}

#[test]
fn skills_rejects_invalid_scope_value() {
    assert_invalid(
        SKILLS_SCHEMA,
        &json!({
            "version": 1,
            "skills": [
                {
                    "name": "test",
                    "source": "owner/repo",
                    "ref": null,
                    "commit": "abc123def456789012345678901234567890abcd",
                    "scope": "INVALID",
                    "project_path": null,
                    "method": "symlink",
                    "agents": [],
                    "store_path": "/tmp/test",
                    "installed_at": "2024-01-01T00:00:00Z",
                    "updated_at": "2024-01-01T00:00:00Z"
                }
            ]
        }),
        "Skill with scope='INVALID' should produce validation errors",
    );
}

#[test]
fn skills_rejects_invalid_method_value() {
    assert_invalid(
        SKILLS_SCHEMA,
        &json!({
            "version": 1,
            "skills": [
                {
                    "name": "test",
                    "source": "owner/repo",
                    "ref": null,
                    "commit": "abc123def456789012345678901234567890abcd",
                    "scope": "global",
                    "project_path": null,
                    "method": "hardlink",
                    "agents": [],
                    "store_path": "/tmp/test",
                    "installed_at": "2024-01-01T00:00:00Z",
                    "updated_at": "2024-01-01T00:00:00Z"
                }
            ]
        }),
        "Skill with method='hardlink' should produce validation errors",
    );
}

#[test]
fn skills_rejects_wrong_version_type() {
    assert_invalid(
        SKILLS_SCHEMA,
        &json!({
            "version": "one",
            "skills": []
        }),
        "Skills with string 'version' should produce validation errors",
    );
}

// ---------------------------------------------------------------------------
// skills-lock.schema.json
// ---------------------------------------------------------------------------

#[test]
fn skills_lock_schema_is_valid_json_schema() {
    let schema = load_schema(SKILLS_LOCK_SCHEMA);
    jsonschema::meta::validate(&schema).unwrap_or_else(|e| {
        panic!("{SKILLS_LOCK_SCHEMA} is not a valid JSON Schema: {e}");
    });
}

#[test]
fn valid_lock_passes_validation() {
    assert_valid(
        SKILLS_LOCK_SCHEMA,
        &json!({
            "version": 1,
            "skills": {
                "from-github": {
                    "source": "owner/repo/sub",
                    "ref": "main",
                    "sourceType": "github",
                    "commit": "abc123def456789012345678901234567890abcd",
                    "agents": ["claude-code"]
                },
                "from-git": {
                    "source": "git@example.com:org/repo.git",
                    "ref": null,
                    "sourceType": "git",
                    "commit": "fedcba9876543210fedcba9876543210fedcba98",
                    "agents": ["claude-code", "opencode"]
                },
                "from-local": {
                    "source": "/abs/path/to/skill",
                    "sourceType": "local",
                    "commit": null,
                    "agents": ["claude-code"]
                }
            }
        }),
    );
}

#[test]
fn empty_lock_skills_map_passes_validation() {
    assert_valid(SKILLS_LOCK_SCHEMA, &json!({ "version": 1, "skills": {} }));
}

#[test]
fn lock_rejects_missing_version() {
    assert_invalid(
        SKILLS_LOCK_SCHEMA,
        &json!({ "skills": {} }),
        "Lock missing 'version' should produce validation errors",
    );
}

#[test]
fn lock_rejects_unknown_source_type() {
    assert_invalid(
        SKILLS_LOCK_SCHEMA,
        &json!({
            "version": 1,
            "skills": {
                "x": {
                    "source": "owner/repo",
                    "sourceType": "npm",
                    "agents": []
                }
            }
        }),
        "Lock with sourceType='npm' should produce validation errors",
    );
}

#[test]
fn lock_rejects_entry_missing_required_field() {
    assert_invalid(
        SKILLS_LOCK_SCHEMA,
        &json!({
            "version": 1,
            "skills": {
                "x": {
                    "sourceType": "github",
                    "agents": []
                }
            }
        }),
        "Lock entry missing 'source' should produce validation errors",
    );
}

#[test]
fn lock_rejects_wrong_version_value() {
    assert_invalid(
        SKILLS_LOCK_SCHEMA,
        &json!({ "version": 2, "skills": {} }),
        "Lock with version != 1 should produce validation errors",
    );
}
