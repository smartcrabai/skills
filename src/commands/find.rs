use std::time::Duration;

use dialoguer::Input;
use dialoguer::theme::ColorfulTheme;
use serde::{Deserialize, Serialize};
use tabled::builder::Builder;
use tabled::settings::Style;

use crate::cli::FindArgs;
use crate::error::{Error, Result};
use crate::registry::Registry;
use crate::ui;

/// Default endpoint for the remote search API. Overridable via the
/// `SKILLS_SEARCH_API` environment variable.
const DEFAULT_SEARCH_API: &str = "https://skills.sh/api/search";

/// One row in the remote search response.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct RemoteSkill {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    installs: Option<u64>,
    source: String,
    #[serde(default)]
    description: Option<String>,
}

/// Top-level shape of `GET /api/search`.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct SearchResponse {
    skills: Vec<RemoteSkill>,
}

/// Run `skills find [query] [--json]`.
///
/// Hits the remote search API; on HTTP / parse failure falls back to a
/// case-insensitive grep over the local registry.
///
/// # Errors
///
/// Returns [`Error::ConfigError`] when no query is provided in a non-TTY
/// environment, or propagates errors from registry / IO.
pub async fn run(args: FindArgs) -> Result<()> {
    let query = resolve_query(args.query)?;
    let endpoint =
        std::env::var("SKILLS_SEARCH_API").unwrap_or_else(|_| DEFAULT_SEARCH_API.to_string());

    match remote_search(&endpoint, &query).await {
        Ok(resp) => {
            if args.json {
                print_json(&resp)?;
            } else if resp.skills.is_empty() {
                println!("No matching skills.");
            } else {
                print_remote_table(&resp.skills);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("warning: remote search failed ({e}); falling back to local registry");
            local_fallback(&query, args.json)
        }
    }
}

/// Resolve the query string, prompting on TTY if absent.
fn resolve_query(query: Option<String>) -> Result<String> {
    if let Some(q) = query {
        return Ok(q);
    }
    if !ui::is_tty() {
        return Err(Error::ConfigError(
            "query required in non-interactive mode".to_string(),
        ));
    }
    let q: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Search skills")
        .interact_text()
        .map_err(|dialoguer::Error::IO(io)| Error::Io(io))?;
    Ok(q)
}

/// Hit the remote search API and parse the JSON response.
async fn remote_search(endpoint: &str, query: &str) -> Result<SearchResponse> {
    let url = reqwest::Url::parse_with_params(endpoint, &[("q", query), ("limit", "10")])
        .map_err(|e| Error::Network(format!("invalid url: {e}")))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| Error::Network(format!("client: {e}")))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("request: {e}")))?;

    if !resp.status().is_success() {
        return Err(Error::Network(format!("http {}", resp.status())));
    }

    let body: SearchResponse = resp
        .json()
        .await
        .map_err(|e| Error::Network(format!("parse: {e}")))?;
    Ok(body)
}

/// Render the remote results as a NAME | INSTALLS | SOURCE | DESCRIPTION table.
fn print_remote_table(skills: &[RemoteSkill]) {
    let mut builder = Builder::default();
    builder.push_record(["NAME", "INSTALLS", "SOURCE", "DESCRIPTION"]);
    for s in skills {
        builder.push_record([
            s.name.clone(),
            s.installs
                .map_or_else(|| "—".to_string(), |n| n.to_string()),
            s.source.clone(),
            s.description.clone().unwrap_or_else(|| "—".to_string()),
        ]);
    }
    let mut table = builder.build();
    table.with(Style::markdown());
    println!("{table}");
}

/// Pretty-print the wire JSON, regardless of result count.
fn print_json(resp: &SearchResponse) -> Result<()> {
    let s = serde_json::to_string_pretty(resp)?;
    println!("{s}");
    Ok(())
}

/// Fall back to a case-insensitive grep over [`Registry::load`].
fn local_fallback(query: &str, json: bool) -> Result<()> {
    let registry = Registry::load()?;
    let needle = query.to_lowercase();
    let matches: Vec<RemoteSkill> = registry
        .skills
        .iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&needle) || s.source.to_lowercase().contains(&needle)
        })
        .map(|s| RemoteSkill {
            id: None,
            name: s.name.clone(),
            installs: None,
            source: s.source.clone(),
            description: None,
        })
        .collect();

    let resp = SearchResponse { skills: matches };

    if json {
        return print_json(&resp);
    }
    if resp.skills.is_empty() {
        println!("No matching skills.");
        return Ok(());
    }
    print_remote_table(&resp.skills);
    Ok(())
}
