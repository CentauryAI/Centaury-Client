//! `resume <name>` — relaunch an agent's tool continuing its previous session.
//!
//! The server knows WHO (tool, directory, owner — from registration);
//! the local disk knows WHICH session (transcript files are physically
//! client-side, per the SaaS split rule). We join the two and delegate the
//! actual spawn to `launch`, so identity/env setup stays on one path.

use crate::commands::launch::{self, LaunchArgs};
use crate::transcript;

#[derive(clap::Parser, Debug)]
#[command(about = "Relaunch an agent continuing its previous session")]
pub struct ResumeArgs {
    /// Agent name as shown in `list`
    pub name: String,
    /// Open in a new terminal window (kitty|wezterm|tmux|foot; no value = auto)
    #[arg(long, num_args = 0..=1, default_missing_value = "auto")]
    pub terminal: Option<String>,
    /// Extra args passed to the tool (after --)
    #[arg(last = true)]
    pub tool_args: Vec<String>,
}

pub async fn run(args: ResumeArgs) -> anyhow::Result<()> {
    let instances: Vec<kore_protocol::api::InstanceSummary> =
        crate::get_json("/v1/instances").await?;
    let inst = instances
        .into_iter()
        .find(|i| i.name == args.name)
        .ok_or_else(|| {
            anyhow::anyhow!("no instance '{}' in your project (see `kore-client list`)", args.name)
        })?;

    let tool = inst.tool.unwrap_or_else(|| "claude".to_string());
    // claude/gemini/omp resume by flag; codex by subcommand (`codex resume <id>`).
    // omp verified live 2026-07-08 (`omp --resume <id>` — 16.3.8 renamed pi's
    // `--session`); the other dialects stay gated on a local install (HC13).
    let resume_verb: &[&str] = match tool.as_str() {
        "claude" | "gemini" | "omp" => &["--resume"],
        "codex" => &["resume"],
        other => anyhow::bail!("resume supports claude, gemini, codex and omp ('{}' runs {other})", args.name),
    };
    let dir = inst.directory.ok_or_else(|| {
        anyhow::anyhow!("server has no directory recorded for '{}'", args.name)
    })?;

    let (session_id, path) = transcript::find_session(&tool, &args.name, &dir).ok_or_else(|| {
        anyhow::anyhow!(
            "no {tool} session for '{}' in {dir} on this machine \
             (the agent must have run here with hooks at least once)",
            args.name
        )
    })?;

    if let Some(last) = transcript::parse(&tool, &path).ok().and_then(|v| v.into_iter().last()) {
        let snippet: String = last.text.chars().take(80).collect();
        println!("resuming '{}' — last activity {} [{}] {snippet}", args.name, last.ts, last.role);
    }

    // Every tool keys sessions by cwd: resume must run where the session lived.
    std::env::set_current_dir(&dir)
        .map_err(|e| anyhow::anyhow!("cannot enter '{dir}': {e}"))?;

    let mut tool_args: Vec<String> = resume_verb.iter().map(|s| s.to_string()).collect();
    tool_args.push(session_id);
    tool_args.extend(args.tool_args);
    launch::run(LaunchArgs {
        name: Some(args.name),
        tag: inst.tag, // keep the group tag the server has on record
        project: None, // token/project routing comes from the caller's KORE_* env, same as launch
        owner: inst.owner,
        tool,
        no_hooks: false,
        terminal: args.terminal,
        count: 1,
        headless: false,
        stay: false,
        wait: None,
        role: None, // resume keeps whatever role/skills the server already has
        skills: vec![],
        tool_args,
    })
    .await
}
