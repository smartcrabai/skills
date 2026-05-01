//! `smartcrabai/skills` — Rust port of `vercel-labs/skills`.
//!
//! Public modules expose configuration, registry, GitHub fetch, install
//! plumbing, and the clap-based CLI dispatcher.
pub mod agents;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod github;
pub mod install;
pub mod registry;
pub mod ui;
