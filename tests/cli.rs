//! Integration tests for the `skills` CLI.
//!
//! Each test spawns the compiled `skills` binary with a fresh
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `HOME`, and working directory pointing
//! at temporary directories. State is therefore isolated per test, and tests
//! are safe to run in parallel.

use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::Utc;
use tempfile::TempDir;

use skills::registry::{InstalledSkill, Method, Registry, Scope};

type TestResult = std::result::Result<(), Box<dyn StdError>>;

const BIN: &str = env!("CARGO_BIN_EXE_skills");

struct Env {
    config_home: TempDir,
    data_home: TempDir,
    home: TempDir,
    cwd: TempDir,
    /// `http://127.0.0.1:<port>` where `<port>` was bound and immediately
    /// released, so a follow-up connection reliably gets ECONNREFUSED. Used to
    /// force `find` into its local-registry fallback without depending on a
    /// hardcoded "should be unused" port like 1, which can hang on hardened
    /// hosts where the kernel silently drops the SYN.
    bogus_search_url: String,
}

impl Env {
    fn new() -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(Self {
            config_home: tempfile::tempdir()?,
            data_home: tempfile::tempdir()?,
            home: tempfile::tempdir()?,
            cwd: tempfile::tempdir()?,
            bogus_search_url: format!("http://127.0.0.1:{port}"),
        })
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.env("XDG_CONFIG_HOME", self.config_home.path());
        cmd.env("XDG_DATA_HOME", self.data_home.path());
        cmd.env("HOME", self.home.path());
        cmd.env("SKILLS_SEARCH_API", &self.bogus_search_url);
        cmd.current_dir(self.cwd.path());
        cmd
    }

    fn config_path(&self) -> PathBuf {
        self.config_home.path().join("smartcrab-skills/config.json")
    }

    fn registry_path(&self) -> PathBuf {
        self.config_home.path().join("smartcrab-skills/skills.json")
    }

    fn global_store(&self, name: &str) -> PathBuf {
        self.data_home
            .path()
            .join("smartcrab-skills/store")
            .join(name)
    }

    fn write_registry(&self, registry: &Registry) -> Result<(), Box<dyn StdError>> {
        let path = self.registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_vec_pretty(registry)?)?;
        Ok(())
    }

    fn read_config(&self) -> Result<serde_json::Value, Box<dyn StdError>> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.config_path(),
        )?)?)
    }
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_ok(out: &Output) -> Result<(), Box<dyn StdError>> {
    if !out.status.success() {
        return Err(format!(
            "command failed (status {:?})\nstdout: {}\nstderr: {}",
            out.status.code(),
            stdout_of(out),
            stderr_of(out)
        )
        .into());
    }
    Ok(())
}

fn global_skill(name: &str, store: PathBuf) -> InstalledSkill {
    let now = Utc::now();
    InstalledSkill {
        name: name.to_string(),
        source: format!("owner/repo/{name}"),
        ref_: None,
        commit: "deadbeefcafe".to_string(),
        scope: Scope::Global,
        project_path: None,
        method: Method::Symlink,
        agents: vec!["claude-code".to_string()],
        store_path: store,
        installed_at: now,
        updated_at: now,
    }
}

fn project_skill(name: &str, project_root: &Path, store: PathBuf) -> InstalledSkill {
    InstalledSkill {
        scope: Scope::Project,
        project_path: Some(project_root.to_path_buf()),
        ..global_skill(name, store)
    }
}

#[test]
fn init_with_name_creates_subdir_skill_md() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().args(["init", "my-skill"]).output()?;
    assert_ok(&out)?;
    let body = fs::read_to_string(env.cwd.path().join("my-skill/SKILL.md"))?;
    assert!(body.contains("name: my-skill"), "frontmatter: {body}");
    assert!(body.contains("# my-skill"), "heading: {body}");
    Ok(())
}

#[test]
fn init_without_name_uses_cwd_basename() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().arg("init").output()?;
    assert_ok(&out)?;
    let basename = env
        .cwd
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("tempdir basename not utf-8")?;
    let body = fs::read_to_string(env.cwd.path().join("SKILL.md"))?;
    assert!(
        body.contains(&format!("name: {basename}")),
        "frontmatter should reference {basename}: {body}"
    );
    Ok(())
}

#[test]
fn init_refuses_to_overwrite_existing_skill_md() -> TestResult {
    let env = Env::new()?;
    let target = env.cwd.path().join("SKILL.md");
    fs::write(&target, b"# existing")?;
    let out = env.cmd().arg("init").output()?;
    assert!(!out.status.success(), "init should fail when target exists");
    let err = stderr_of(&out);
    assert!(err.contains("already exists"), "stderr: {err}");
    assert_eq!(fs::read_to_string(&target)?, "# existing");
    Ok(())
}

#[test]
fn config_show_emits_json_with_default_claude_code() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().args(["config", "show"]).output()?;
    assert_ok(&out)?;
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out))?;
    let defaults = parsed["default_agents"]
        .as_array()
        .ok_or("default_agents missing or not array")?;
    assert!(
        defaults.iter().any(|v| v == "claude-code"),
        "expected claude-code in default_agents: {defaults:?}"
    );
    assert!(
        env.config_path().is_file(),
        "first run should write config.json"
    );
    Ok(())
}

fn json_array_contains<F>(
    value: &serde_json::Value,
    key: &str,
    pred: F,
) -> Result<bool, Box<dyn StdError>>
where
    F: FnMut(&serde_json::Value) -> bool,
{
    Ok(value[key]
        .as_array()
        .ok_or_else(|| format!("{key} not array"))?
        .iter()
        .any(pred))
}

#[test]
fn config_default_agents_add_then_remove_persists_to_disk() -> TestResult {
    let env = Env::new()?;

    assert_ok(
        &env.cmd()
            .args(["config", "default_agents", "add", "codex"])
            .output()?,
    )?;
    assert!(
        json_array_contains(&env.read_config()?, "default_agents", |v| v == "codex")?,
        "codex should have been added"
    );

    assert_ok(
        &env.cmd()
            .args(["config", "default_agents", "remove", "codex"])
            .output()?,
    )?;
    assert!(
        !json_array_contains(&env.read_config()?, "default_agents", |v| v == "codex")?,
        "codex should have been removed"
    );
    Ok(())
}

#[test]
fn config_default_agents_add_unknown_agent_fails() -> TestResult {
    let env = Env::new()?;
    let out = env
        .cmd()
        .args(["config", "default_agents", "add", "no-such-agent"])
        .output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(err.contains("unknown agent"), "stderr: {err}");
    Ok(())
}

#[test]
fn config_agents_add_then_remove_round_trips() -> TestResult {
    let env = Env::new()?;

    assert_ok(
        &env.cmd()
            .args([
                "config",
                "agents",
                "add",
                "demo",
                "~/.demo/skills",
                ".demo/skills",
            ])
            .output()?,
    )?;
    assert!(
        json_array_contains(&env.read_config()?, "agents", |a| a["name"] == "demo")?,
        "demo should be present"
    );

    assert_ok(
        &env.cmd()
            .args(["config", "agents", "remove", "demo"])
            .output()?,
    )?;
    assert!(
        !json_array_contains(&env.read_config()?, "agents", |a| a["name"] == "demo")?,
        "demo should be gone"
    );
    Ok(())
}

#[test]
fn config_unknown_op_returns_usage_error() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().args(["config", "bogus", "op"]).output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(err.contains("usage: skills config"), "stderr: {err}");
    Ok(())
}

#[test]
fn list_empty_registry_prints_friendly_message() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().arg("list").output()?;
    assert_ok(&out)?;
    let body = stdout_of(&out);
    assert!(body.contains("No skills installed"), "stdout: {body}");
    Ok(())
}

#[test]
fn list_renders_table_with_seeded_global_skill() -> TestResult {
    let env = Env::new()?;
    let mut reg = Registry::default();
    reg.add(global_skill("foo", env.global_store("foo")));
    env.write_registry(&reg)?;

    let out = env.cmd().arg("list").output()?;
    assert_ok(&out)?;
    let body = stdout_of(&out);
    for needle in ["NAME", "foo", "global", "symlink", "claude-code"] {
        assert!(
            body.contains(needle),
            "expected {needle:?} in output: {body}"
        );
    }
    Ok(())
}

#[test]
fn list_json_emits_serializable_array() -> TestResult {
    let env = Env::new()?;
    let mut reg = Registry::default();
    reg.add(global_skill("foo", env.global_store("foo")));
    env.write_registry(&reg)?;

    let out = env.cmd().args(["list", "--json"]).output()?;
    assert_ok(&out)?;
    let v: serde_json::Value = serde_json::from_str(&stdout_of(&out))?;
    let arr = v.as_array().ok_or("expected JSON array")?;
    assert_eq!(arr.len(), 1, "{arr:?}");
    assert_eq!(arr[0]["name"], "foo");
    assert_eq!(arr[0]["scope"], "global");
    Ok(())
}

#[test]
fn list_global_filter_excludes_project_skills() -> TestResult {
    let env = Env::new()?;
    let mut reg = Registry::default();
    reg.add(global_skill("global-only", env.global_store("global-only")));
    reg.add(project_skill(
        "proj-only",
        env.cwd.path(),
        env.global_store("proj-only"),
    ));
    env.write_registry(&reg)?;

    let out = env.cmd().args(["list", "--global"]).output()?;
    assert_ok(&out)?;
    let body = stdout_of(&out);
    assert!(body.contains("global-only"), "{body}");
    assert!(
        !body.contains("proj-only"),
        "project skill should be filtered out: {body}"
    );
    Ok(())
}

#[test]
fn find_falls_back_to_local_registry_when_remote_unreachable() -> TestResult {
    let env = Env::new()?;
    let mut reg = Registry::default();
    reg.add(global_skill("find-me", env.global_store("find-me")));
    env.write_registry(&reg)?;

    let out = env.cmd().args(["find", "find"]).output()?;
    assert_ok(&out)?;
    let body = stdout_of(&out);
    let err = stderr_of(&out);
    assert!(
        body.contains("find-me"),
        "stdout should contain match: {body}"
    );
    assert!(
        err.contains("falling back"),
        "stderr should announce fallback: {err}"
    );
    Ok(())
}

#[test]
fn find_no_match_prints_friendly_message() -> TestResult {
    let env = Env::new()?;
    env.write_registry(&Registry::default())?;
    let out = env.cmd().args(["find", "nothing-matches-this"]).output()?;
    assert_ok(&out)?;
    let body = stdout_of(&out);
    assert!(body.contains("No matching skills"), "stdout: {body}");
    Ok(())
}

#[test]
fn remove_deletes_master_agent_link_and_registry_entry() -> TestResult {
    let env = Env::new()?;

    let master = env.global_store("foo");
    fs::create_dir_all(&master)?;
    fs::write(master.join("SKILL.md"), b"# foo")?;

    // The default `claude-code` agent's global_dir is `~/.claude/skills`, which
    // expands against the per-test `HOME` we set above.
    let agent_dir = env.home.path().join(".claude/skills");
    fs::create_dir_all(&agent_dir)?;
    let link = agent_dir.join("foo");
    std::os::unix::fs::symlink(&master, &link)?;

    let mut reg = Registry::default();
    reg.add(global_skill("foo", master.clone()));
    env.write_registry(&reg)?;

    let out = env.cmd().args(["remove", "foo", "-g", "-y"]).output()?;
    assert_ok(&out)?;

    assert!(!master.exists(), "master should be deleted");
    assert!(
        fs::symlink_metadata(&link).is_err(),
        "agent link should be deleted (symlink_metadata should error)"
    );

    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(env.registry_path())?)?;
    let skills = saved["skills"].as_array().ok_or("skills not array")?;
    assert!(skills.is_empty(), "registry should be empty: {skills:?}");
    Ok(())
}

#[test]
fn remove_unknown_skill_errors_out() -> TestResult {
    let env = Env::new()?;
    env.write_registry(&Registry::default())?;
    let out = env.cmd().args(["remove", "nope", "-g", "-y"]).output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("skill not found") || err.contains("nope"),
        "stderr: {err}"
    );
    Ok(())
}

#[test]
fn add_in_non_tty_without_scope_flag_errors() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().args(["add", "owner/repo/path", "-y"]).output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("--global") && err.contains("--project"),
        "stderr should ask for a scope flag: {err}"
    );
    Ok(())
}

#[test]
fn add_invalid_source_errors_before_network() -> TestResult {
    let env = Env::new()?;
    let out = env.cmd().args(["add", "solo", "-g", "-y"]).output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(err.contains("invalid source"), "stderr: {err}");
    Ok(())
}

#[test]
fn add_local_path_installs_into_master_and_registry() -> TestResult {
    let env = Env::new()?;

    // Build a local skill directory: <cwd>/my-local-skill/SKILL.md
    let src_dir = env.cwd.path().join("my-local-skill");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("SKILL.md"),
        "---\nname: my-local-skill\ndescription: a local test skill\n---\n# my-local-skill\n",
    )?;

    let out = env
        .cmd()
        .args(["add", "./my-local-skill", "-g", "-y"])
        .output()?;
    assert_ok(&out)?;

    // Master copy was created with the SKILL.md
    let master = env.global_store("my-local-skill");
    assert!(
        master.is_dir(),
        "master dir should exist: {}",
        master.display()
    );
    assert!(master.join("SKILL.md").is_file());

    // Registry contains the entry with the canonical absolute source.
    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(env.registry_path())?)?;
    let skills = saved["skills"].as_array().ok_or("skills not array")?;
    assert_eq!(skills.len(), 1, "registry: {skills:?}");
    assert_eq!(skills[0]["name"], "my-local-skill");
    let source = skills[0]["source"].as_str().ok_or("source not str")?;
    assert!(
        source.ends_with("my-local-skill"),
        "source should end with skill dir: {source}"
    );
    assert!(
        std::path::Path::new(source).is_absolute(),
        "source should be an absolute path: {source}"
    );

    // The default `claude-code` agent gets a symlink in ~/.claude/skills.
    let link = env.home.path().join(".claude/skills/my-local-skill");
    assert!(
        fs::symlink_metadata(&link).is_ok(),
        "agent link should exist: {}",
        link.display()
    );
    Ok(())
}

#[test]
fn add_same_local_skill_globally_then_project_shares_master() -> TestResult {
    let env = Env::new()?;

    // Local skill source on disk.
    let src_dir = env.cwd.path().join("shared-skill");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("SKILL.md"),
        "---\nname: shared-skill\ndescription: shared\n---\n# shared-skill\n",
    )?;

    // Install globally, then again into the (same) project. Both must end up
    // sharing the user-level master under $XDG_DATA_HOME/.../store/<name>.
    assert_ok(
        &env.cmd()
            .args(["add", "./shared-skill", "-g", "-y"])
            .output()?,
    )?;
    assert_ok(
        &env.cmd()
            .args(["add", "./shared-skill", "-p", "-y"])
            .output()?,
    )?;

    let master = env.global_store("shared-skill");
    assert!(
        master.is_dir(),
        "shared master should exist: {}",
        master.display()
    );
    // No project-local master directory should have been created.
    let legacy_project_store = env.cwd.path().join("smartcrab-skills/store");
    assert!(
        !legacy_project_store.exists(),
        "project scope must not create a project-local store: {}",
        legacy_project_store.display()
    );

    // Both registry entries point at the same shared store_path.
    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(env.registry_path())?)?;
    let skills = saved["skills"].as_array().ok_or("skills not array")?;
    assert_eq!(skills.len(), 2, "{skills:?}");
    let store_paths: std::collections::HashSet<&str> = skills
        .iter()
        .filter_map(|s| s["store_path"].as_str())
        .collect();
    assert_eq!(
        store_paths.len(),
        1,
        "both entries should share one master: {store_paths:?}"
    );
    Ok(())
}

#[test]
fn add_same_name_different_source_is_rejected() -> TestResult {
    let env = Env::new()?;

    let a = env.cwd.path().join("a");
    let b = env.cwd.path().join("b");
    fs::create_dir_all(&a)?;
    fs::create_dir_all(&b)?;
    // Both directories carry SKILL.md frontmatter with the same `name:` but
    // they live at different absolute paths, so they're different sources.
    let body = "---\nname: same-name\ndescription: x\n---\n# same-name\n";
    fs::write(a.join("SKILL.md"), body)?;
    fs::write(b.join("SKILL.md"), body)?;

    assert_ok(&env.cmd().args(["add", "./a", "-g", "-y"]).output()?)?;

    let out = env.cmd().args(["add", "./b", "-p", "-y"]).output()?;
    assert!(!out.status.success(), "second add must be rejected");
    let err = stderr_of(&out);
    assert!(
        err.contains("already installed") && err.contains("same-name"),
        "stderr should explain the master conflict: {err}"
    );
    Ok(())
}

#[test]
fn remove_one_sharer_keeps_master_for_others() -> TestResult {
    let env = Env::new()?;

    let src_dir = env.cwd.path().join("shared");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("SKILL.md"),
        "---\nname: shared\ndescription: x\n---\n# shared\n",
    )?;

    assert_ok(&env.cmd().args(["add", "./shared", "-g", "-y"]).output()?)?;
    assert_ok(&env.cmd().args(["add", "./shared", "-p", "-y"]).output()?)?;

    let master = env.global_store("shared");
    assert!(master.is_dir());

    // Remove the global entry; the project entry still references the master.
    assert_ok(&env.cmd().args(["remove", "shared", "-g", "-y"]).output()?)?;
    assert!(
        master.is_dir(),
        "master must survive while another entry references it"
    );

    // Removing the last entry should clean the master up.
    assert_ok(&env.cmd().args(["remove", "shared", "-p", "-y"]).output()?)?;
    assert!(
        !master.exists(),
        "master should be deleted once unreferenced"
    );
    Ok(())
}

#[test]
fn add_local_path_missing_skill_md_errors() -> TestResult {
    let env = Env::new()?;
    let src_dir = env.cwd.path().join("not-a-skill");
    fs::create_dir_all(&src_dir)?;

    let out = env
        .cmd()
        .args(["add", "./not-a-skill", "-g", "-y"])
        .output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("SKILL.md not found"),
        "stderr should mention missing SKILL.md: {err}"
    );
    Ok(())
}

/// Build a local "collection" directory with two SKILL.md files nested
/// underneath, returning its absolute path. Mirrors the
/// `expo/skills` shape (no SKILL.md at the root, real skills two levels down).
fn make_local_collection(env: &Env) -> Result<PathBuf, Box<dyn StdError>> {
    let root = env.cwd.path().join("collection");
    let alpha = root.join("plugins/foo/skills/alpha");
    let beta = root.join("plugins/foo/skills/beta");
    fs::create_dir_all(&alpha)?;
    fs::create_dir_all(&beta)?;
    fs::write(
        alpha.join("SKILL.md"),
        "---\nname: alpha\ndescription: first\n---\n# alpha\n",
    )?;
    fs::write(
        beta.join("SKILL.md"),
        "---\nname: beta\ndescription: second\n---\n# beta\n",
    )?;
    Ok(root)
}

#[test]
fn add_local_collection_without_select_lists_skills_and_errors() -> TestResult {
    let env = Env::new()?;
    make_local_collection(&env)?;

    let out = env
        .cmd()
        .args(["add", "./collection", "-g", "-y"])
        .output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("found 2 skills") && err.contains("--skill") && err.contains("--all"),
        "stderr should explain how to disambiguate: {err}"
    );
    assert!(
        err.contains("alpha") && err.contains("beta"),
        "stderr should list discovered skill names: {err}"
    );
    Ok(())
}

#[test]
fn add_local_collection_with_skill_flag_picks_one() -> TestResult {
    let env = Env::new()?;
    make_local_collection(&env)?;

    let out = env
        .cmd()
        .args(["add", "./collection", "-g", "--skill", "alpha", "-y"])
        .output()?;
    assert_ok(&out)?;
    assert!(
        env.global_store("alpha").is_dir(),
        "alpha master should exist"
    );
    assert!(
        !env.global_store("beta").exists(),
        "beta should not have been installed"
    );

    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(env.registry_path())?)?;
    let skills = saved["skills"].as_array().ok_or("skills not array")?;
    assert_eq!(skills.len(), 1, "{skills:?}");
    assert_eq!(skills[0]["name"], "alpha");
    let source = skills[0]["source"].as_str().ok_or("source not str")?;
    assert!(
        source.ends_with("plugins/foo/skills/alpha"),
        "registered source should be the discovered skill path, got {source}"
    );
    Ok(())
}

#[test]
fn add_local_collection_with_all_installs_every_skill() -> TestResult {
    let env = Env::new()?;
    make_local_collection(&env)?;

    let out = env
        .cmd()
        .args(["add", "./collection", "-g", "--all", "-y"])
        .output()?;
    assert_ok(&out)?;
    assert!(env.global_store("alpha").is_dir());
    assert!(env.global_store("beta").is_dir());

    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(env.registry_path())?)?;
    let skills = saved["skills"].as_array().ok_or("skills not array")?;
    let names: std::collections::HashSet<_> =
        skills.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(
        names.contains("alpha") && names.contains("beta"),
        "{names:?}"
    );
    Ok(())
}

#[test]
fn add_local_collection_unknown_skill_name_errors() -> TestResult {
    let env = Env::new()?;
    make_local_collection(&env)?;

    let out = env
        .cmd()
        .args(["add", "./collection", "-g", "--skill", "no-such", "-y"])
        .output()?;
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("no-such") && err.contains("not present"),
        "stderr should explain that the requested skill isn't in the source: {err}"
    );
    Ok(())
}

#[test]
fn add_with_all_and_skill_flags_conflict() -> TestResult {
    let env = Env::new()?;
    let out = env
        .cmd()
        .args(["add", "owner/repo", "-g", "--all", "--skill", "x", "-y"])
        .output()?;
    assert!(!out.status.success(), "clap should reject the combo");
    let err = stderr_of(&out);
    assert!(
        err.contains("--all") && err.contains("--skill"),
        "stderr should mention the conflict: {err}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// create
// ---------------------------------------------------------------------------

#[test]
fn create_missing_description_shows_usage_error() -> TestResult {
    // Given: a fresh env
    let env = Env::new()?;
    // When: `create` is invoked without required description argument
    let out = env.cmd().args(["create"]).output()?;
    // Then: fails with usage error
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("usage") || err.contains("required"),
        "stderr should mention usage: {err}"
    );
    assert!(
        err.contains("DESCRIPTION") || err.contains("<DESCRIPTION>"),
        "stderr should mention missing description: {err}"
    );
    Ok(())
}

#[test]
fn create_global_and_project_conflict_exits_with_error() -> TestResult {
    // Given: a fresh env
    let env = Env::new()?;
    // When: both --global and --project are specified
    let out = env
        .cmd()
        .args(["create", "test skill", "--global", "--project"])
        .output()?;
    // Then: clap rejects it
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("--global") && err.contains("--project"),
        "conflict message: {err}"
    );
    Ok(())
}

#[test]
fn create_non_tty_without_scope_flag_errors() -> TestResult {
    // Given: a non-interactive env
    let env = Env::new()?;
    // When: create without --global or --project, but with --yes
    let out = env.cmd().args(["create", "test skill", "--yes"]).output()?;
    // Then: errors asking for scope flag
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("--global") && err.contains("--project"),
        "stderr should ask for a scope flag: {err}"
    );
    Ok(())
}

#[test]
fn create_with_unknown_agent_fails_before_creator_invocation() -> TestResult {
    // Given: a fresh env
    let env = Env::new()?;
    // When: specifying an unknown agent
    let out = env
        .cmd()
        .args([
            "create",
            "test skill",
            "--global",
            "--yes",
            "--agent",
            "no-such-agent",
        ])
        .output()?;
    // Then: fails with unknown agent error (before invoking creator)
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("unknown agent"),
        "stderr should report unknown agent: {err}"
    );
    Ok(())
}

#[test]
fn create_duplicate_skill_fails_before_creator_invocation() -> TestResult {
    // Given: a registry with an existing skill named "dup-skill"
    let env = Env::new()?;
    let mut reg = Registry::default();
    reg.add(global_skill("dup-skill", env.global_store("dup-skill")));
    env.write_registry(&reg)?;
    // When: trying to create a skill with the same explicit name
    let out = env
        .cmd()
        .args([
            "create",
            "some description text",
            "--global",
            "--yes",
            "--name",
            "dup-skill",
        ])
        .output()?;
    // Then: fails with duplicate error before invoking creator
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        err.contains("already installed") || err.contains("dup-skill"),
        "stderr should report duplicate: {err}"
    );
    Ok(())
}

#[test]
fn create_help_shows_flags() -> TestResult {
    // Given: a fresh env
    let env = Env::new()?;
    // When: `skills create --help` is invoked
    let out = env.cmd().args(["create", "--help"]).output()?;
    // Then: help text includes expected flags
    assert_ok(&out)?;
    let body = stdout_of(&out);
    for flag in [
        "--creator",
        "--name",
        "--global",
        "--project",
        "--agent",
        "--yes",
    ] {
        assert!(body.contains(flag), "help should mention {flag}: {body}");
    }
    Ok(())
}
