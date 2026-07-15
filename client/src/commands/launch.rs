//! Summon an agent: spawn the tool CLI (claude, ...) in the current directory
//! with kore env set; the SessionStart hook self-registers the agent. No PTY
//! wrapper — hook injection replaced legacy PTY delivery.

use std::process::Command;

#[derive(clap::Parser, Debug)]
#[command(about = "Launch an AI agent connected to kore in the current directory")]
pub struct LaunchArgs {
    /// Agent name (default: derived stable name)
    #[arg(long)]
    pub name: Option<String>,
    /// Group tag (legacy): shared label, NOT the name. Stored per instance;
    /// display/addressing use "{tag}-{name}" as an alias (default: KORE_TAG)
    #[arg(long)]
    pub tag: Option<String>,
    /// Project to join (default: KORE_PROJECT env, else "default")
    #[arg(long)]
    pub project: Option<String>,
    /// Owning human's name (@bigboss resolves to them)
    #[arg(long)]
    pub owner: Option<String>,
    /// Tool CLI to run
    #[arg(long, default_value = "claude")]
    pub tool: String,
    /// Skip installing hooks into ./.claude/settings.json
    #[arg(long)]
    pub no_hooks: bool,
    /// Open the agent in a new terminal window (kitty|wezterm|tmux|foot;
    /// "split" divides the CURRENT terminal instead — errors if it can't;
    /// no value = auto-detect from env, then PATH)
    #[arg(long, num_args = 0..=1, default_missing_value = "auto")]
    pub terminal: Option<String>,
    /// How many agents to launch (names <name>-1..N; needs --terminal or --headless)
    #[arg(long, short = 'n', default_value_t = 1)]
    pub count: u8,
    /// Run without a window: detached, output to $KORE_DIR/logs/<name>.log
    #[arg(long)]
    pub headless: bool,
    /// Keep a headless agent alive between turns (DU-C1): the Stop hook waits
    /// ~forever for kore messages (86400s, like interactive) instead of the
    /// 120s default that lets one-shot -p runs exit. claude gets a seeded
    /// first prompt when none is given. Requires --headless
    #[arg(long)]
    pub stay: bool,
    /// Wait until the launched agent(s) actually registered (secs, default 60);
    /// exits non-zero listing the names that never came up
    #[arg(long, num_args = 0..=1, default_missing_value = "60")]
    pub wait: Option<u64>,
    /// Assign a behavioral role, re-injected on a cadence:
    /// "<title>[:every=<N><msg|tok>]" (default every=20msg). The role must
    /// already exist in the project (`centaury role create`).
    #[arg(long)]
    pub role: Option<String>,
    /// Auto-reminded skill: "<name>[:every=<N><msg|tok>][:pointer|full]"
    /// (default every=15msg, pointer). Repeatable; each must exist in the
    /// project (`centaury skill create`).
    #[arg(long = "skill")]
    pub skills: Vec<String>,
    /// Extra args passed to the tool (after --)
    #[arg(last = true)]
    pub tool_args: Vec<String>,
}

/// First-turn bootstrap for tools launched without a prompt — hook-only
/// delivery means an agent that never had a turn is DEAF (no Stop hook
/// listening); this seeds the turn that enters the Stop-wait loop.
const SEED_PROMPT: &str = "You just joined the kore network. Run `centaury list` to see \
                    who is online, then wait for kore messages or user instructions.";

/// `fork`: new agent continuing the current directory's most recent session.
/// Tool name → executable on PATH. Only cursor differs (`cursor-agent`).
fn tool_binary(tool: &str) -> &str {
    match tool {
        "cursor" => "cursor-agent",
        t => t,
    }
}

/// HC5 (hcom 3db0398): inject ephemeral workspace-trust args so gemini/codex
/// don't stall on their first-run "trust this folder?" prompt. Session-scoped,
/// nothing persisted. Idempotent. `KORE_AUTO_TRUST=0` opts out.
/// - gemini: `--skip-trust`.
/// - codex: `-c projects={ "<canonical cwd>" = { trust_level = "trusted" } }`
///   — key `projects` has no dots (codex splits `-c` keys on `.`), the path
///   lives quoted in the VALUE so dotted dir names stay intact.
/// - cursor: marker file, not argv (its `--trust` flag only works in print
///   mode) — `~/.cursor/projects/<slug>/.workspace-trusted`, cursor's own
///   path-slug scheme, written before spawn (hcom 134e0ba/3db0398).
fn inject_workspace_trust(tool: &str, tool_args: &mut Vec<String>) {
    if std::env::var("KORE_AUTO_TRUST").is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("false")) {
        return;
    }
    match tool {
        "gemini" if !tool_args.iter().any(|a| a == "--skip-trust") => {
            tool_args.push("--skip-trust".to_string());
        }
        "codex"
            if !tool_args
                .windows(2)
                .any(|w| w[0] == "-c" && w[1].contains("trust_level")) =>
        {
            let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let canonical = std::fs::canonicalize(&dir).unwrap_or(dir);
            tool_args.push("-c".to_string());
            tool_args.push(format!(
                "projects={{ \"{}\" = {{ trust_level = \"trusted\" }} }}",
                canonical.to_string_lossy()
            ));
        }
        "cursor" | "cursor-agent" => {
            let _ = seed_cursor_trust_marker();
        }
        "claude" => {
            let _ = seed_claude_trust();
        }
        _ => {}
    }
}

/// Claude parks untrusted workspaces behind an interactive trust dialog and —
/// worse for headless — silently IGNORES `.claude/settings.json`
/// `permissions.allow` there ("Ignoring N permissions.allow entries"), so a
/// background agent can never run `centaury send` and is effectively mute
/// (found live in the DU-C1 spike). Pre-grant trust the way the dialog would:
/// `projects["<canonical cwd>"].hasTrustDialogAccepted = true` in claude's
/// state file (`$CLAUDE_CONFIG_DIR/.claude.json`, default `~/.claude.json`).
/// Surgical merge — only that one key is ever added, nothing overwritten.
fn seed_claude_trust() -> anyhow::Result<()> {
    let state_path = match std::env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) => std::path::PathBuf::from(dir).join(".claude.json"),
        Err(_) => dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("no home dir"))?
            .join(".claude.json"),
    };
    let dir = std::env::current_dir()?;
    let cwd = std::fs::canonicalize(&dir)
        .unwrap_or(dir)
        .to_string_lossy()
        .into_owned();

    let mut root: serde_json::Value = match std::fs::read_to_string(&state_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };
    let projects = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("claude state file is not a JSON object"))?
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let entry = projects
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("claude 'projects' is not an object"))?
        .entry(cwd)
        .or_insert_with(|| serde_json::json!({}));
    let obj = entry
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("claude project entry is not an object"))?;
    if obj.get("hasTrustDialogAccepted").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(()); // already trusted — never rewrite the file for nothing
    }
    obj.insert(
        "hasTrustDialogAccepted".into(),
        serde_json::Value::Bool(true),
    );
    std::fs::write(&state_path, serde_json::to_string(&root)?)?;
    Ok(())
}

/// Cursor stores per-workspace state under `~/.cursor/projects/<slug>`; the
/// slug mirrors cursor's own scheme (separators/punctuation → dashes). An
/// existing marker is left alone — never overwrite user/tool state.
fn cursor_trust_slug(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .split(std::path::MAIN_SEPARATOR)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn seed_cursor_trust_marker() -> anyhow::Result<()> {
    let dir = std::env::current_dir()?;
    let workspace = std::fs::canonicalize(&dir).unwrap_or(dir);
    let marker = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?
        .join(".cursor/projects")
        .join(cursor_trust_slug(&workspace))
        .join(".workspace-trusted");
    if marker.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(marker.parent().unwrap())?;
    std::fs::write(
        &marker,
        serde_json::to_string_pretty(&serde_json::json!({
            // epoch seconds — cursor validates workspacePath, not the date;
            // no chrono dep in the client for one timestamp
            "trustedAt": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "workspacePath": workspace.to_string_lossy(),
            "trustMethod": "kore-launch",
        }))?,
    )?;
    Ok(())
}

/// Claude only — gemini/codex can resume a session but not fork one into a
/// new session id; use `centaury resume` for them.
pub async fn run_fork(mut args: LaunchArgs) -> anyhow::Result<()> {
    if args.tool != "claude" {
        anyhow::bail!(
            "fork supports claude only ({} can't fork sessions — use `centaury resume`)",
            args.tool
        );
    }
    let mut tool_args = vec!["--continue".to_string(), "--fork-session".to_string()];
    tool_args.extend(std::mem::take(&mut args.tool_args));
    args.tool_args = tool_args;
    run(args).await
}

/// Async wrapper: clear any kill tombstones (relaunch = explicit revive),
/// spawn via the sync chain (which recurses for --count), then optionally
/// wait for the agents' registration receipts (--wait).
pub async fn run(mut args: LaunchArgs) -> anyhow::Result<()> {
    let wait = args.wait;
    if wait.is_some() && !args.headless && args.terminal.is_none() {
        anyhow::bail!(
            "--wait needs --terminal or --headless (an interactive launch blocks here anyway)"
        );
    }
    if args.tag.is_none() {
        args.tag = std::env::var("KORE_TAG").ok().filter(|t| !t.is_empty());
    }
    if let Some(tag) = &args.tag
        && (tag.is_empty() || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        anyhow::bail!("tag can only contain letters, numbers, and hyphens");
    }
    // Every agent has an owner = whoever launches it (server derives the same
    // from the launcher's token; this covers the hook self-register fallback,
    // where an unset KORE_OWNER would otherwise null the owner on re-register).
    if args.owner.is_none() {
        args.owner = std::env::var("KORE_OWNER")
            .ok()
            .filter(|o| !o.is_empty())
            .or_else(|| crate::config::instance_name().ok());
    }
    if args.owner.is_none() {
        anyhow::bail!(
            "launch needs an owner — register yourself first (`centaury human <you>`) or pass --owner"
        );
    }
    // Names resolved HERE so pre-registration, env and any re-exec all agree.
    // No --name → fresh random CVCV per agent, even with --count (a shared
    // random base with -1..N suffixes read as "they all got the same name").
    let names = resolve_names(&args);
    if args.count <= 1 {
        args.name = Some(names[0].clone());
    }
    revive(&names, &args).await;
    let project = args
        .project
        .clone()
        .or_else(|| std::env::var("KORE_PROJECT").ok());
    // Fail fast on a bad/nonexistent --role/--skill before we spawn anything.
    validate_role_skills(&args, project.as_deref()).await?;
    if wait.is_some() {
        // A token left by a previous run would false-positive the wait; the
        // agent re-registers when its token is missing, so deleting is safe.
        for n in &names {
            let _ = std::fs::remove_file(crate::config::agent_token_path(n, project.as_deref()));
        }
    }
    // Register every agent NOW with the launcher's own identity — the agent
    // is in the roster before its first hook fires (legacy launch semantics),
    // and hooks never need KORE_REG_SECRET. Falls back to hook
    // self-registration when the launcher has no token.
    preregister(&names, project.as_deref(), &args).await;
    run_inner(args, &names)?;
    if let Some(secs) = wait {
        wait_for_registration(&names, project.as_deref(), secs).await?;
    }
    Ok(())
}

/// Fail fast before spawning: every `--role`/`--skill` spec must parse, and
/// the named role/skill must already exist in the project. A parse error is
/// the caller's typo → always fatal; a transient fetch failure (offline / old
/// server) only warns, leaving the hook's self-assign to retry.
async fn validate_role_skills(args: &LaunchArgs, project: Option<&str>) -> anyhow::Result<()> {
    if args.role.is_none() && args.skills.is_empty() {
        return Ok(());
    }
    let project = project.unwrap_or("default");
    let role_title = match &args.role {
        Some(spec) => Some(
            crate::cadence::parse_role_spec(spec)
                .map_err(|e| anyhow::anyhow!("--role {spec:?}: {e}"))?
                .0,
        ),
        None => None,
    };
    let mut skill_names = Vec::with_capacity(args.skills.len());
    for spec in &args.skills {
        skill_names.push(
            crate::cadence::parse_skill_spec(spec)
                .map_err(|e| anyhow::anyhow!("--skill {spec:?}: {e}"))?
                .skill,
        );
    }
    if let Some(title) = &role_title {
        match crate::get_json::<Vec<kore_protocol::api::RoleSummary>>(&format!(
            "/v1/projects/{project}/roles"
        ))
        .await
        {
            Ok(roles) if !roles.iter().any(|r| &r.title == title) => anyhow::bail!(
                "role '{title}' not found in project '{project}' — create it first: \
                 centaury role create {title:?} --content \"...\""
            ),
            Ok(_) => {}
            Err(e) => eprintln!(
                "[kore] couldn't verify role '{title}' ({e}) — launching anyway; the hook self-assigns"
            ),
        }
    }
    if !skill_names.is_empty() {
        match crate::get_json::<Vec<kore_protocol::api::SkillSummary>>(&format!(
            "/v1/projects/{project}/skills"
        ))
        .await
        {
            Ok(skills) => {
                let have: std::collections::HashSet<&str> =
                    skills.iter().map(|s| s.name.as_str()).collect();
                let missing: Vec<&str> = skill_names
                    .iter()
                    .map(String::as_str)
                    .filter(|n| !have.contains(n))
                    .collect();
                if !missing.is_empty() {
                    anyhow::bail!(
                        "skill(s) not found in project '{project}': {} — create with \
                         centaury skill create",
                        missing.join(", ")
                    );
                }
            }
            Err(e) => eprintln!(
                "[kore] couldn't verify skills ({e}) — launching anyway; the hook self-assigns"
            ),
        }
    }
    Ok(())
}

/// The instance names this launch will register. Explicit --name + --count
/// keeps the <base>-1..N scheme; no --name gets an independent random CVCV
/// name per agent (dedup guard: random_name is time-seeded and could repeat
/// within one tight loop). --tag never touches the name — it's a separate
/// column (legacy semantics); "{tag}-{name}" is only a display/routing alias.
fn resolve_names(args: &LaunchArgs) -> Vec<String> {
    let n = args.count.max(1);
    match &args.name {
        Some(base) if n > 1 => (1..=n).map(|i| format!("{base}-{i}")).collect(),
        Some(base) => vec![base.clone()],
        None => {
            let mut names: Vec<String> = Vec::with_capacity(n as usize);
            while names.len() < n as usize {
                let name = crate::config::random_name();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            names
        }
    }
}

/// Poll until every launched name has its token file — written by save_token
/// the moment the server accepts the register call, so it doubles as the
/// registration receipt without a roster API call (which couldn't see across
/// sealed project bubbles anyway).
async fn wait_for_registration(
    names: &[String],
    project: Option<&str>,
    secs: u64,
) -> anyhow::Result<()> {
    println!(
        "waiting up to {secs}s for {} agent(s) to register...",
        names.len()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        let missing = missing_names(names, project);
        if missing.is_empty() {
            println!("all {} agent(s) registered", names.len());
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {secs}s waiting for: {} (check $KORE_DIR/logs/<name>.log for headless agents)",
                missing.join(", ")
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

fn missing_names(names: &[String], project: Option<&str>) -> Vec<String> {
    names
        .iter()
        .filter(|n| !crate::config::agent_token_path(n, project).exists())
        .cloned()
        .collect()
}

/// POST /v1/instances/agents for each name using the launcher's token; the
/// returned agent token is written to `agents/<name>.token` (the same file
/// hooks and --wait use). Best-effort: on any failure the old path still
/// works — the agent self-registers via its SessionStart hook.
async fn preregister(names: &[String], project: Option<&str>, args: &LaunchArgs) {
    let Ok(token) = crate::config::load_token() else {
        eprintln!(
            "[kore] no identity here — agent will self-register via hooks (needs KORE_REG_SECRET)"
        );
        return;
    };
    let client = crate::config::http_client();
    let url = format!("{}/v1/instances/agents", crate::config::server_url());
    let directory = std::env::current_dir()
        .ok()
        .map(|d| d.display().to_string());
    for name in names {
        let resp = client
            .post(&url)
            .bearer_auth(&token)
            .json(&kore_protocol::api::RegisterAgentRequest {
                name: name.clone(),
                tag: args.tag.clone(),
                tool: Some(args.tool.clone()),
                directory: directory.clone(),
                // Resolved --owner/KORE_OWNER; server validates it's a real
                // human here (self or a delegate account, DU-S2).
                owner: args.owner.clone(),
            })
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                if let Ok(body) = r.json::<kore_protocol::api::RegisterResponse>().await {
                    let path = crate::config::agent_token_path(name, project);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(path, body.token);
                }
            }
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                eprintln!(
                    "[kore] pre-register '{name}' failed ({status}): {text} — hooks will retry"
                );
            }
            Err(e) => eprintln!("[kore] pre-register '{name}' failed: {e} — hooks will retry"),
        }
    }
}

/// Fire-and-forget POST /v1/instances/revive for every name this launch will
/// register. Errors ignored: no tombstone / old server / offline server all
/// mean the register attempt decides, exactly as before G1.
async fn revive(names: &[String], args: &LaunchArgs) {
    let Ok(reg_secret) = crate::config::reg_secret() else {
        return;
    };
    let client = crate::config::http_client();
    let url = format!("{}/v1/instances/revive", crate::config::server_url());
    for name in names {
        let _ = client
            .post(&url)
            .json(&kore_protocol::api::ReviveRequest {
                name: name.clone(),
                reg_secret: reg_secret.clone(),
                org: std::env::var("KORE_ORG").ok(),
                project: args
                    .project
                    .clone()
                    .or_else(|| std::env::var("KORE_PROJECT").ok()),
            })
            .send()
            .await;
    }
}

fn run_inner(mut args: LaunchArgs, names: &[String]) -> anyhow::Result<()> {
    if args.headless && args.terminal.is_some() {
        anyhow::bail!("--headless and --terminal are mutually exclusive");
    }
    if args.stay && !args.headless {
        anyhow::bail!("--stay requires --headless (interactive launches already stay)");
    }
    // --stay rides a BLOCKING stop hook (claude Stop / gemini AfterAgent /
    // codex Stop / cursor stop / copilot stop / kimi stop). Plugin-dialect
    // tools inject cooperatively inside a live process — their print modes
    // (`opencode run`, `omp -p`) exit at turn end regardless (live-verified
    // opencode 1.17.15 + omp 16.3.8, 2026-07-07); cline has no async inject.
    if args.stay
        && matches!(
            args.tool.as_str(),
            "opencode" | "kilo" | "pi" | "omp" | "cline"
        )
    {
        anyhow::bail!(
            "--stay doesn't work for {}: its print mode exits when the turn ends. Launch it interactive (--terminal) instead.",
            args.tool
        );
    }
    if args.count > 1 {
        return run_batch(args, names);
    }
    if let Some(term) = args.terminal.take() {
        return spawn_in_terminal(&term, &args);
    }
    // Interactive claude without a prompt just hangs headless (legacy had the
    // same validation) — check BEFORE hook install so an error leaves no trace.
    // claude and gemini share the shape: `-p <prompt>` is the only headless
    // entry into the blocking stop-wait loop (Stop / AfterAgent).
    if args.headless
        && matches!(args.tool.as_str(), "claude" | "gemini")
        && !args
            .tool_args
            .iter()
            .any(|a| a == "-p" || a == "--print" || a == "--prompt")
    {
        if args.stay {
            // Persistent agent (DU-C1): seed the first turn like interactive does.
            args.tool_args.push("-p".into());
            args.tool_args.push(SEED_PROMPT.into());
        } else {
            anyhow::bail!(
                "headless {} needs a prompt: centaury launch --headless -- -p '<task>' (or --stay for a persistent agent)",
                args.tool
            );
        }
    }
    // Hooks do registration + delivery; without them the agent is deaf.
    if !args.no_hooks {
        crate::hook::install(&args.tool, false)?;
    }

    // Message delivery is hook-driven: an interactive agent that never had a
    // turn has no Stop hook listening and receives NOTHING (legacy PTY
    // injected at any time). Seed a first turn so the agent enters the
    // Stop-wait loop immediately. ponytail: claude only (positional prompt);
    // other tools when someone launches them interactive and unprompted.
    if !args.headless && args.tool_args.is_empty() {
        match args.tool.as_str() {
            // claude takes a bare positional prompt.
            "claude" => args.tool_args.push(SEED_PROMPT.into()),
            // agy has no bare positional (HC13, hcom 62b1795): its documented
            // interactive flag is --prompt-interactive.
            "antigravity" => {
                args.tool_args.push("--prompt-interactive".into());
                args.tool_args.push(SEED_PROMPT.into());
            }
            _ => {}
        }
    }

    // HC5: folder-trust is a first-run gate — gemini/codex park on a "trust
    // this folder?" prompt and never start, fatal for --headless/--count.
    // Running centaury here IS the consent; pre-grant it. KORE_AUTO_TRUST=0
    // opts out (no config file in kore — env is the knob).
    inject_workspace_trust(&args.tool, &mut args.tool_args);

    // Always name the agent: KORE_NAME routes its token to a per-agent file
    // ($KORE_DIR/[project/]agents/<name>.token), so it can never pick up the
    // human's token and speak as them.
    let name = args
        .name
        .clone()
        .unwrap_or_else(crate::config::derived_name);

    if args.headless {
        return spawn_headless(&name, &args);
    }
    println!("launching {} as agent '{name}'", args.tool);

    let mut cmd = Command::new(tool_binary(&args.tool));
    cmd.args(&args.tool_args);
    set_kore_env(&mut cmd, &name, &args);

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "cannot launch '{}': {e} (is it installed and on PATH?)",
            args.tool
        )
    })?;
    // Terminal-window launches funnel through this path too (the re-exec'd
    // inner `centaury launch` wraps the tool), so the recorded pid is
    // always the real tool process, not the terminal emulator.
    write_pid(&name, &args, child.id());
    spawn_waker(&name, child.id(), &args);
    let status = child.wait();
    let _ = std::fs::remove_file(pid_path(&name, &args)); // process gone, pid stale
    std::process::exit(status.map_or(1, |s| s.code().unwrap_or(1)));
}

fn pid_path(name: &str, args: &LaunchArgs) -> std::path::PathBuf {
    let project = args
        .project
        .clone()
        .or_else(|| std::env::var("KORE_PROJECT").ok());
    crate::config::agent_pid_path(name, project.as_deref())
}

/// Record "<pid> <tool>" so `kill` can signal the process. Fire-and-forget:
/// a failed write just means kill falls back to server-side unregister only.
fn write_pid(name: &str, args: &LaunchArgs, pid: u32) {
    let path = pid_path(name, args);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("{pid} {}", args.tool));
}

/// W2: detached waker for this agent — pokes its terminal when a message
/// arrives and no Stop-hook wait is listening. Fire-and-forget: a waker that
/// fails to start never breaks the launch. It exits on its own when the tool
/// dies or no poke backend exists (bare terminal); stderr goes to the log so
/// "nothing to poke" is findable.
fn spawn_waker(name: &str, tool_pid: u32, args: &LaunchArgs) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(["waker", name, &tool_pid.to_string(), &args.tool]);
    set_kore_env(&mut cmd, name, args);
    let log_dir = crate::config::kore_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let stderr = std::fs::File::create(log_dir.join(format!("{name}.waker.log")))
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null());
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(stderr);
    let _ = cmd.spawn();
}

/// Signal the locally recorded process for `name` (SIGTERM via kill(1)).
/// Returns the pid it signalled, None when there was nothing to kill here.
pub fn kill_local(name: &str) -> Option<u32> {
    let project = std::env::var("KORE_PROJECT").ok();
    kill_waker(name, project.as_deref());
    let path = crate::config::agent_pid_path(name, project.as_deref());
    let content = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let (pid, tool) = content.trim().split_once(' ')?;
    let pid: u32 = pid.parse().ok()?;
    // pid-reuse guard: only signal if the pid still runs the tool we spawned.
    let cmdline = std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if !cmdline.replace('\0', " ").contains(tool) {
        return None; // stale pid (agent exited on its own)
    }
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| pid)
}

/// W2 teardown: SIGTERM the agent's waker via its pid file (cmdline-guarded
/// against pid reuse). Best-effort — the waker also self-exits when its tool
/// dies.
fn kill_waker(name: &str, project: Option<&str>) {
    let path = crate::config::agent_waker_pid_path(name, project);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let _ = std::fs::remove_file(&path);
    let Ok(pid) = content.trim().parse::<u32>() else {
        return;
    };
    let cmdline = std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if cmdline.replace('\0', " ").contains("waker") {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
}

fn set_kore_env(cmd: &mut Command, name: &str, args: &LaunchArgs) {
    cmd.env("KORE_NAME", name);
    // Launched interactive agents stay reachable between turns (legacy PTY
    // parity): the Stop hook listens ~forever instead of the 120s default.
    // Headless keeps the short default so one-shot -p runs actually exit —
    // unless --stay (DU-C1): persistent background agents ride the same
    // Stop-block loop as interactive (verified live: claude -p runs new turns
    // on message arrival and survives between them).
    if (!args.headless || args.stay) && std::env::var("KORE_HOOK_TIMEOUT").is_err() {
        cmd.env("KORE_HOOK_TIMEOUT", "86400");
    }
    // Explicit, not inherited-only: terminal spawns (tmux runs in the server's
    // context) and the agent's own centaury calls must hit the same server.
    cmd.env("KORE_SERVER_URL", crate::config::server_url());
    if let Some(project) = &args.project {
        cmd.env("KORE_PROJECT", project);
    }
    if let Some(owner) = &args.owner {
        cmd.env("KORE_OWNER", owner);
    }
    // Hook self-register fallback reads this so a re-register keeps the tag.
    if let Some(tag) = &args.tag {
        cmd.env("KORE_TAG", tag);
    }
    // Role/skills ride env → the agent's OWN hook self-assigns them (self
    // endpoints use the caller's instance token, so the identity is naturally
    // correct). Specs are validated in run() before we get here.
    if let Some(role) = &args.role {
        cmd.env("KORE_ROLE", role);
    }
    if !args.skills.is_empty() {
        cmd.env("KORE_SKILLS", args.skills.join(";"));
    }
}

/// --count N: launch N agents (names resolved in run() — <base>-1..N with
/// --name, independent random names without), each through the normal
/// single-launch path (own window or headless — N interactive CLIs can't
/// share this terminal).
fn run_batch(args: LaunchArgs, names: &[String]) -> anyhow::Result<()> {
    if !args.headless && args.terminal.is_none() {
        anyhow::bail!(
            "--count {} needs --terminal or --headless ({} interactive agents can't share this terminal)",
            args.count,
            args.count
        );
    }
    for name in names {
        let one = LaunchArgs {
            name: Some(name.clone()),
            tag: args.tag.clone(),
            count: 1,
            project: args.project.clone(),
            owner: args.owner.clone(),
            tool: args.tool.clone(),
            no_hooks: args.no_hooks,
            terminal: args.terminal.clone(),
            headless: args.headless,
            stay: args.stay,
            wait: None, // run() waits for the whole batch after run_inner
            role: args.role.clone(),
            skills: args.skills.clone(),
            tool_args: args.tool_args.clone(),
        };
        run_inner(one, std::slice::from_ref(name))?; // revive already covered all names in run()
    }
    Ok(())
}

/// --headless: spawn the tool detached, output to $KORE_DIR/logs/<name>.log.
/// Claude prompt validation already happened in run(), before hook install.
fn spawn_headless(name: &str, args: &LaunchArgs) -> anyhow::Result<()> {
    let log_dir = crate::config::kore_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("{name}.log"));
    let log = std::fs::File::create(&log_path)?;

    let mut cmd = Command::new(tool_binary(&args.tool));
    cmd.args(&args.tool_args);
    set_kore_env(&mut cmd, name, args);
    // Headless agents have no terminal of their own — scrub the launcher's
    // terminal identity, or the SessionStart hook would spawn a waker that
    // pokes its nudge into the HUMAN's pane (typed into their shell).
    for var in ["TMUX", "TMUX_PANE", "WEZTERM_PANE", "KITTY_WINDOW_ID"] {
        cmd.env_remove(var);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "cannot launch '{}': {e} (is it installed and on PATH?)",
            args.tool
        )
    })?;
    write_pid(name, args, child.id());
    println!(
        "started '{name}' (headless, pid {}) — log: {}",
        child.id(),
        log_path.display()
    );
    Ok(())
}

// ---- --terminal: open the agent in a new terminal window ----
//
// Re-execs `centaury launch` (same flags, minus --terminal) inside the new
// window, so identity/env setup goes through the one normal path. Preset
// subset ported from legacy/src/shared/terminal_presets.rs; Warp/Zellij/macOS
// bundles return when someone asks (plan B1).

fn spawn_in_terminal(term: &str, args: &LaunchArgs) -> anyhow::Result<()> {
    let term = if term == "auto" {
        detect_terminal()?
    } else {
        term.to_string()
    };
    // Resolve the name here so the printed name matches the inner launch.
    let name = args
        .name
        .clone()
        .unwrap_or_else(crate::config::derived_name);
    let title = format!("{} — {name}", args.tool);
    let argv = if term == "split" {
        split_argv(inner_launch_argv(&name, args), &title)?
    } else {
        terminal_argv(&term, inner_launch_argv(&name, args), &title)?
    };

    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if term == "split" {
        // Pane-creating commands return as soon as the pane exists — wait, so
        // "can't split" (e.g. kitty without allow_remote_control) errors here.
        let out = cmd.output().map_err(|e| {
            anyhow::anyhow!(
                "cannot spawn '{}': {e} (is it installed and on PATH?)",
                argv[0]
            )
        })?;
        if !out.status.success() {
            anyhow::bail!(
                "split failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        println!("launching {} as agent '{name}' in a split pane", args.tool);
        return Ok(());
    }
    // ponytail: spawn-and-detach — GUI terminals block until their window
    // closes, so we never wait; a missing binary still errors right here.
    cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "cannot spawn '{}': {e} (is it installed and on PATH?)",
            argv[0]
        )
    })?;
    println!(
        "launching {} as agent '{name}' in a new {term} window",
        args.tool
    );
    Ok(())
}

/// Env → preferred terminal we are inside of; else first supported one on PATH.
fn detect_terminal() -> anyhow::Result<String> {
    for (var, term) in [
        ("KITTY_WINDOW_ID", "kitty"),
        ("WEZTERM_PANE", "wezterm"),
        ("TMUX", "tmux"),
    ] {
        if std::env::var(var).is_ok_and(|v| !v.is_empty()) {
            return Ok(term.to_string());
        }
    }
    for term in ["kitty", "wezterm", "foot"] {
        if on_path(term) {
            return Ok(term.to_string());
        }
    }
    anyhow::bail!(
        "no supported terminal detected (kitty|wezterm|tmux|foot) — pass --terminal <name>"
    )
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// The `centaury launch ...` argv to run inside the new window.
fn inner_launch_argv(name: &str, args: &LaunchArgs) -> Vec<String> {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "centaury".to_string());
    let mut v = vec![
        exe,
        "launch".into(),
        "--name".into(),
        name.into(),
        "--tool".into(),
        args.tool.clone(),
    ];
    for (flag, val) in [
        ("--project", &args.project),
        ("--owner", &args.owner),
        ("--tag", &args.tag),
        ("--role", &args.role),
    ] {
        if let Some(val) = val {
            v.extend([flag.to_string(), val.clone()]);
        }
    }
    for skill in &args.skills {
        v.extend(["--skill".to_string(), skill.clone()]);
    }
    if args.no_hooks {
        v.push("--no-hooks".into());
    }
    if !args.tool_args.is_empty() {
        v.push("--".into());
        v.extend(args.tool_args.iter().cloned());
    }
    v
}

/// Wrap an inner argv in the terminal's own spawn command, titling the window
/// "<tool> — <name>" like legacy. kitty/foot/wezterm take real argv and
/// inherit cwd + env from us; tmux runs commands in the server's context, so
/// cwd (-c), KORE_* env (-e) and the command (one shell string) must be
/// passed explicitly. wezterm start has no title flag — skipped.
fn terminal_argv(term: &str, inner: Vec<String>, title: &str) -> anyhow::Result<Vec<String>> {
    let mut argv: Vec<String> = match term {
        "kitty" => vec!["kitty".into(), "--title".into(), title.into()],
        "foot" => vec!["foot".into(), "-T".into(), title.into()],
        "wezterm" => vec!["wezterm".into(), "start".into(), "--".into()],
        "tmux" => {
            let mut v = vec![
                "tmux".into(),
                "new-window".into(),
                "-n".into(),
                title.into(),
            ];
            if let Ok(cwd) = std::env::current_dir() {
                v.extend(["-c".into(), cwd.display().to_string()]);
            }
            for (k, val) in std::env::vars().filter(|(k, _)| k.starts_with("KORE_")) {
                v.extend(["-e".into(), format!("{k}={val}")]);
            }
            v.push(
                inner
                    .iter()
                    .map(|s| shell_quote(s))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            return Ok(v);
        }
        other => anyhow::bail!("unsupported terminal '{other}' (kitty|wezterm|tmux|foot)"),
    };
    argv.extend(inner);
    Ok(argv)
}

/// Divide the terminal we're INSIDE of into a new pane running the agent.
/// Detected from env; errors when the current terminal can't split.
fn split_argv(inner: Vec<String>, title: &str) -> anyhow::Result<Vec<String>> {
    let kore_env = || std::env::vars().filter(|(k, _)| k.starts_with("KORE_"));
    if std::env::var("TMUX").is_ok_and(|v| !v.is_empty()) {
        // tmux split-window has no -n; pane title comes from the command itself.
        let mut v = vec!["tmux".into(), "split-window".into(), "-h".into()];
        if let Ok(cwd) = std::env::current_dir() {
            v.extend(["-c".into(), cwd.display().to_string()]);
        }
        for (k, val) in kore_env() {
            v.extend(["-e".into(), format!("{k}={val}")]);
        }
        v.push(
            inner
                .iter()
                .map(|s| shell_quote(s))
                .collect::<Vec<_>>()
                .join(" "),
        );
        return Ok(v);
    }
    if std::env::var("WEZTERM_PANE").is_ok_and(|v| !v.is_empty()) {
        // ponytail: wezterm cli has no --env; KORE_* config rides the inner
        // flags, server URL/secret must be in the mux server's env.
        let mut v = vec!["wezterm".into(), "cli".into(), "split-pane".into()];
        if let Ok(cwd) = std::env::current_dir() {
            v.extend(["--cwd".into(), cwd.display().to_string()]);
        }
        v.push("--".into());
        v.extend(inner);
        return Ok(v);
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok_and(|v| !v.is_empty()) {
        // Needs allow_remote_control in kitty.conf; kitty errors loudly if not.
        let mut v = vec![
            "kitty".into(),
            "@".into(),
            "launch".into(),
            "--location=vsplit".into(),
            "--cwd=current".into(),
            "--title".into(),
            title.into(),
        ];
        for (k, val) in kore_env() {
            v.extend(["--env".into(), format!("{k}={val}")]);
        }
        v.push("--".into());
        v.extend(inner);
        return Ok(v);
    }
    anyhow::bail!(
        "current terminal can't split (need tmux, wezterm, or kitty with allow_remote_control) — use --terminal <kitty|wezterm|tmux|foot> for a new window instead"
    )
}

/// Minimal POSIX single-quote escaping for the tmux shell-command string.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./@=:".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_argv_wraps_inner_command() {
        let inner = vec![
            "centaury".to_string(),
            "launch".into(),
            "--name".into(),
            "luna".into(),
        ];
        let title = "claude — luna";

        // argv-passing terminals: title flag, then inner command verbatim
        let kitty = terminal_argv("kitty", inner.clone(), title).unwrap();
        assert_eq!(&kitty[..3], &["kitty", "--title", title]);
        assert_eq!(&kitty[3..], &inner[..]);
        let foot = terminal_argv("foot", inner.clone(), title).unwrap();
        assert_eq!(&foot[..3], &["foot", "-T", title]);
        let wez = terminal_argv("wezterm", inner.clone(), title).unwrap();
        assert_eq!(&wez[..3], &["wezterm", "start", "--"]);

        // tmux: named window, single shell string, spaces quoted
        let spaced = vec!["centaury".to_string(), "send".into(), "hi there".into()];
        let tmux = terminal_argv("tmux", spaced, title).unwrap();
        assert_eq!(&tmux[..4], &["tmux", "new-window", "-n", title]);
        assert!(tmux.last().unwrap().ends_with("send 'hi there'"));

        assert!(terminal_argv("warp", vec![], title).is_err());
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    fn base_args() -> LaunchArgs {
        use clap::Parser;
        LaunchArgs::parse_from(["launch"])
    }

    /// KORE_DIR is process-global; tests that pin it must not overlap.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cursor_slug_matches_cursor_scheme() {
        let p = std::path::Path::new("/home/user/my.project/sub_dir");
        assert_eq!(cursor_trust_slug(p), "home-user-my-project-sub_dir");
    }

    #[test]
    fn workspace_trust_injects_per_tool() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("KORE_AUTO_TRUST") };

        let mut g = vec!["--model".to_string(), "flash".into()];
        inject_workspace_trust("gemini", &mut g);
        assert!(g.contains(&"--skip-trust".to_string()));
        // idempotent
        inject_workspace_trust("gemini", &mut g);
        assert_eq!(g.iter().filter(|a| *a == "--skip-trust").count(), 1);

        let mut c: Vec<String> = vec![];
        inject_workspace_trust("codex", &mut c);
        assert_eq!(c[0], "-c");
        assert!(c[1].starts_with("projects={") && c[1].contains("trust_level"));

        // claude untouched
        let mut cl = vec!["-p".to_string()];
        inject_workspace_trust("claude", &mut cl);
        assert_eq!(cl, vec!["-p".to_string()]);
    }

    #[test]
    fn workspace_trust_opt_out() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("KORE_AUTO_TRUST", "0") };
        let mut g: Vec<String> = vec![];
        inject_workspace_trust("gemini", &mut g);
        assert!(g.is_empty());
        unsafe { std::env::remove_var("KORE_AUTO_TRUST") };
    }

    #[test]
    fn count_needs_terminal_or_headless() {
        let mut args = base_args();
        args.count = 3;
        let err = run_batch(args, &[]).unwrap_err().to_string();
        assert!(err.contains("--terminal or --headless"), "{err}");
    }

    #[test]
    fn headless_claude_needs_prompt() {
        let mut args = base_args();
        args.headless = true;
        let err = run_inner(args, &["x".into()]).unwrap_err().to_string();
        assert!(err.contains("-p"), "{err}");
    }

    /// No --name: every agent gets its own random name — never a shared base
    /// with -1..N suffixes (read as "they all got the same name"). Explicit
    /// --name keeps the deterministic suffix scheme.
    #[test]
    fn unnamed_batch_gets_independent_names() {
        let mut args = base_args();
        args.count = 3;
        let names = resolve_names(&args);
        assert_eq!(names.len(), 3);
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 3, "{names:?}");
        assert!(names.iter().all(|n| !n.contains('-')), "{names:?}");

        args.name = Some("luna".into());
        assert_eq!(resolve_names(&args), ["luna-1", "luna-2", "luna-3"]);
        args.count = 1;
        assert_eq!(resolve_names(&args), ["luna"]);

        // --tag is a separate label (own column server-side), never in the name.
        args.tag = Some("team".into());
        assert_eq!(resolve_names(&args), ["luna"]);
    }

    #[tokio::test]
    async fn wait_reports_missing_names_on_timeout() {
        // Interactive launch can't --wait (it blocks in run_inner anyway).
        let mut args = base_args();
        args.wait = Some(1);
        let err = super::run(args).await.unwrap_err().to_string();
        assert!(err.contains("--terminal or --headless"), "{err}");

        // Poll core: registered name (token file) clears, missing one reported.
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("kore-wait-test-{}", std::process::id()));
        let names = vec!["ready".to_string(), "ghost".to_string()];
        let token = dir.join("agents").join("ready.token");
        std::fs::create_dir_all(token.parent().unwrap()).unwrap();
        std::fs::write(&token, "tok").unwrap();
        // agent_token_path reads KORE_DIR at call time; missing_names is the
        // pure part — drive it via the env like the real flow does.
        unsafe { std::env::set_var("KORE_DIR", &dir) };
        let missing = missing_names(&names, None);
        unsafe { std::env::remove_var("KORE_DIR") };
        assert_eq!(missing, vec!["ghost".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_local_signals_recorded_pid() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("kore-kill-test-{}", std::process::id()));
        unsafe {
            std::env::set_var("KORE_DIR", &dir);
            std::env::remove_var("KORE_PROJECT");
        }

        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();
        // spawn() returns pre-exec: /proc/<pid>/cmdline still shows the fork
        // copy for a moment. Wait until the child actually IS `sleep`, or the
        // pid-reuse guard in kill_local correctly (but flakily) refuses.
        for _ in 0..100 {
            let cmdline =
                std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            if cmdline.contains("sleep") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let path = crate::config::agent_pid_path("guinea", None);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{pid} sleep")).unwrap();

        assert_eq!(kill_local("guinea"), Some(pid));
        assert!(!path.exists(), "pid file must be consumed");
        // SIGTERM delivered: wait() returns a signal-killed status, and it
        // returns now instead of after the full 30s sleep.
        let status = child.wait().unwrap();
        assert!(
            !status.success(),
            "process must die from SIGTERM, not exit 0"
        );

        // Stale pid (tool no longer matches) → no signal, file still consumed.
        std::fs::write(&path, "99999999 sleep").unwrap();
        assert_eq!(kill_local("guinea"), None);
        let _ = std::fs::remove_dir_all(&dir);
        unsafe { std::env::remove_var("KORE_DIR") };
    }

    #[test]
    fn headless_conflicts_with_terminal() {
        let mut args = base_args();
        args.headless = true;
        args.terminal = Some("kitty".into());
        let err = run_inner(args, &["x".into()]).unwrap_err().to_string();
        assert!(err.contains("mutually exclusive"), "{err}");
    }
}
