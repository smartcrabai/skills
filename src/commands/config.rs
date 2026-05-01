use tabled::{Table, Tabled};

use crate::agents::Agent;
use crate::cli::ConfigArgs;
use crate::config::Config;
use crate::error::{Error, Result};

const USAGE: &str = "usage: skills config <key> <op> [value...]";

/// Run the `config` subcommand.
///
/// # Errors
///
/// Returns [`Error::ConfigError`] for unknown key/op combinations or invalid
/// operands; propagates [`Error::Io`] / [`Error::Json`] from config I/O.
#[expect(
    clippy::unused_async,
    reason = "kept async to match dispatcher signature"
)]
pub async fn run(args: ConfigArgs) -> Result<()> {
    let mut cfg = Config::load()?;
    let key = args.key.as_str();
    let op = args.op.as_deref();
    let values = args.values;

    match (key, op) {
        ("show", None) => show(&cfg),
        ("default_agents", Some("list")) => {
            list_default_agents(&cfg);
            Ok(())
        }
        ("default_agents", Some("add")) => {
            default_agents_add(&mut cfg, &values)?;
            cfg.save()
        }
        ("default_agents", Some("remove")) => {
            default_agents_remove(&mut cfg, &values)?;
            cfg.save()
        }
        ("default_agents", Some("set")) => {
            default_agents_set(&mut cfg, &values)?;
            cfg.save()
        }
        ("store.global", Some("set")) => {
            store_set(&mut cfg.store.global, &values)?;
            cfg.save()
        }
        ("store.project", Some("set")) => {
            store_set(&mut cfg.store.project, &values)?;
            cfg.save()
        }
        ("agents", Some("list")) => {
            list_agents(&cfg);
            Ok(())
        }
        ("agents", Some("add")) => {
            agents_add(&mut cfg, &values)?;
            cfg.save()
        }
        ("agents", Some("remove")) => {
            agents_remove(&mut cfg, &values)?;
            cfg.save()
        }
        _ => Err(Error::ConfigError(USAGE.to_string())),
    }
}

fn show(cfg: &Config) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    println!("{json}");
    Ok(())
}

fn list_default_agents(cfg: &Config) {
    for name in &cfg.default_agents {
        println!("{name}");
    }
}

fn default_agents_add(cfg: &mut Config, values: &[String]) -> Result<()> {
    let agent = single_value(values, "default_agents add <agent>")?;
    if cfg.agent(agent).is_none() {
        return Err(Error::ConfigError(format!("unknown agent: {agent}")));
    }
    if cfg.default_agents.iter().any(|n| n == agent) {
        println!("{agent} already in default_agents");
    } else {
        cfg.default_agents.push(agent.to_string());
        println!("added {agent} to default_agents");
    }
    Ok(())
}

fn default_agents_remove(cfg: &mut Config, values: &[String]) -> Result<()> {
    let agent = single_value(values, "default_agents remove <agent>")?;
    let before = cfg.default_agents.len();
    cfg.default_agents.retain(|n| n != agent);
    if cfg.default_agents.len() == before {
        println!("{agent} not in default_agents");
    } else {
        println!("removed {agent} from default_agents");
    }
    Ok(())
}

fn default_agents_set(cfg: &mut Config, values: &[String]) -> Result<()> {
    if values.is_empty() {
        return Err(Error::ConfigError(
            "default_agents set <a,b,c> | <a> <b> <c>".to_string(),
        ));
    }
    let new_list: Vec<String> = if values.len() == 1 {
        values[0]
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect()
    } else {
        values.to_vec()
    };
    if new_list.is_empty() {
        return Err(Error::ConfigError(
            "default_agents set requires at least one agent".to_string(),
        ));
    }
    for name in &new_list {
        if cfg.agent(name).is_none() {
            return Err(Error::ConfigError(format!("unknown agent: {name}")));
        }
    }
    cfg.default_agents = new_list;
    println!("default_agents set to {}", cfg.default_agents.join(","));
    Ok(())
}

fn store_set(field: &mut String, values: &[String]) -> Result<()> {
    let path = single_value(values, "store.<global|project> set <path>")?;
    *field = path.to_string();
    println!("store path set to {path}");
    Ok(())
}

fn list_agents(cfg: &Config) {
    let rows: Vec<AgentRow> = cfg.agents.iter().map(AgentRow::from).collect();
    let table = Table::new(rows).to_string();
    println!("{table}");
}

fn agents_add(cfg: &mut Config, values: &[String]) -> Result<()> {
    let [name, global_dir, project_dir] = match values {
        [a, b, c] => [a.as_str(), b.as_str(), c.as_str()],
        _ => {
            return Err(Error::ConfigError(
                "agents add <name> <global_dir> <project_dir>".to_string(),
            ));
        }
    };
    if cfg.agent(name).is_some() {
        return Err(Error::ConfigError(format!("agent already exists: {name}")));
    }
    cfg.agents.push(Agent {
        name: name.to_string(),
        global_dir: global_dir.to_string(),
        project_dir: project_dir.to_string(),
    });
    println!("added agent {name}");
    Ok(())
}

fn agents_remove(cfg: &mut Config, values: &[String]) -> Result<()> {
    let name = single_value(values, "agents remove <name>")?;
    let before = cfg.agents.len();
    cfg.agents.retain(|a| a.name != name);
    if cfg.agents.len() == before {
        return Err(Error::ConfigError(format!("agent not found: {name}")));
    }
    let cascaded = cfg.default_agents.iter().any(|n| n == name);
    cfg.default_agents.retain(|n| n != name);
    println!("removed agent {name}");
    if cascaded {
        println!("also removed {name} from default_agents");
    }
    Ok(())
}

fn single_value<'a>(values: &'a [String], hint: &str) -> Result<&'a str> {
    match values {
        [v] => Ok(v.as_str()),
        _ => Err(Error::ConfigError(hint.to_string())),
    }
}

#[derive(Tabled)]
struct AgentRow {
    name: String,
    global_dir: String,
    project_dir: String,
}

impl From<&Agent> for AgentRow {
    fn from(a: &Agent) -> Self {
        Self {
            name: a.name.clone(),
            global_dir: a.global_dir.clone(),
            project_dir: a.project_dir.clone(),
        }
    }
}
