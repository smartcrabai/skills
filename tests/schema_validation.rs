use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

const CONFIG_SCHEMA: &str = "config.schema.json";
const SKILLS_SCHEMA: &str = "skills.schema.json";

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
    let errors: Vec<_> = validator.iter_errors(instance).collect();
    assert!(!errors.is_empty(), "{reason}");
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
    assert_valid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 1,
            "store": {
                "global": "~/.local/share/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "default_agents": ["claude-code"],
            "agents": [
                {
                    "name": "claude-code",
                    "global_dir": "~/.claude/skills",
                    "project_dir": ".claude/skills"
                }
            ]
        }),
    );
}

#[test]
fn valid_config_with_multiple_agents_passes_validation() {
    assert_valid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 1,
            "store": {
                "global": "${XDG_DATA_HOME:-~/.local/share}/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "default_agents": ["claude-code", "opencode"],
            "agents": [
                {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"},
                {"name": "opencode", "global_dir": "${XDG_CONFIG_HOME:-~/.config}/opencode/skills", "project_dir": ".agents/skills"}
            ]
        }),
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
                    "store_path": "/home/user/.local/share/smartcrab-skills/store/global/example-skill",
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
                    "store_path": "/home/user/projects/myapp/.smartcrab-skills/store/project/project-skill",
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
    assert_invalid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 1,
            "store": {
                "global": "~/.local/share/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "agents": [
                {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"}
            ]
        }),
        "Config missing 'default_agents' should produce validation errors",
    );
}

#[test]
fn config_rejects_wrong_schema_type() {
    assert_invalid(
        CONFIG_SCHEMA,
        &json!({
            "schema": "not-a-number",
            "store": {
                "global": "~/.local/share/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "default_agents": ["claude-code"],
            "agents": [
                {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"}
            ]
        }),
        "Config with string 'schema' field should produce validation errors",
    );
}

#[test]
fn config_rejects_schema_const_violation() {
    assert_invalid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 2,
            "store": {
                "global": "~/.local/share/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "default_agents": ["claude-code"],
            "agents": [
                {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"}
            ]
        }),
        "Config with schema != 1 should produce validation errors",
    );
}

#[test]
fn config_rejects_agent_missing_required_field() {
    assert_invalid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 1,
            "store": {
                "global": "~/.local/share/smartcrab-skills/store",
                "project": ".smartcrab-skills/store"
            },
            "default_agents": ["claude-code"],
            "agents": [
                {"name": "claude-code"}
            ]
        }),
        "Agent missing 'global_dir' and 'project_dir' should produce validation errors",
    );
}

#[test]
fn config_rejects_store_missing_required_field() {
    assert_invalid(
        CONFIG_SCHEMA,
        &json!({
            "schema": 1,
            "store": {
                "global": "~/.local/share/smartcrab-skills/store"
            },
            "default_agents": ["claude-code"],
            "agents": [
                {"name": "claude-code", "global_dir": "~/.claude/skills", "project_dir": ".claude/skills"}
            ]
        }),
        "Store missing 'project' should produce validation errors",
    );
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
