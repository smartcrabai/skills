use std::io::IsTerminal;

use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, MultiSelect, Select};

use crate::error::{Error, Result};
use crate::registry::Scope;

/// True if both stdin and stderr are TTYs.
#[must_use]
pub fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Prompt the user to choose between Global and Project scope.
///
/// `default` highlights an initial item.
///
/// # Errors
///
/// Returns [`Error::TtyRequired`] in non-interactive contexts; propagates I/O errors.
pub fn select_scope(default: Option<Scope>) -> Result<Scope> {
    if !is_tty() {
        return Err(Error::TtyRequired);
    }
    let items = ["Global (~/...)", "Project (./...)"];
    let initial = match default {
        Some(Scope::Project) => 1,
        _ => 0,
    };
    let chosen = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Where should this skill be installed?")
        .items(items.as_slice())
        .default(initial)
        .interact()
        .map_err(io_err)?;
    Ok(if chosen == 0 {
        Scope::Global
    } else {
        Scope::Project
    })
}

/// Multi-select agents from `all`, with `defaults` pre-checked.
///
/// # Errors
///
/// Returns [`Error::TtyRequired`] in non-interactive contexts; propagates I/O errors.
pub fn multiselect_agents(all: &[String], defaults: &[String]) -> Result<Vec<String>> {
    if !is_tty() {
        return Err(Error::TtyRequired);
    }
    let checked: Vec<bool> = all
        .iter()
        .map(|n| defaults.iter().any(|d| d == n))
        .collect();
    let picks = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select agents to link this skill into")
        .items(all)
        .defaults(&checked)
        .interact()
        .map_err(io_err)?;
    Ok(picks.into_iter().map(|i| all[i].clone()).collect())
}

/// Multi-select skills from `names`. All entries pre-checked. Requires at
/// least one selection.
///
/// # Errors
///
/// Returns [`Error::TtyRequired`] in non-interactive contexts; propagates I/O
/// errors. Returns [`Error::ConfigError`] if the user picks zero items.
pub fn multiselect_skills(names: &[String]) -> Result<Vec<usize>> {
    if !is_tty() {
        return Err(Error::TtyRequired);
    }
    let checked: Vec<bool> = vec![true; names.len()];
    let picks = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Select which of the {} skill(s) to install",
            names.len()
        ))
        .items(names)
        .defaults(&checked)
        .interact()
        .map_err(io_err)?;
    if picks.is_empty() {
        return Err(Error::ConfigError(
            "no skills selected; aborting".to_string(),
        ));
    }
    Ok(picks)
}

/// Yes/no confirmation prompt.
///
/// # Errors
///
/// Returns [`Error::TtyRequired`] in non-interactive contexts; propagates I/O errors.
pub fn confirm(prompt: &str) -> Result<bool> {
    if !is_tty() {
        return Err(Error::TtyRequired);
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(io_err)
}

fn io_err(e: dialoguer::Error) -> Error {
    match e {
        dialoguer::Error::IO(io) => Error::Io(io),
    }
}
