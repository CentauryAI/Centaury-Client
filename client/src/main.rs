mod cadence;
mod commands;
mod config;
mod hook;
mod transcript;
mod tui;
mod ws;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kore-client",
    version,
    about = "kore thin client — connect AI agents to kore cloud"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Register this instance with the server and store its auth token
    Register {
        name: String,
        /// Project to join (own kore dir: $KORE_DIR/<project>/token)
        #[arg(long)]
        project: Option<String>,
        /// CLI tool driving this agent (default: claude)
        #[arg(long, default_value = "claude")]
        tool: String,
        /// Owner account of this agent (its owner name; @bigboss resolves to them)
        #[arg(long)]
        owner: Option<String>,
    },
    /// Create a human account (email + password); logs you in
    Signup {
        email: String,
        /// Your display name
        #[arg(long)]
        name: String,
        /// Join/create the org for your email's domain (business account)
        #[arg(long)]
        business: bool,
    },
    /// Log in to your human account
    Login { email: String },
    /// Register yourself (logged-in human) as an instance in a project —
    /// your network name is your account's owner name (server-assigned)
    Human {
        #[arg(long)]
        project: Option<String>,
    },
    /// Launch an AI agent connected to kore in the current directory
    Launch(commands::launch::LaunchArgs),
    /// Launch a new agent continuing this directory's most recent session
    Fork(commands::launch::LaunchArgs),
    /// Relaunch an agent continuing its previous session (looked up by name)
    Resume(commands::resume::ResumeArgs),
    /// Share materialized context with other agents (create/list/show/cat)
    #[command(subcommand)]
    Bundle(commands::bundle::BundleCmd),
    /// Manage project roles (behavioral text re-injected on a cadence)
    #[command(subcommand)]
    Role(commands::roles::RoleCmd),
    /// Manage project skills (auto-reminded on a cadence)
    #[command(subcommand)]
    Skill(commands::roles::SkillCmd),
    /// Interactive dashboard: agents, live messages, send (uses your identity)
    Tui,
    /// List projects in the org
    Projects,
    /// Send a message to agents
    Send(commands::send::SendArgs),
    /// Listen for incoming messages
    Listen {
        /// Positional timeout shorthand (secs): `listen 300`
        secs: Option<u64>,
        /// Stop after N seconds, exit 0 (0 = forever)
        #[arg(long, default_value_t = 0)]
        timeout: u64,
        /// One Delivery JSON object per line (NDJSON)
        #[arg(long)]
        json: bool,
    },
    /// List agents in the org with presence, or one agent's detail
    List {
        /// Show detail for one agent (base or {tag}-{name} form) instead of the roster
        name: Option<String>,
        /// Full roster as JSON (InstanceSummary array)
        #[arg(long)]
        json: bool,
        /// Bare instance names only, one per line (scripting)
        #[arg(long)]
        names: bool,
    },
    /// Show recent messages
    History {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        /// Only messages in this thread
        #[arg(long)]
        thread: Option<String>,
    },
    /// Typed event stream (life/status) and subscriptions
    Events(commands::events::EventsArgs),
    /// List named threads in your project, or rename one
    Threads {
        #[command(subcommand)]
        cmd: Option<ThreadsCmd>,
    },
    /// Remove an instance from the org (kicks its live connection)
    Unregister {
        name: String,
        /// Confirm when run from inside an agent (KORE_NAME set)
        #[arg(long)]
        go: bool,
    },
    /// Kill an agent for real: unregister + tombstone the name (its hooks
    /// can't re-register it until relaunch) + SIGTERM the local process
    Kill {
        name: String,
        /// Confirm when run from inside an agent (KORE_NAME set)
        #[arg(long)]
        go: bool,
    },
    /// Update kore-client to the latest released version
    Update,
    /// Set your "what I'm doing" status (shown in list)
    Status {
        #[arg(trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Mint an MCP credential for a NEW agent in your project (you become its
    /// owner). Prints the key ONCE + a ready-to-paste MCP config. For tools that
    /// can't run kore hooks; the agent connects with `kore-client mcp`.
    AgentKey {
        /// Agent name. Omit for an auto-generated CVCV name (like `launch`).
        name: Option<String>,
        /// Group tag (legacy label), same as launch --tag.
        #[arg(long)]
        tag: Option<String>,
    },
    /// Run as a stdio MCP server (Layer B transport): exposes kore as MCP tools
    /// (kore_send/list/history/status/check/wait) for agent tools that attach an
    /// MCP server instead of running kore hooks. Enrolls via KORE_AGENT_KEY if
    /// set (mint one with `agent-key`), else uses the stored token.
    Mcp,
    /// Agent-CLI hook integration (invoked by the agent CLI, not by hand)
    #[command(subcommand)]
    Hook(HookCommands),
    /// Internal (W2): poke a deaf agent's terminal when messages arrive —
    /// spawned by launch/hooks, not by hand
    #[command(hide = true)]
    Waker {
        name: String,
        tool_pid: u32,
        tool: String,
    },
}

#[derive(Subcommand)]
enum ThreadsCmd {
    /// Retitle a thread across all its messages (history/--thread follow the new name)
    Rename { from: String, to: String },
}

#[derive(Subcommand)]
enum HookCommands {
    /// Handle a Claude Code hook event (reads payload from stdin)
    Claude,
    /// Handle a Codex hook event (claude-compatible payload on stdin)
    Codex,
    /// Handle a Centaury Agent hook event (claude-compatible payload on stdin)
    Centaury,
    /// Handle a Gemini CLI hook event (event name passed as arg)
    Gemini { event: String },
    /// Handle an Antigravity hook event (gemini-family dialect, argv event)
    Antigravity { event: String },
    /// Handle a Cursor Agent hook event (argv event, JSON stdin, JSON stdout)
    Cursor { event: String },
    /// Handle a GitHub Copilot CLI hook event (argv event, JSON stdin/stdout)
    Copilot { event: String },
    /// Handle a Kimi Code CLI hook event (argv event, JSON stdin, exit 0/2)
    Kimi { event: String },
    /// Handle a Hermes Agent (Nous) shell-hook event (hook_event_name in
    /// stdin JSON like claude; JSON reply on stdout)
    Hermes,
    /// Handle a Pi Coding Agent extension event (session-start|wait|drain)
    Pi { event: String },
    /// Handle an oh-my-pi extension event (same dialect as pi)
    Omp { event: String },
    /// Handle an OpenCode plugin event (session-start|wait|drain)
    Opencode { event: String },
    /// Handle a Kilo Code plugin event (same dialect as opencode)
    Kilo { event: String },
    /// Handle a Cline hook-script event (session-start|drain)
    Cline { event: String },
    /// Handle an OpenClaw plugin event (session-start|drain; no wait — see plugin)
    Openclaw { event: String },
    /// Write kore hooks into the tool's settings
    Install {
        /// claude only: ~/.claude/settings.json instead of ./.claude/settings.json
        #[arg(long)]
        user: bool,
        /// Tool to install hooks for (claude|gemini|antigravity|codex|centaury|cursor|copilot|kimi|hermes|pi|omp|opencode|kilo|cline|openclaw)
        #[arg(long, default_value = "claude")]
        tool: String,
    },
    /// Show per tool whether kore hooks are installed, and where
    Status {
        /// claude only: check ~/.claude instead of ./.claude
        #[arg(long)]
        user: bool,
    },
    /// Remove kore hooks surgically (only kore entries leave shared configs)
    Uninstall {
        /// Tool to remove hooks from, or `all`
        #[arg(long, default_value = "claude")]
        tool: String,
        /// claude only: ~/.claude instead of ./.claude
        #[arg(long)]
        user: bool,
    },
}

/// Register with the server and store the token. Shared by the register
/// command and hook auto-registration (generated agents have no human to run
/// `register` by hand).
pub async fn do_register(
    name: &str,
    project: Option<&str>,
    tool: &str,
    owner: Option<String>,
) -> anyhow::Result<()> {
    let resp = config::http_client()
        .post(format!(
            "{}/v1/auth/register-instance",
            config::server_url()
        ))
        .json(&kore_protocol::api::RegisterRequest {
            name: name.to_string(),
            reg_secret: config::reg_secret()?,
            org: std::env::var("KORE_ORG").ok(),
            project: project.map(String::from),
            tag: std::env::var("KORE_TAG").ok().filter(|t| !t.is_empty()),
            tool: Some(tool.to_string()),
            directory: std::env::current_dir()
                .ok()
                .map(|d| d.display().to_string()),
            owner,
        })
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "register failed ({}): {}",
            resp.status(),
            resp.text().await?
        );
    }

    let body: kore_protocol::api::RegisterResponse = resp.json().await?;
    config::save_token(&body.token, project)?;
    Ok(())
}

/// D10: destructive verbs run from inside an agent (KORE_NAME set) need --go.
/// Exit 1 (legacy exited 0) so a scripted agent notices the refusal. Humans
/// (no agent env) never hit this. Client friction only — server-side authz
/// stays org-internal trust until OAuth2 roles (see server api.rs unregister).
fn gate_destructive(verb: &str, target: &str, go: bool) -> anyhow::Result<()> {
    if config::inside_agent_context() && !go {
        anyhow::bail!("{verb} '{target}' is destructive and you're an agent — add --go to confirm");
    }
    Ok(())
}

/// One-line unread hint after read-mostly verbs (G6): count only — never the
/// bodies, never a cursor advance. Silent on any error/timeout: a hint must
/// not turn a working command into a failing one.
async fn print_unread_hint() {
    commands::update::print_hint().await;
    let Ok(token) = config::load_token() else {
        return;
    };
    let Ok(resp) = config::http_client()
        .get(format!("{}/v1/messages/unread", config::server_url()))
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(v) = resp.json::<kore_protocol::api::UnreadResponse>().await else {
        return;
    };
    if v.count > 0 {
        println!("[kore] {} unread — kore-client history", v.count);
    }
}

pub async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> anyhow::Result<T> {
    let resp = config::http_client()
        .get(format!("{}{path}", config::server_url()))
        .bearer_auth(config::load_token()?)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("request failed ({}): {}", resp.status(), resp.text().await?);
    }
    Ok(resp.json().await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // bare `kore-client` → TUI, like legacy bare `kore`
    match cli.command.unwrap_or(Commands::Tui) {
        Commands::Register {
            name,
            project,
            tool,
            owner,
        } => {
            do_register(&name, project.as_deref(), &tool, owner).await?;
            match project {
                Some(p) => println!(
                    "registered as '{name}' in project '{p}' — export KORE_PROJECT={p} to use this identity"
                ),
                None => println!("registered as '{name}'"),
            }
        }
        Commands::Signup {
            email,
            name,
            business,
        } => commands::auth::signup(email, name, business).await?,
        Commands::Login { email } => commands::auth::login(email).await?,
        Commands::Human { project } => commands::auth::human(project).await?,
        Commands::Launch(args) => commands::launch::run(args).await?,
        Commands::Fork(args) => commands::launch::run_fork(args).await?,
        Commands::Resume(args) => commands::resume::run(args).await?,
        Commands::Bundle(cmd) => commands::bundle::run(cmd).await?,
        Commands::Role(cmd) => commands::roles::role(cmd).await?,
        Commands::Skill(cmd) => commands::roles::skill(cmd).await?,
        Commands::Tui => tui::run().await?,
        Commands::Projects => {
            let projects: Vec<kore_protocol::api::ProjectSummary> =
                get_json("/v1/projects").await?;
            for p in projects {
                println!("{:<20} {} agents", p.name, p.instance_count);
            }
            print_unread_hint().await;
        }
        Commands::Send(args) => {
            commands::send::run(args).await?;
            print_unread_hint().await;
        }
        Commands::Listen {
            secs,
            timeout,
            json,
        } => ws::listen(secs.unwrap_or(timeout), json).await?,
        Commands::List { name, json, names } => {
            let instances: Vec<kore_protocol::api::InstanceSummary> =
                get_json("/v1/instances").await?;
            // HC4: `list <name>` = detail card for one agent. Match base name
            // or the "{tag}-{name}" display form; the server already returns
            // every field the card shows, so this stays client-only.
            if let Some(target) = name {
                let hit = instances.iter().find(|i| {
                    i.name == target
                        || i.tag
                            .as_deref()
                            .filter(|t| !t.is_empty())
                            .map(|t| format!("{t}-{}", i.name))
                            == Some(target.clone())
                });
                let Some(i) = hit else {
                    eprintln!(
                        "no agent '{target}' in your project — `kore-client list` to see the roster"
                    );
                    std::process::exit(1);
                };
                if json {
                    println!("{}", serde_json::to_string_pretty(i)?);
                    return Ok(());
                }
                let shown = match i.tag.as_deref() {
                    Some(t) if !t.is_empty() => format!("{t}-{}", i.name),
                    _ => i.name.clone(),
                };
                let who = match (i.kind.as_str(), i.owner.as_deref()) {
                    ("human", _) => "human".to_string(),
                    (_, Some(o)) => format!("agent of @{o}"),
                    _ => "agent".to_string(),
                };
                println!("{shown}");
                println!(
                    "  status:    {}{}",
                    i.status,
                    if i.status_context.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", i.status_context)
                    }
                );
                println!("  kind:      {who}");
                if let Some(t) = &i.tool {
                    if !t.is_empty() {
                        println!("  tool:      {t}");
                    }
                }
                if let Some(d) = &i.directory {
                    if !d.is_empty() {
                        println!("  directory: {d}");
                    }
                }
                return Ok(());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&instances)?);
                return Ok(());
            }
            if names {
                for i in instances {
                    println!("{}", i.name);
                }
                return Ok(());
            }
            for i in instances {
                let who = match (i.kind.as_str(), i.owner.as_deref()) {
                    ("human", _) => "human".to_string(),
                    (_, Some(o)) => format!("of @{o}"),
                    _ => "agent".to_string(),
                };
                let doing = if i.status_context.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", i.status_context)
                };
                // Display name is "{tag}-{name}" (legacy): the tag is a group
                // label, not part of the registered name.
                let shown = match i.tag.as_deref() {
                    Some(t) if !t.is_empty() => format!("{t}-{}", i.name),
                    _ => i.name,
                };
                println!(
                    "{:<16} {:<9} {:<12} {:<8}{}",
                    shown,
                    i.status,
                    who,
                    i.tool.unwrap_or_default(),
                    doing
                );
            }
            print_unread_hint().await;
        }
        Commands::History { limit, thread } => {
            let mut path = format!("/v1/messages?limit={limit}");
            if let Some(t) = thread {
                path.push_str(&format!("&thread={t}"));
            }
            let messages: Vec<kore_protocol::api::Delivery> = get_json(&path).await?;
            for d in messages {
                let to = if d.message.mentions.is_empty() {
                    "all".to_string()
                } else {
                    format!("@{}", d.message.mentions.join(", @"))
                };
                println!("#{} {} → {}: {}", d.id, d.message.from, to, d.message.text);
            }
            print_unread_hint().await;
        }
        Commands::Events(args) => commands::events::run(args).await?,
        Commands::Threads { cmd } => match cmd {
            None => {
                let threads: Vec<kore_protocol::api::ThreadSummary> =
                    get_json("/v1/threads").await?;
                if threads.is_empty() {
                    println!("no named threads — send with --thread <name> to start one");
                }
                for t in threads {
                    println!(
                        "{:<28} {:>4} msgs  (last #{})",
                        t.name, t.message_count, t.last_msg_id
                    );
                }
            }
            Some(ThreadsCmd::Rename { from, to }) => {
                let resp = config::http_client()
                    .post(format!("{}/v1/threads/rename", config::server_url()))
                    .bearer_auth(config::load_token()?)
                    .json(&kore_protocol::api::RenameThreadRequest {
                        from: from.clone(),
                        to: to.clone(),
                    })
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    anyhow::bail!("rename failed ({}): {}", resp.status(), resp.text().await?);
                }
                let r: kore_protocol::api::RenameThreadResponse = resp.json().await?;
                println!("renamed '{from}' → '{to}' ({} messages)", r.renamed);
            }
        },
        Commands::Unregister { name, go } => {
            gate_destructive("unregister", &name, go)?;
            let resp = config::http_client()
                .delete(format!("{}/v1/instances/{name}", config::server_url()))
                .bearer_auth(config::load_token()?)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "unregister failed ({}): {}",
                    resp.status(),
                    resp.text().await?
                );
            }
            println!("unregistered '{name}'");
        }
        Commands::Kill { name, go } => {
            gate_destructive("kill", &name, go)?;
            let resp = config::http_client()
                .delete(format!(
                    "{}/v1/instances/{name}?kill=true",
                    config::server_url()
                ))
                .bearer_auth(config::load_token()?)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("kill failed ({}): {}", resp.status(), resp.text().await?);
            }
            match commands::launch::kill_local(&name) {
                Some(pid) => println!(
                    "killed '{name}' (signalled pid {pid}; name stays dead until relaunch)"
                ),
                None => println!(
                    "killed '{name}' (no local process found; name stays dead until relaunch)"
                ),
            }
        }
        Commands::Update => commands::update::run().await?,
        Commands::Status { text } => {
            let text = text.join(" ");
            let resp = config::http_client()
                .patch(format!("{}/v1/instances/self", config::server_url()))
                .bearer_auth(config::load_token()?)
                .json(&kore_protocol::api::SetStatusRequest {
                    status_context: text.clone(),
                })
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("status failed ({}): {}", resp.status(), resp.text().await?);
            }
            println!(
                "status: {}",
                if text.is_empty() { "(cleared)" } else { &text }
            );
        }
        Commands::AgentKey { name, tag } => {
            // No name → auto CVCV like `launch` (fresh per call: time+pid seed).
            let name = name.unwrap_or_else(config::random_name);
            let url = config::server_url();
            let resp = config::http_client()
                .post(format!("{url}/v1/instances/agents/keys"))
                .bearer_auth(config::load_token()?)
                .json(&kore_protocol::api::CreateAgentKeyRequest {
                    name,
                    project: None,
                    tag,
                })
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "agent-key failed ({}): {}",
                    resp.status(),
                    resp.text().await?
                );
            }
            let body: kore_protocol::api::CreateAgentKeyResponse = resp.json().await?;
            // Paste-ready MCP client config: the standard `mcpServers` block any
            // MCP app (Claude Desktop, Cursor, …) reads. serde builds it so the
            // key/url are correctly escaped.
            let cfg = serde_json::json!({
                "mcpServers": {
                    "kore": {
                        "command": "kore-client",
                        "args": ["mcp"],
                        "env": { "KORE_SERVER_URL": url, "KORE_AGENT_KEY": body.key }
                    }
                }
            });
            println!(
                "minted MCP credential for agent '{}' — shown ONCE, store it now.\n",
                body.name
            );
            println!("Paste this into your MCP client config (Claude Desktop, Cursor, …):\n");
            println!("{}\n", serde_json::to_string_pretty(&cfg)?);
            println!("Or set the env directly and run `kore-client mcp`:");
            println!("  KORE_SERVER_URL={url}");
            println!("  KORE_AGENT_KEY={}", body.key);
        }
        Commands::Mcp => commands::mcp::run().await?,
        Commands::Hook(HookCommands::Claude) => hook::run_stdin_dispatch("claude").await?,
        Commands::Hook(HookCommands::Codex) => hook::run_stdin_dispatch("codex").await?,
        Commands::Hook(HookCommands::Centaury) => hook::run_stdin_dispatch("centaury").await?,
        Commands::Hook(HookCommands::Gemini { event }) => {
            hook::run_argv_hook("gemini", &event).await?
        }
        Commands::Hook(HookCommands::Antigravity { event }) => {
            hook::run_argv_hook("antigravity", &event).await?
        }
        Commands::Hook(HookCommands::Cursor { event }) => hook::run_cursor_hook(&event).await?,
        Commands::Hook(HookCommands::Copilot { event }) => hook::run_copilot_hook(&event).await?,
        Commands::Hook(HookCommands::Kimi { event }) => hook::run_kimi_hook(&event).await?,
        Commands::Hook(HookCommands::Hermes) => hook::run_hermes_hook().await?,
        Commands::Hook(HookCommands::Pi { event }) => hook::run_plugin_tool("pi", &event).await?,
        Commands::Hook(HookCommands::Omp { event }) => hook::run_plugin_tool("omp", &event).await?,
        Commands::Hook(HookCommands::Opencode { event }) => {
            hook::run_plugin_tool("opencode", &event).await?
        }
        Commands::Hook(HookCommands::Kilo { event }) => {
            hook::run_plugin_tool("kilo", &event).await?
        }
        Commands::Hook(HookCommands::Cline { event }) => {
            hook::run_plugin_tool("cline", &event).await?
        }
        Commands::Hook(HookCommands::Openclaw { event }) => {
            hook::run_plugin_tool("openclaw", &event).await?
        }
        Commands::Hook(HookCommands::Install { user, tool }) => hook::install(&tool, user)?,
        Commands::Hook(HookCommands::Status { user }) => hook::status(user)?,
        Commands::Hook(HookCommands::Uninstall { tool, user }) => hook::uninstall(&tool, user)?,
        Commands::Waker {
            name,
            tool_pid,
            tool,
        } => commands::waker::run(name, tool_pid, tool).await?,
    }

    Ok(())
}
