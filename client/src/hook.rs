//! Agent-CLI hook integration — the push side of kore.
//!
//! The server is the inbox (watermark + replay), so hooks are thin: connect
//! the WebSocket, drain what the server replays/pushes, hand it to the agent.
//! Three logical events, one implementation, three tool dialects:
//!
//! - stop/turn-end (claude `Stop`, gemini `AfterAgent`, codex `Stop`): block
//!   up to KORE_HOOK_TIMEOUT secs waiting for messages; when they arrive,
//!   return `{"decision":"block","reason":<messages>}` (exit 2) so the
//!   agent's turn continues with the messages in context (legacy behavior).
//! - prompt (claude/codex `UserPromptSubmit`, gemini `BeforeAgent`): fast
//!   drain (~1s) injected as additionalContext.
//! - mid-turn (claude/codex `PostToolUse`, gemini `AfterTool` — W1): same
//!   fast drain between tool calls so messages reach a BUSY agent inside the
//!   turn, not after it. Never the blocking wait. Skipped inside subagents
//!   (agent_id present — G2). antigravity has NO mid-turn: its PostToolUse
//!   must emit exactly `{}` — cannot inject (legacy gemini.rs verified this).
//! - session start: auto-register + bootstrap context.
//!
//! Dialect notes (ported from legacy/src/hooks/):
//! - codex ≥0.129 speaks claude's hook protocol verbatim (`hook_event_name`
//!   in stdin, same output JSON) — only the install path differs.
//! - gemini ≥0.26 events are dispatched by argv (its stdin `hook_name` values
//!   are unreliable across versions — legacy did the same), and context
//!   injection needs a `"decision":"allow"` wrapper around hookSpecificOutput.
//!
//! Generated agents have no human to run `register`: if the token is missing
//! or rejected, hooks self-register (KORE_NAME, else a stable derived name)
//! and retry once.

use std::io::Read;
use std::time::Duration;

use kore_protocol::api::{Delivery, InjectItem, InstanceInjectConfig};

use crate::{config, ws};

const DEFAULT_STOP_TIMEOUT_SECS: u64 = 120;
/// After the first message lands, keep draining briefly to batch a burst.
const DRAIN_WINDOW: Duration = Duration::from_millis(500);

/// claude and codex: event name arrives in stdin JSON (same protocol).
pub async fn run_stdin_dispatch(tool: &str) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    dispatch_stdin_event(tool, &input).await
}

/// Subagent invariant (G2): Task subagents never reach the identity paths.
/// SessionStart fires only for real sessions (start/resume/clear), so
/// subagents can't auto-register; their end event is `SubagentStop`, matched
/// explicitly below as a no-op — it must NEVER route to `stop_poll`, which
/// would block the subagent AND ack the parent's inbox under the parent's
/// identity (subagents inherit KORE_* env).
async fn dispatch_stdin_event(tool: &str, input: &str) -> anyhow::Result<()> {
    let payload: serde_json::Value = serde_json::from_str(input).unwrap_or_default();
    let event = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match event {
        "Stop" => stop_poll(tool).await,
        "UserPromptSubmit" => prompt_drain(tool, "UserPromptSubmit").await,
        // W1 mid-turn drain: fires between tool calls, so queued messages
        // land inside the turn instead of after it. NEVER the blocking wait —
        // a blocked PostToolUse would freeze the turn. Subagents fire this
        // too (agent_id present only then) — same G2 invariant as
        // SubagentStop: they must never drain the parent's inbox.
        "PostToolUse" if payload.get("agent_id").is_none() => {
            prompt_drain(tool, "PostToolUse").await
        }
        "SessionStart" => session_start(tool, "SessionStart").await,
        "SubagentStop" => Ok(()), // see invariant above — silent, no wait, no ack
        _ => Ok(()),              // unknown event: succeed silently, never break the agent
    }
}

/// gemini + antigravity (gemini-family fork, same argv dialect + allow
/// wrapper): event name arrives as argv (stdin consumed by the CLI already).
pub async fn run_argv_hook(tool: &str, event: &str) -> anyhow::Result<()> {
    match event {
        "afteragent" => stop_poll(tool).await,
        "beforeagent" => prompt_drain(tool, "BeforeAgent").await,
        // W1 mid-turn drain, gemini only: antigravity's PostToolUse must emit
        // exactly `{}` (cannot inject; legacy gemini.rs) — its install never
        // wires aftertool, and this guard keeps a stray call harmless.
        "aftertool" if tool == "gemini" => prompt_drain(tool, "AfterTool").await,
        "sessionstart" => session_start(tool, "SessionStart").await,
        _ => Ok(()),
    }
}

/// Tools speaking gemini's output dialect (top-level `"decision":"allow"`).
fn gemini_family(tool: &str) -> bool {
    matches!(tool, "gemini" | "antigravity")
}

/// opencode/kilo (TS plugin) and cline (hook scripts): the plugin/script owns
/// injection, so this dialect prints bare text — no JSON envelope, no exit-2
/// blocking protocol. Same three moments, third dialect.
pub async fn run_plugin_tool(tool: &str, event: &str) -> anyhow::Result<()> {
    match event {
        "session-start" => {
            if config::load_token().is_err() || identity_mismatch() {
                let _ = auto_register(tool).await;
            }
            spawn_waker(tool);
            if let Ok(name) = config::instance_name() {
                println!("{}", session_bootstrap(&name).await);
            }
            Ok(())
        }
        "wait" => {
            if let Some(mut d) = collect_stop(tool).await {
                println!("{}", d.text);
                d.ack().await;
            }
            Ok(())
        }
        "drain" => {
            let extra = cadence_reminders(tool).await;
            match collect_drain(tool).await {
                Some(mut d) => {
                    println!("{}", join_extra(&d.text, &extra));
                    d.ack().await;
                }
                None if !extra.is_empty() => println!("{extra}"),
                None => {}
            }
            Ok(())
        }
        _ => Ok(()), // unknown event: succeed silently, never break the agent
    }
}

/// Cursor Agent (HC8, hcom 134e0ba): fourth dialect — event name as argv
/// (each hooks.json entry carries its own command), JSON payload on stdin,
/// JSON reply on stdout. sessionStart answers `additional_context`
/// (bootstrap), stop answers `followup_message` (cursor injects it as a new
/// turn — the blocking-wait slot), postToolUse answers `additional_context`
/// (W1 mid-turn drain). Unknown events answer `{}` — never break the host.
pub async fn run_cursor_hook(event: &str) -> anyhow::Result<()> {
    // Drain stdin so cursor never sees EPIPE writing the payload we don't need
    // (identity comes from KORE_* env, not session binding).
    let mut payload = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut payload);
    match event {
        "sessionstart" => {
            if config::load_token().is_err() || identity_mismatch() {
                let _ = auto_register("cursor").await;
            }
            spawn_waker("cursor");
            match config::instance_name() {
                Ok(name) => {
                    let boot = session_bootstrap(&name).await;
                    println!("{}", serde_json::json!({ "additional_context": boot }));
                }
                Err(_) => println!("{{}}"),
            }
        }
        "stop" => match collect_stop("cursor").await {
            Some(mut d) => {
                println!("{}", serde_json::json!({ "followup_message": d.text }));
                d.ack().await;
            }
            None => println!("{{}}"),
        },
        "posttooluse" => {
            let extra = cadence_reminders("cursor").await;
            match collect_drain("cursor").await {
                Some(mut d) => {
                    let text = join_extra(&d.text, &extra);
                    println!("{}", serde_json::json!({ "additional_context": text }));
                    d.ack().await;
                }
                None if !extra.is_empty() => {
                    println!("{}", serde_json::json!({ "additional_context": extra }))
                }
                None => println!("{{}}"),
            }
        }
        _ => println!("{{}}"),
    }
    Ok(())
}

/// GitHub Copilot CLI (HC9, hcom 2ad34cd): argv event + JSON stdin + JSON
/// stdout. sessionStart/postToolUse answer `additionalContext` (camelCase —
/// differs from cursor), Stop speaks claude's decision dialect
/// (`{"decision":"block","reason":<messages>}` injects a follow-up turn;
/// allow when nothing), PermissionRequest auto-allows safe centaury
/// commands so replies don't stall on shell prompts (copilot has no config
/// allow-rule file — the hook IS the permission surface).
pub async fn run_copilot_hook(event: &str) -> anyhow::Result<()> {
    let mut raw = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut raw);
    let payload: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    match event {
        "sessionstart" => {
            if config::load_token().is_err() || identity_mismatch() {
                let _ = auto_register("copilot").await;
            }
            spawn_waker("copilot");
            match config::instance_name() {
                Ok(name) => {
                    let boot = session_bootstrap(&name).await;
                    println!("{}", serde_json::json!({ "additionalContext": boot }));
                }
                Err(_) => println!("{{}}"),
            }
        }
        "stop" => match collect_stop("copilot").await {
            Some(mut d) => {
                println!(
                    "{}",
                    serde_json::json!({ "decision": "block", "reason": d.text })
                );
                d.ack().await;
            }
            None => println!("{}", serde_json::json!({ "decision": "allow" })),
        },
        "posttooluse" => {
            let extra = cadence_reminders("copilot").await;
            match collect_drain("copilot").await {
                Some(mut d) => {
                    let text = join_extra(&d.text, &extra);
                    println!("{}", serde_json::json!({ "additionalContext": text }));
                    d.ack().await;
                }
                None if !extra.is_empty() => {
                    println!("{}", serde_json::json!({ "additionalContext": extra }))
                }
                None => println!("{{}}"),
            }
        }
        "permissionrequest" => {
            let tool = payload
                .get("tool_name")
                .or_else(|| payload.get("toolName"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let input = payload
                .get("tool_input")
                .or_else(|| payload.get("toolInput"))
                .cloned()
                .unwrap_or_default();
            let command = ["command", "cmd", "script"]
                .iter()
                .find_map(|k| input.get(k).and_then(|v| v.as_str()))
                .unwrap_or("");
            if matches!(tool, "bash" | "powershell" | "shell") && kore_command_is_safe(command) {
                println!(
                    "{}",
                    serde_json::json!({ "behavior": "allow", "message": "kore coordination command" })
                );
            } else {
                println!("{{}}"); // copilot falls back to its own prompt
            }
        }
        _ => println!("{{}}"),
    }
    Ok(())
}

/// `centaury <safe verb> ...` and the bare `centaury` only.
fn kore_command_is_safe(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed == "centaury" {
        return true;
    }
    SAFE_KORE_VERBS.iter().any(|verb| {
        let expected = format!("centaury {verb}");
        trimmed == expected || trimmed.starts_with(&format!("{expected} "))
    })
}

/// Kimi Code CLI (HC10, hcom 7cbe0d9): argv event (kimi-sessionstart, …) +
/// JSON stdin + JSON stdout, claude-style exit codes (0 allow, 2 block).
/// sessionstart/posttooluse answer `{hookSpecificOutput:{message}}`; stop is
/// the blocking-wait slot → deny-decision (exit 2) carries the messages as
/// the reason. Unknown events answer `{}` exit 0.
pub async fn run_kimi_hook(event: &str) -> anyhow::Result<()> {
    let mut raw = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut raw);
    let exit = match event {
        "kimi-sessionstart" => {
            if config::load_token().is_err() || identity_mismatch() {
                let _ = auto_register("kimi").await;
            }
            spawn_waker("kimi");
            if let Ok(name) = config::instance_name() {
                let boot = session_bootstrap(&name).await;
                println!(
                    "{}",
                    serde_json::json!({ "hookSpecificOutput": { "message": boot } })
                );
            } else {
                println!("{{}}");
            }
            0
        }
        "kimi-posttooluse" => {
            let extra = cadence_reminders("kimi").await;
            match collect_drain("kimi").await {
                Some(mut d) => {
                    let text = join_extra(&d.text, &extra);
                    println!(
                        "{}",
                        serde_json::json!({ "hookSpecificOutput": { "message": text } })
                    );
                    d.ack().await;
                    0
                }
                None if !extra.is_empty() => {
                    println!(
                        "{}",
                        serde_json::json!({ "hookSpecificOutput": { "message": extra } })
                    );
                    0
                }
                None => {
                    println!("{{}}");
                    0
                }
            }
        }
        "kimi-stop" => match collect_stop("kimi").await {
            Some(mut d) => {
                println!(
                    "{}",
                    serde_json::json!({ "hookSpecificOutput": {
                        "permissionDecision": "deny",
                        "permissionDecisionReason": d.text,
                    } })
                );
                d.ack().await;
                2 // block = inject the reason as a follow-up turn
            }
            None => {
                println!("{{}}");
                0
            }
        },
        // subagent* + everything else: no-op success (G2 — never drain the
        // parent's inbox from a subagent).
        _ => {
            println!("{{}}");
            0
        }
    };
    if exit != 0 {
        std::process::exit(exit);
    }
    Ok(())
}

/// Hermes Agent by Nous (HC17): eighth dialect — ONE command for every
/// event (hermes pipes `hook_event_name` in stdin JSON, claude-style), JSON
/// reply on stdout, per-event response shapes (hermes agent/shell_hooks.py):
/// - `on_session_start`: return value IGNORED by hermes — register + waker,
///   then drop a pending-bootstrap marker; the bootstrap rides the FIRST
///   `pre_llm_call` instead (stateless hook processes bridge via a marker
///   file keyed by session_id).
/// - `pre_llm_call`: `{"context": text}` — fires before EVERY model call,
///   so it is prompt drain AND W1 mid-turn drain in one moment.
/// - `pre_verify`: the blocking-wait slot, but OPPORTUNISTIC — hermes fires
///   it only when the turn edited files (conversation_loop gate) and caps
///   continue-nudges per turn; `{"decision":"block","reason":<msgs>}` is
///   normalised by hermes into a continue-with-message follow-up. Hook
///   subprocesses are killed at 300s (MAX_TIMEOUT_SECONDS) — the wait stays
///   under that so the kill never eats an emit (a killed hook can't print;
///   D12 replay keeps the messages, but the turn they were meant for is gone).
/// - Turn-ends where `pre_verify` doesn't fire (no edits) are covered by
///   the W2 waker: poke → new turn → `pre_llm_call` drains. Documented
///   ceiling; the future upgrade is a hermes Python plugin using its
///   `inject_message` API (plugins.py) for true async delivery.
/// - G2: delegate-tool child sessions inherit KORE_* env — any payload
///   carrying `parent_session_id` is answered `{}` before ANY identity or
///   network path (claude's agent_id rule, hermes-shaped).
pub async fn run_hermes_hook() -> anyhow::Result<()> {
    let mut raw = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut raw);
    dispatch_hermes_event(&raw).await
}

/// Stay under hermes's 300s subprocess kill (MAX_TIMEOUT_SECONDS).
const HERMES_WAIT_BUDGET_SECS: u64 = 290;

async fn dispatch_hermes_event(input: &str) -> anyhow::Result<()> {
    let payload: serde_json::Value = serde_json::from_str(input).unwrap_or_default();
    if payload["parent_session_id"]
        .as_str()
        .is_some_and(|p| !p.is_empty())
    {
        println!("{{}}"); // G2 — never drain the parent's inbox from a subagent
        return Ok(());
    }
    match payload["hook_event_name"].as_str().unwrap_or("") {
        "on_session_start" => {
            if config::load_token().is_err() || identity_mismatch() {
                let _ = auto_register("hermes").await;
            }
            spawn_waker("hermes");
            if let Some(sid) = payload["session_id"].as_str() {
                mark_hermes_bootstrap_pending(sid);
            }
            println!("{{}}");
        }
        "pre_llm_call" => {
            let boot = match payload["session_id"]
                .as_str()
                .filter(|sid| take_hermes_bootstrap_pending(sid))
                .and_then(|_| config::instance_name().ok())
            {
                Some(name) => Some(session_bootstrap(&name).await),
                None => None,
            };
            let extra = cadence_reminders("hermes").await;
            match collect_drain("hermes").await {
                Some(mut d) => {
                    let text = match &boot {
                        Some(b) => format!("{b}\n\n{}", d.text),
                        None => d.text.clone(),
                    };
                    let text = join_extra(&text, &extra);
                    println!("{}", serde_json::json!({ "context": text }));
                    d.ack().await;
                }
                None => {
                    let text = join_extra(&boot.unwrap_or_default(), &extra);
                    if text.is_empty() {
                        println!("{{}}");
                    } else {
                        println!("{}", serde_json::json!({ "context": text }));
                    }
                }
            }
        }
        "pre_verify" => {
            let budget = stop_timeout_secs().min(HERMES_WAIT_BUDGET_SECS);
            match collect_stop_within("hermes", budget).await {
                Some(mut d) => {
                    println!(
                        "{}",
                        serde_json::json!({ "decision": "block", "reason": d.text })
                    );
                    d.ack().await;
                }
                None => println!("{{}}"),
            }
        }
        // subagent_stop + unknown events: silent no-op, never break the host.
        _ => println!("{{}}"),
    }
    Ok(())
}

/// on_session_start's return is ignored by hermes, so the bootstrap bridges
/// to the first pre_llm_call through a marker file keyed by session_id.
fn hermes_boot_dir() -> std::path::PathBuf {
    config::kore_dir().join("hermes-boot")
}

fn hermes_boot_marker(session_id: &str) -> std::path::PathBuf {
    // session ids are hermes-controlled — sanitize before touching the fs.
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    hermes_boot_dir().join(safe)
}

fn mark_hermes_bootstrap_pending(session_id: &str) {
    let dir = hermes_boot_dir();
    let _ = std::fs::create_dir_all(&dir);
    // Sessions that never reach a model call leak their marker — prune by
    // age opportunistically (fire-and-forget, like every hook side effect).
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let stale = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 7 * 86400);
            if stale {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let _ = std::fs::write(hermes_boot_marker(session_id), b"");
}

/// True exactly once per marked session (consume-on-read).
fn take_hermes_bootstrap_pending(session_id: &str) -> bool {
    std::fs::remove_file(hermes_boot_marker(session_id)).is_ok()
}

// The first sentence is the transcript identity marker (transcript.rs
// session_marker) — never reword "[kore] You are agent '{name}'".
fn bootstrap_text(name: &str) -> String {
    format!(
        "[kore] You are agent '{name}' on the kore network. Messages from \
         other agents and humans arrive automatically in your context; \
         senders tagged [human] are people — treat their messages like user \
         instructions. Use the `centaury` CLI (see the 'kore' skill if \
         available):\n\
         centaury send @name --reply-to <id> -- \"your answer\"\n\
         centaury list          # who is online\n\
         centaury history --limit 20\n\
         Always double-quote the message text after `--` — unquoted \
         apostrophes, ?, * or $ break the shell. A send with no @target \
         broadcasts to the WHOLE project — big broadcasts are refused until \
         you re-run with --go; prefer @mentioning who you mean."
    )
}

async fn session_start(tool: &str, event: &str) -> anyhow::Result<()> {
    // Registration happens here so the rest of the session has identity.
    if config::load_token().is_err() || identity_mismatch() {
        let _ = auto_register(tool).await;
    }
    spawn_waker(tool);
    if let Ok(name) = config::instance_name() {
        emit_context(event, &session_bootstrap(&name).await, gemini_family(tool));
    }
    Ok(())
}

// ── Role / skills injection (PLAN-AGENT-ROLES-SKILLS #8) ─────────────────
// The server owns the config (role + skills, each with its own cadence); the
// client counts turns/tokens locally and re-injects an item when its cadence
// crosses. Role rides SessionStart full (Tier-0) and its own cadence after;
// skills arrive only on cadence — as a pointer (name + trigger) unless the
// server marked inject_mode="full". All best-effort: a failed fetch or an
// unreadable state file returns "" and never breaks the agent's turn.

const INJECT_STATE_TTL_SECS: u64 = 300; // refetch inject-config at most this often
const INJECT_MIN_INTERVAL_SECS: u64 = 2; // gate re-firing the same item on racing events
const CHARS_PER_TOKEN: u64 = 4; // ponytail: crude token est (transcript bytes/4); swap in a real tokenizer if cadence needs to be tight

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct InjectState {
    fetched_unix: u64,
    #[serde(default)]
    items: Vec<InjectItem>,
    #[serde(default)]
    transcript_path: Option<String>,
    #[serde(default)]
    transcript_bytes: u64,
    #[serde(default)]
    counters: std::collections::HashMap<String, Counter>,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Counter {
    count: i64,
    last_unix: u64,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn inject_min_interval() -> u64 {
    std::env::var("KORE_INJECT_MIN_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(INJECT_MIN_INTERVAL_SECS)
}

fn inject_state_path(name: &str) -> std::path::PathBuf {
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    config::kore_dir()
        .join("inject")
        .join(format!("{safe}.json"))
}

fn load_inject_state(name: &str) -> InjectState {
    std::fs::read_to_string(inject_state_path(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_inject_state(name: &str, st: &InjectState) {
    let path = inject_state_path(name);
    let Some(dir) = path.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    let Ok(json) = serde_json::to_string(st) else {
        return;
    };
    // ponytail: tmp+rename for atomicity; last-writer-wins across racing hook
    // processes is fine — worst case a reminder lands one turn late.
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// pointer = name + trigger (lean; the body loads when the agent invokes it),
/// full = whole content. Role is always emitted full.
fn format_inject_item(item: &InjectItem) -> String {
    match (item.kind.as_str(), item.inject_mode.as_str()) {
        ("role", _) => format!("\n\n[kore role] {}\n{}", item.name, item.content),
        (_, "full") => format!("\n\n[kore skill] {}\n{}", item.name, item.content),
        _ => format!(
            "\n\n[kore skill] Use '{}' when relevant: {}",
            item.name, item.description
        ),
    }
}

/// Advance one item's counter for this event and report whether it is due.
/// Pure (no IO) so the threshold / reset / min-interval logic is unit-tested.
fn advance_counter(
    c: &mut Counter,
    cadence_kind: &str,
    cadence_value: i64,
    turn_inc: i64,
    token_delta: i64,
    now: u64,
    min_interval: u64,
) -> bool {
    c.count += if cadence_kind == "tokens" {
        token_delta
    } else {
        turn_inc
    };
    let due = c.count >= cadence_value && now.saturating_sub(c.last_unix) >= min_interval;
    if due {
        c.count = 0;
        c.last_unix = now;
    }
    due
}

/// Tokens that flowed through context since the last check, estimated from
/// transcript byte-growth (bytes/CHARS_PER_TOKEN). Returns 0 when the
/// transcript can't be located (plugin dialects, first observation) — those
/// items simply stay on messages-cadence. Resolved path is cached in state.
fn transcript_token_delta(tool: &str, name: &str, st: &mut InjectState) -> i64 {
    if st.transcript_path.is_none() {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some((_, path)) = crate::transcript::find_session(tool, name, &cwd) {
            st.transcript_path = Some(path.to_string_lossy().into_owned());
        }
    }
    let Some(path) = st.transcript_path.clone() else {
        return 0;
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        return 0;
    };
    let size = meta.len();
    // First observation sets the baseline — don't count pre-existing text.
    if st.transcript_bytes == 0 {
        st.transcript_bytes = size;
        return 0;
    }
    let delta = size.saturating_sub(st.transcript_bytes);
    st.transcript_bytes = size;
    (delta / CHARS_PER_TOKEN) as i64
}

/// SessionStart: bootstrap text + (best-effort) the role block. Fetches the
/// instance's inject-config, seeds cadence state, and appends the role full
/// (Tier-0). Skills are NOT injected here — they arrive on their cadence.
async fn session_bootstrap(name: &str) -> String {
    let mut s = bootstrap_text(name);
    if let Some(role) = seed_inject_state(name).await {
        s.push_str(&role);
    }
    s
}

async fn seed_inject_state(name: &str) -> Option<String> {
    // Consume KORE_ROLE/KORE_SKILLS from `launch --role/--skill` before we read
    // the config, so this session picks up what we just self-assigned.
    self_assign_from_env().await;
    let cfg = crate::get_json::<InstanceInjectConfig>("/v1/instances/self/inject-config")
        .await
        .ok()?;
    let now = now_unix();
    let mut st = InjectState {
        fetched_unix: now,
        items: cfg.items.clone(),
        ..Default::default()
    };
    // Seed the role counter at "now" so its cadence measures from session start
    // and the role we inject here doesn't immediately re-fire on turn 1.
    if let Some(r) = cfg.items.iter().find(|i| i.kind == "role") {
        st.counters.insert(
            format!("role:{}", r.name),
            Counter {
                count: 0,
                last_unix: now,
            },
        );
    }
    save_inject_state(name, &st);
    cfg.items
        .iter()
        .find(|i| i.kind == "role")
        .map(format_inject_item)
}

/// Self-assign role/skills from KORE_ROLE/KORE_SKILLS (set by `launch
/// --role/--skill`). Runs as the agent, so the self endpoints use its own
/// token — the identity is naturally correct. Best-effort: specs were already
/// validated at launch, and a failure here must never break the session.
async fn self_assign_from_env() {
    let Ok(token) = config::load_token() else {
        return;
    };
    if let Ok(spec) = std::env::var("KORE_ROLE")
        && let Ok((title, kind, value)) = crate::cadence::parse_role_spec(&spec)
    {
        let _ = config::http_client()
            .patch(format!("{}/v1/instances/self/role", config::server_url()))
            .bearer_auth(&token)
            .json(&kore_protocol::api::AssignRoleRequest {
                role: Some(title),
                cadence_kind: Some(kind),
                cadence_value: Some(value),
            })
            .send()
            .await;
    }
    if let Ok(raw) = std::env::var("KORE_SKILLS") {
        let skills: Vec<_> = raw
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| crate::cadence::parse_skill_spec(s).ok())
            .collect();
        if !skills.is_empty() {
            let _ = config::http_client()
                .put(format!("{}/v1/instances/self/skills", config::server_url()))
                .bearer_auth(&token)
                .json(&kore_protocol::api::AssignSkillsRequest { skills })
                .send()
                .await;
        }
    }
}

/// Recurring events (prompt / mid-turn drain): advance every item's cadence
/// counter and return the reminders that crossed threshold ("" when none).
async fn cadence_reminders(tool: &str) -> String {
    cadence_reminders_inner(tool).await.unwrap_or_default()
}

async fn cadence_reminders_inner(tool: &str) -> Option<String> {
    let name = auto_name();
    let mut st = load_inject_state(&name);
    let now = now_unix();
    // Keep the hot mid-turn path off the network: refetch at most every TTL.
    if (st.fetched_unix == 0 || now.saturating_sub(st.fetched_unix) > INJECT_STATE_TTL_SECS)
        && let Ok(cfg) =
            crate::get_json::<InstanceInjectConfig>("/v1/instances/self/inject-config").await
    {
        st.items = cfg.items;
        st.fetched_unix = now;
    }
    if st.items.is_empty() {
        save_inject_state(&name, &st);
        return None;
    }
    let token_delta = transcript_token_delta(tool, &name, &mut st);
    let min_interval = inject_min_interval();
    let items = st.items.clone();
    let mut out = String::new();
    for item in &items {
        let key = format!("{}:{}", item.kind, item.name);
        let c = st.counters.entry(key).or_default();
        if advance_counter(
            c,
            &item.cadence_kind,
            item.cadence_value,
            1,
            token_delta,
            now,
            min_interval,
        ) {
            out.push_str(&format_inject_item(item));
        }
    }
    save_inject_state(&name, &st);
    (!out.is_empty()).then_some(out)
}

/// Append cadence reminders to a drain/bootstrap payload (both may be empty).
fn join_extra(text: &str, extra: &str) -> String {
    if extra.is_empty() {
        text.to_string()
    } else {
        format!("{text}{extra}")
    }
}

/// W2: give MANUAL sessions a waker too — this hook fires on manual opens
/// (it's already how they auto-register) and runs inside the agent's
/// terminal, so the poke env ($TMUX_PANE & co) is correct right here. The
/// hook's parent process IS the tool. The waker dedups itself via its pid
/// file, so launched agents (which got one from `launch`) spawn no second
/// one. Fire-and-forget: never break the agent.
#[cfg(not(unix))]
fn spawn_waker(_tool: &str) {} // waker pokes tmux/wezterm/kitty panes — unix-only
#[cfg(unix)]
fn spawn_waker(tool: &str) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let ppid = std::os::unix::process::parent_id();
    let _ = std::process::Command::new(exe)
        .args(["waker", &auto_name(), &ppid.to_string(), tool])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Identity for agents nobody registers by hand: KORE_NAME, else the name in
/// the existing (possibly stale) token, else a stable name derived from
/// host+cwd+project — deterministic so retries never mint duplicates.
fn auto_name() -> String {
    if let Ok(name) = std::env::var("KORE_NAME") {
        return name;
    }
    if let Ok(name) = config::instance_name() {
        return name;
    }
    config::derived_name()
}

/// KORE_NAME says we should be someone else than the stored token — stale or
/// foreign token file; re-register instead of impersonating.
fn identity_mismatch() -> bool {
    match (std::env::var("KORE_NAME"), config::instance_name()) {
        (Ok(want), Ok(have)) => want != have,
        _ => false,
    }
}

async fn auto_register(tool: &str) -> anyhow::Result<()> {
    let project = std::env::var("KORE_PROJECT").ok();
    let owner = std::env::var("KORE_OWNER").ok();
    crate::do_register(&auto_name(), project.as_deref(), tool, owner).await
}

/// Fire-and-forget presence: never let a status update break the hook.
async fn post_status(text: &str) {
    let Ok(token) = config::load_token() else {
        return;
    };
    let _ = config::http_client()
        .patch(format!("{}/v1/instances/self", config::server_url()))
        .bearer_auth(token)
        .json(&kore_protocol::api::SetStatusRequest {
            status_context: text.to_string(),
        })
        .send()
        .await;
}

/// Connect; on a missing or rejected token, self-register once and retry.
async fn connect_or_register(tool: &str) -> anyhow::Result<ws::WsStream> {
    if config::load_token().is_err() || identity_mismatch() {
        auto_register(tool).await?;
    }
    match ws::connect(false).await {
        Ok(s) => Ok(s),
        Err(first_err) => {
            if config::reg_secret().is_err() {
                return Err(first_err); // can't re-register without the secret
            }
            auto_register(tool).await?;
            ws::connect(false).await
        }
    }
}

/// Stop/turn-end hook: wait for messages, deliver as block reason so the turn
/// continues. The block JSON is identical for claude, codex and gemini.
async fn stop_poll(tool: &str) -> anyhow::Result<()> {
    let Some(mut d) = collect_stop(tool).await else {
        return Ok(());
    };
    println!(
        "{}",
        serde_json::json!({ "decision": "block", "reason": d.text })
    );
    d.ack().await; // after emit (D12): a death before this line = replay, not loss
    std::process::exit(2);
}

fn stop_timeout_secs() -> u64 {
    std::env::var("KORE_HOOK_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_STOP_TIMEOUT_SECS)
}

/// W3 wait-marker: "a blocking wait holds the consuming socket — incoming
/// messages will inject themselves, a waker poke would be noise." Written
/// when the Stop wait engages, removed on Drop so every exit path (message,
/// timeout, error) cleans up. A crashed hook can still leak the file; that's
/// why `wait_marker_active` also checks age.
struct WaitMarker(std::path::PathBuf);

impl WaitMarker {
    fn engage() -> Option<Self> {
        let path =
            config::agent_wait_path(&auto_name(), std::env::var("KORE_PROJECT").ok().as_deref());
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, b"").ok()?;
        Some(Self(path))
    }
}

impl Drop for WaitMarker {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Waker-side check (W2 step 3): marker present AND younger than the hook
/// timeout. Older = leaked by a crashed hook, treated absent.
pub fn wait_marker_active(name: &str, project: Option<&str>) -> bool {
    let Ok(meta) = std::fs::metadata(config::agent_wait_path(name, project)) else {
        return false;
    };
    meta.modified()
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age.as_secs() < stop_timeout_secs())
}

/// Collected messages plus what acking them needs (D12): the consuming
/// socket stays open until the caller has EMITTED the text to the tool —
/// only then may the server's watermark pass these ids. Ack after emit,
/// never before; a hook death in between means replay, not loss.
struct Drained {
    text: String,
    last_id: i64,
    socket: ws::WsStream,
}

impl Drained {
    fn new(messages: &[Delivery], socket: ws::WsStream) -> Self {
        let last_id = messages.iter().map(|d| d.id).max().unwrap_or(0);
        Self {
            text: format_messages(messages),
            last_id,
            socket,
        }
    }

    /// Fire-and-forget: a failed ack just means re-delivery on reconnect.
    async fn ack(&mut self) {
        let _ = ws::send_ack(&mut self.socket, self.last_id).await;
    }
}

/// Blocking-wait core shared by every dialect: wait up to KORE_HOOK_TIMEOUT
/// for a first message, batch the burst, return formatted text. None on
/// timeout or unreachable server — the agent stops normally.
async fn collect_stop(tool: &str) -> Option<Drained> {
    collect_stop_within(tool, stop_timeout_secs()).await
}

/// The same wait with an explicit budget: hermes kills hook subprocesses at
/// its own 300s cap, so its dialect must exit first — a killed hook can't
/// emit, and D12 replay can't resurrect the turn the messages were meant for.
async fn collect_stop_within(tool: &str, timeout_secs: u64) -> Option<Drained> {
    let mut socket = connect_or_register(tool).await.ok()?;
    let _wait = WaitMarker::engage();
    post_status("idle").await;

    let first = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        ws::next_delivery(&mut socket),
    )
    .await;

    let mut messages = match first {
        Ok(Ok(Some(d))) => vec![d],
        _ => return None, // timeout, closed or error: normal stop
    };
    drain_into(&mut socket, &mut messages).await;
    post_status("working").await; // the turn continues with these messages
    Some(Drained::new(&messages, socket))
}

/// Prompt hook: quick drain, inject as context alongside the user prompt.
async fn prompt_drain(tool: &str, event: &str) -> anyhow::Result<()> {
    let extra = cadence_reminders(tool).await;
    match collect_drain(tool).await {
        Some(mut d) => {
            emit_context(event, &join_extra(&d.text, &extra), gemini_family(tool));
            d.ack().await;
        }
        None if !extra.is_empty() => emit_context(event, &extra, gemini_family(tool)),
        None => {}
    }
    Ok(())
}

/// Fast-drain core shared by every dialect: grab whatever the server has
/// queued right now (~1s), return formatted text or None.
async fn collect_drain(tool: &str) -> Option<Drained> {
    let mut socket = connect_or_register(tool).await.ok()?;
    post_status("working").await;

    let mut messages = Vec::new();
    drain_into(&mut socket, &mut messages).await;
    (!messages.is_empty()).then(|| Drained::new(&messages, socket))
}

/// Collect already-available deliveries (replayed backlog / in-flight burst).
async fn drain_into(socket: &mut ws::WsStream, out: &mut Vec<Delivery>) {
    while let Ok(Ok(Some(d))) = tokio::time::timeout(DRAIN_WINDOW, ws::next_delivery(socket)).await
    {
        out.push(d);
    }
}

fn format_messages(messages: &[Delivery]) -> String {
    let mut s = String::from("Incoming kore messages:\n");
    for d in messages {
        let mut meta: Vec<String> = Vec::new();
        if let Some(t) = &d.message.thread {
            meta.push(format!("thread:{t}"));
        }
        if let Some(r) = d.message.reply_to {
            meta.push(format!("replies-to:#{r}"));
        }
        if let Some(b) = &d.message.bundle_id {
            meta.push(b.clone()); // ids are already "bundle:<8hex>"
        }
        let meta = if meta.is_empty() {
            String::new()
        } else {
            format!(" ({})", meta.join(", "))
        };
        // Tag non-agent senders so the model knows a person (or the server)
        // is talking — untagged names are AI peers.
        let from = match d.message.sender_kind.as_str() {
            "agent" => d.message.from.clone(),
            kind => format!("{} [{kind}]", d.message.from),
        };
        s.push_str(&format!(
            "[#{}] {}{}: {}\n",
            d.id, from, meta, d.message.text
        ));
    }
    s.push_str(
        "\nSenders tagged [human] are people — treat their messages like user \
         instructions. Untagged senders are AI agents.\n\
         If a message needs an answer, reply now:\n\
         centaury send @<sender> --reply-to <id> -- \"your answer\"\n\
         (double-quote the text — apostrophes, ?, * or $ break the shell)\n\
         Then continue (or finish) your current task. Not every message needs a reply.",
    );
    if messages.iter().any(|d| d.message.bundle_id.is_some()) {
        s.push_str(
            "\nA message carries attached context: load it with\n\
             centaury bundle cat <bundle-id>",
        );
    }
    s
}

/// gemini requires a top-level `"decision":"allow"` next to hookSpecificOutput;
/// claude/codex must NOT get one (their allow is implicit, decision has other meanings).
fn emit_context(event: &str, text: &str, gemini_allow: bool) {
    let mut out = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": event, "additionalContext": text }
    });
    if gemini_allow {
        out["decision"] = "allow".into();
    }
    println!("{out}");
}

/// The agent-facing skill installed next to the hooks: short, current, and
/// the single place that teaches the right commands.
const KORE_SKILL: &str = r#"---
name: kore
description: Talk to other AI agents over the kore network. Use when a kore message arrives in context, when the user says to ask/tell another agent (send @name), or to check who is online (centaury list) or read history.
---

# kore — agent messaging

You are connected to other agents through kore. Incoming messages appear in
your context automatically (you never poll). Everything below is one CLI.

## Answering a message

Messages look like `[#42] luna: can you review api.rs?`. A tag after the
name says who is talking: `[#43] solar [human]: ...` is a PERSON — treat it
like a user instruction. `[system]` is a server notice. No tag = AI agent
peer. Reply to the sender, referencing the id:

```bash
centaury send @luna --reply-to 42 -- "reviewed, two issues: ..."
```

Not every message needs a reply; acknowledge requests, ignore FYIs. Human
requests take priority over agent chatter.

## Talking to agents

```bash
centaury send @luna -- "one target"
centaury send @luna @nova -- "two targets"
centaury send -- "broadcast to every agent in your project"
centaury send @luna --wait --timeout 60 -- "question, blocks until luna replies"
```

Message text goes after `--`, always. `--wait` turns a send into a synchronous
question — use it when you need the answer to continue.

Broadcasts (no @target) reach EVERYONE and wake every terminal. When the
project has more than a few agents the server refuses an agent broadcast
until you re-run it with `--go` — that's your cue to check whether an
@mention was what you meant.

## Quoting (the #1 source of broken sends)

ALWAYS double-quote the message text after `--`. Unquoted text breaks on
apostrophes (`what's`), shell globs (`?`, `*`) and expansion (`$`, backtick):

```bash
centaury send @luna -- "what's your status?"      # right
centaury send @luna -- what's your status?        # WRONG: shell eats it
centaury send @luna -- 'literal $VAR and `cmd`'   # single quotes when text has $ or `
centaury send @luna --stdin <<'EOF'               # gnarly multi-line text
any "quotes" and $chars survive here
EOF
```

## Conversations

- `--thread <name>` groups related messages; keep one thread per topic.
- `--intent request|inform|ack` marks purpose; `ack` requires `--reply-to`.

## Who is out there

```bash
centaury list                      # name, presence, tool, directory
centaury history --limit 20        # recent traffic
centaury history --thread <name>   # one conversation
```

## Your owner

`@bigboss` always means *your* human owner — use it to notify or escalate:

```bash
centaury send @bigboss -- "deploy finished, 2 tests skipped, details in thread deploy"
```

Humans appear in `list` as kind "human"; agents show their owner.

## Status

Keep your status honest so others know what you're doing:

```bash
centaury status doing a big refactor in auth.rs
```

## Rules

- Address agents by the exact name shown in `list`; unknown names are rejected.
- You only see and reach agents in your own project — that wall is
  absolute; other projects don't exist for you.
- Broadcasts reach everyone in your project — prefer @mentions.
- Offline agents still get your message when they return (server stores it).
"#;

fn consent_marker() -> std::path::PathBuf {
    crate::config::kore_dir().join("hooks-consent")
}

/// Record the one-time hook-install consent (an explicit `hook install` IS
/// consent — main.rs calls this before install).
pub fn record_install_consent() -> anyhow::Result<()> {
    record_consent_at(&consent_marker())
}

fn record_consent_at(marker: &std::path::Path) -> anyhow::Result<()> {
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(marker, "")?;
    Ok(())
}

/// Hook install writes into the TOOL's own config files — often user-global
/// (~/.gemini, ~/.codex, ~/.cursor, ~/.kimi-code, ...). Nothing writes there
/// until the user says yes once: prompt on a tty, bail otherwise.
pub fn ensure_install_consent(tool: &str) -> anyhow::Result<()> {
    ensure_consent_at(&consent_marker(), tool)
}

fn ensure_consent_at(marker: &std::path::Path, tool: &str) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if marker.exists() {
        return Ok(());
    }
    if std::io::stdin().is_terminal() {
        eprint!(
            "centaury will install {tool} hooks + skill into {tool}'s config files \
             (one-time consent; `centaury hook uninstall --tool all` reverts). Continue? [y/N] "
        );
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if matches!(line.trim(), "y" | "Y" | "yes") {
            return record_consent_at(marker);
        }
    }
    anyhow::bail!(
        "hooks not installed — run `centaury hook install --tool {tool}` once to consent, \
         or launch with --no-hooks"
    )
}

/// Install kore hooks for a tool. claude honors user_scope; gemini/codex only
/// have global config dirs; opencode/kilo/cline install project-local files.
/// Consent-gated: first ever install asks (or bails when non-interactive).
pub fn install(tool: &str, user_scope: bool) -> anyhow::Result<()> {
    ensure_install_consent(tool)?;
    match tool {
        "claude" => install_claude(user_scope),
        "gemini" => install_gemini(),
        "antigravity" => install_antigravity(),
        "codex" => install_codex(),
        "centaury" => install_centaury(),
        "cursor" | "cursor-agent" => install_cursor(),
        "copilot" => install_copilot(),
        "kimi" => install_kimi(),
        "hermes" => install_hermes(),
        "pi" | "omp" => install_pi(tool),
        "opencode" => install_ts_plugin("opencode", ".opencode/plugin"),
        "kilo" | "kilocode" => install_ts_plugin("kilo", ".kilocode/plugins"),
        "cline" => install_cline(),
        "openclaw" => install_openclaw(),
        other => anyhow::bail!(
            "no hook support for tool '{other}' yet ({})",
            SUPPORTED_TOOLS.join("|")
        ),
    }
}

/// Claude events kore wires (shared by install/status/uninstall — D8's
/// legacy invariant: the three verbs must never disagree about what
/// "installed" means, so they read the same list).
const CLAUDE_HOOK_EVENTS: &[(&str, u64)] = &[
    // Stop needs a generous command timeout: it deliberately blocks waiting.
    ("Stop", 86400),
    ("UserPromptSubmit", 30),
    ("SessionStart", 30),
    ("PostToolUse", 30), // W1 mid-turn drain
];

const CLAUDE_ALLOW_RULE: &str = "Bash(centaury:*)";

fn claude_base(user_scope: bool) -> anyhow::Result<std::path::PathBuf> {
    Ok(if user_scope {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("no home dir"))?
            .join(".claude")
    } else {
        std::path::PathBuf::from(".claude")
    })
}

/// Merge kore hooks + the permission rule into a claude settings object.
/// Pure so it's testable (round-trips with `strip_claude_kore`).
fn merge_claude_hooks(settings: &mut serde_json::Value, exe: &str) -> anyhow::Result<()> {
    let entry = |timeout: u64| serde_json::json!([{ "hooks": [{ "type": "command", "command": format!("{exe} hook claude"), "timeout": timeout }] }]);

    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks key is not an object"))?;

    for &(event, timeout) in CLAUDE_HOOK_EVENTS {
        hooks.insert(event.into(), entry(timeout));
    }

    // Without this, every reply an agent tries to send stalls on a Bash
    // permission prompt — headless agents can't approve, so messages look
    // "never received" (they arrived; the answer never left).
    let allow = settings
        .as_object_mut()
        .unwrap() // checked object above
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions key is not an object"))?
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]));
    let allow = allow
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.allow is not an array"))?;
    let rule = serde_json::json!(CLAUDE_ALLOW_RULE);
    if !allow.contains(&rule) {
        allow.push(rule);
    }
    Ok(())
}

/// Merge kore hooks into Claude Code settings.json and install the kore skill.
fn install_claude(user_scope: bool) -> anyhow::Result<()> {
    let base = claude_base(user_scope)?;
    let settings_path = base.join("settings.json");

    let before = std::fs::read_to_string(&settings_path).ok();
    let mut settings: serde_json::Value = before
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    merge_claude_hooks(
        &mut settings,
        &std::env::current_exe()?.display().to_string(),
    )?;

    let rendered = serde_json::to_string_pretty(&settings)?;
    let skill_dir = base.join("skills/kore");
    let skill_path = skill_dir.join("SKILL.md");
    // Idempotent + quiet: `launch` calls this every time; only a real change
    // (first install, upgraded skill text, moved binary) deserves a line.
    let unchanged = before.as_deref() == Some(rendered.as_str())
        && std::fs::read_to_string(&skill_path).as_deref().ok() == Some(KORE_SKILL);
    if unchanged {
        return Ok(());
    }

    std::fs::create_dir_all(&base)?;
    std::fs::write(&settings_path, rendered)?;
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(&skill_path, KORE_SKILL)?;

    println!(
        "kore installed: hooks → {}, skill → {}",
        settings_path.display(),
        skill_path.display()
    );
    Ok(())
}

// ---- centaury (Centaury Agent — first-party; speaks claude's hook dialect
// natively, so the stdin-dispatch handler is reused verbatim like codex) ----

fn centaury_hooks_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/centaury/hooks.json")
}

/// Same entry shape as claude's settings.json `hooks` key — centaury reads
/// it directly. Pure so it round-trips with `strip_kore_entries` in tests.
fn merge_centaury_hooks(root: &mut serde_json::Value, exe: &str) -> anyhow::Result<()> {
    let hooks = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks key is not an object"))?;
    for &(event, timeout) in CLAUDE_HOOK_EVENTS {
        hooks.insert(
            event.into(),
            serde_json::json!([{ "hooks": [{ "type": "command", "command": format!("{exe} hook centaury"), "timeout": timeout }] }]),
        );
    }
    Ok(())
}

fn install_centaury() -> anyhow::Result<()> {
    let path = centaury_hooks_path();
    let before = std::fs::read_to_string(&path).ok();
    let mut root: serde_json::Value = before
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    merge_centaury_hooks(&mut root, &std::env::current_exe()?.display().to_string())?;

    let rendered = serde_json::to_string_pretty(&root)?;
    if before.as_deref() == Some(rendered.as_str()) {
        return Ok(()); // idempotent + quiet, like claude's install
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, rendered)?;
    println!("kore installed: centaury hooks → {}", path.display());
    Ok(())
}

// ---- gemini (≥0.26 hooks; shapes ported from legacy/src/hooks/gemini.rs) ----

/// (settings key, argv event, timeout ms). AfterAgent deliberately blocks.
const GEMINI_HOOKS: &[(&str, &str, u64)] = &[
    ("SessionStart", "sessionstart", 30_000),
    ("BeforeAgent", "beforeagent", 30_000),
    ("AfterTool", "aftertool", 30_000), // W1 mid-turn drain
    ("AfterAgent", "afteragent", 86_400_000),
];

/// Merge kore hooks into a gemini settings object. Pure so it's testable;
/// idempotent: strips previous kore- entries before adding.
fn merge_gemini_hooks(settings: &mut serde_json::Value, exe: &str) {
    let root = match settings.as_object_mut() {
        Some(o) => o,
        None => {
            *settings = serde_json::json!({});
            settings.as_object_mut().unwrap()
        }
    };

    // Gemini needs both switches on or hooks never fire.
    root.entry("tools").or_insert_with(|| serde_json::json!({}))["enableHooks"] = true.into();
    root.entry("hooksConfig")
        .or_insert_with(|| serde_json::json!({}))["enabled"] = true.into();

    let hooks = root.entry("hooks").or_insert_with(|| serde_json::json!({}));
    for &(event, suffix, timeout) in GEMINI_HOOKS {
        let arr = hooks[event].as_array().cloned().unwrap_or_default();
        let mut arr: Vec<serde_json::Value> = arr
            .into_iter()
            .filter(|e| {
                e["hooks"][0]["name"]
                    .as_str()
                    .is_none_or(|n| !n.starts_with("kore-"))
            })
            .collect();
        arr.push(serde_json::json!({
            "matcher": "*",
            "hooks": [{
                "name": format!("kore-{suffix}"),
                "type": "command",
                "command": format!("{exe} hook gemini {suffix}"),
                "timeout": timeout,
            }]
        }));
        hooks[event] = arr.into();
    }
}

/// GEMINI_CLI_HOME is a HOME override, not the config dir: gemini always
/// appends `.gemini` (verified against gemini 0.40 + legacy runtime_env.rs).
fn gemini_config_dir() -> std::path::PathBuf {
    std::env::var("GEMINI_CLI_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join(".gemini")
}

fn install_gemini() -> anyhow::Result<()> {
    let dir = gemini_config_dir();
    let path = dir.join("settings.json");
    let mut settings: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    merge_gemini_hooks(
        &mut settings,
        &std::env::current_exe()?.display().to_string(),
    );
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    println!(
        "kore installed: gemini hooks → {} (needs gemini ≥0.26)",
        path.display()
    );
    Ok(())
}

// ---- antigravity (gemini-family fork; shapes from legacy/src/hooks/antigravity.rs) ----

/// Merge the "kore-lifecycle" group into an antigravity hooks.json root.
/// Pure so it's testable; overwrites only the kore-lifecycle key, other keys
/// survive. Entries are flat (name/type/command/timeout), NOT gemini's
/// {matcher, hooks:[...]} nesting. Legacy also wired PreToolUse/PostToolUse/
/// Stop — the new stack's 3-moment model doesn't need them.
fn merge_antigravity_hooks(root: &mut serde_json::Value, exe: &str) {
    let obj = match root.as_object_mut() {
        Some(o) => o,
        None => {
            *root = serde_json::json!({});
            root.as_object_mut().unwrap()
        }
    };
    let entry = |event: &str, timeout_secs: u64, desc: &str| {
        serde_json::json!({
            "name": format!("kore-{event}"),
            "type": "command",
            "command": format!("{exe} hook antigravity {event}"),
            "timeout": timeout_secs,
            "description": desc,
        })
    };
    obj.insert(
        "kore-lifecycle".into(),
        serde_json::json!({
            // PreInvocation re-fires per turn; session_start is idempotent.
            "PreInvocation": [
                entry("sessionstart", 30, "Register with kore + bootstrap"),
                entry("beforeagent", 30, "Deliver pending kore messages"),
            ],
            // Blocking wait, like claude's Stop — needs the generous timeout.
            "PostInvocation": [entry("afteragent", 86400, "Wait for kore messages")],
        }),
    );
}

/// antigravity stores hooks at `<gemini dir>/config/hooks.json` (split-config
/// design, same base dir as gemini).
fn install_antigravity() -> anyhow::Result<()> {
    let dir = gemini_config_dir().join("config");
    let path = dir.join("hooks.json");
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    merge_antigravity_hooks(&mut root, &std::env::current_exe()?.display().to_string());
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    update_antigravity_permissions(true)?;
    println!("kore installed: antigravity hooks → {}", path.display());
    Ok(())
}

/// antigravity's settings.json — its OWN permission surface, distinct from
/// gemini-cli's TOML policy engine (HC13, hcom 5f427b8). Lives beside the
/// gemini dir under `antigravity-cli/`.
fn antigravity_settings_path() -> std::path::PathBuf {
    gemini_config_dir()
        .join("antigravity-cli")
        .join("settings.json")
}

/// agy auto-approves `run_command` by matching `command(<prefix>)` entries in
/// `permissions.allow` (the same format it writes when the user picks "always
/// allow"). Without them every centaury reply prompts and messages look
/// "never received" — same failure mode as cursor/kimi/copilot, gemini's TOML
/// policy does NOT cover agy's run_command (upstream-verified).
fn antigravity_permission_rules() -> Vec<String> {
    SAFE_KORE_VERBS
        .iter()
        .map(|v| format!("command(centaury {v})"))
        .collect()
}

/// Add/remove ONLY kore's allow rules in a settings.json root. Pure so it's
/// testable; user entries and other keys survive, empty `allow`/`permissions`
/// are cleaned on removal (surgical uninstall).
fn merge_antigravity_permissions(root: &mut serde_json::Value, add: bool) {
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    let rules = antigravity_permission_rules();
    let obj = root.as_object_mut().unwrap();
    let perms = obj.entry("permissions").or_insert(serde_json::json!({}));
    if !perms.is_object() {
        *perms = serde_json::json!({});
    }
    let perms = perms.as_object_mut().unwrap();
    let allow = perms.entry("allow").or_insert(serde_json::json!([]));
    if !allow.is_array() {
        *allow = serde_json::json!([]);
    }
    let arr = allow.as_array_mut().unwrap();
    arr.retain(|e| e.as_str().is_none_or(|s| !rules.iter().any(|r| r == s)));
    if add {
        for r in &rules {
            arr.push(serde_json::json!(r));
        }
    } else {
        if arr.is_empty() {
            perms.remove("allow");
        }
        if perms.is_empty() {
            obj.remove("permissions");
        }
    }
}

fn update_antigravity_permissions(add: bool) -> anyhow::Result<()> {
    let path = antigravity_settings_path();
    if !add && !path.exists() {
        return Ok(());
    }
    let mut root = read_json(&path).unwrap_or_else(|| serde_json::json!({}));
    merge_antigravity_permissions(&mut root, add);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

// ---- codex (≥0.129 hooks.json, claude-compatible protocol) ----

/// Merge kore hooks into a codex hooks.json root. Pure so it's testable.
fn merge_codex_hooks(root: &mut serde_json::Value, exe: &str) -> anyhow::Result<()> {
    let entry = |matcher: Option<&str>| {
        let mut e = serde_json::json!({
            "hooks": [{ "type": "command", "command": format!("{exe} hook codex") }]
        });
        if let Some(m) = matcher {
            e["matcher"] = m.into();
        }
        e
    };
    let hooks = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    hooks["SessionStart"] = serde_json::json!([entry(Some("startup|resume|clear"))]);
    hooks["UserPromptSubmit"] = serde_json::json!([entry(None)]);
    // W1 mid-turn drain — codex speaks claude's protocol, dispatch_stdin_event
    // already routes PostToolUse; legacy delivered via additionalContext here.
    hooks["PostToolUse"] = serde_json::json!([entry(None)]);
    hooks["Stop"] = serde_json::json!([entry(None)]);
    Ok(())
}

fn codex_home() -> std::path::PathBuf {
    std::env::var("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".codex"))
}

fn install_codex() -> anyhow::Result<()> {
    let dir = codex_home();
    let path = dir.join("hooks.json");
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    merge_codex_hooks(&mut root, &std::env::current_exe()?.display().to_string())?;

    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    // HC6 (hcom c72bdcd concept): codex ≥0.131 refuses hooks without a trust
    // entry — in exec/headless there is no prompt, they just never fire (kore
    // agent deaf). Persist the trust the way codex's own dialog would.
    // Failure is a WARN, not an error: hooks must never break the agent, and
    // older codex (<0.131, no app-server hooks/list) has no gate to satisfy.
    match ensure_codex_hooks_trusted() {
        Ok(true) => println!(
            "kore installed: codex hooks → {} (trusted for this codex build)",
            path.display()
        ),
        Ok(false) => println!(
            "kore installed: codex hooks → {} (trust already current)",
            path.display()
        ),
        Err(e) => println!(
            "kore installed: codex hooks → {} (WARN: trust registration failed: {e} — codex will prompt once, or launch with --dangerously-bypass-hook-trust)",
            path.display()
        ),
    }
    Ok(())
}

// ---- codex hook trust (HC6, hcom b2962fa/c72bdcd concepts; shapes
// live-verified against codex 0.143.0 app-server, 2026-07-08) ----

/// One hook row from `hooks/list`: `key` names the hooks.json slot
/// (`<path>:session_start:0:0`), codex computes `currentHash` over the
/// definition — writing it to `config.toml [hooks.state."<key>"]
/// trusted_hash` is exactly what codex's own trust dialog persists.
struct CodexTrustEntry {
    key: String,
    current_hash: String,
}

/// `codex --version` → "codex-cli 0.143.0" → "0.143.0"; stamped into each
/// state entry so a codex upgrade (new hash inputs) re-triggers the fetch
/// instead of leaving silently-dead hooks.
fn codex_cli_version() -> anyhow::Result<String> {
    let out = std::process::Command::new("codex")
        .arg("--version")
        .output()?;
    if !out.status.success() {
        anyhow::bail!("codex --version failed");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.split_whitespace().last().unwrap_or("").to_string())
}

const KORE_CODEX_VERSION_KEY: &str = "kore_codex_cli_version";

/// Idempotent trust registration. Ok(false) = already current (the common
/// launch path costs one config.toml read, no app-server spawn); Ok(true) =
/// fetched + wrote. Err = codex missing/old/handshake failed (caller warns).
fn ensure_codex_hooks_trusted() -> anyhow::Result<bool> {
    let version = codex_cli_version()?;
    let config_path = codex_home().join("config.toml");
    let exe = std::env::current_exe()?.display().to_string();
    let command = format!("{exe} hook codex");

    let mut doc: toml_edit::DocumentMut = std::fs::read_to_string(&config_path)
        .unwrap_or_default()
        .parse()
        .unwrap_or_default();
    if codex_trust_is_current(&doc, &version) {
        return Ok(false);
    }

    let entries = fetch_codex_hook_entries(&command)?;
    write_codex_trust_state(&mut doc, &entries, &version);
    std::fs::create_dir_all(codex_home())?;
    std::fs::write(&config_path, doc.to_string())?;
    Ok(true)
}

/// Current = all four kore hook slots carry a non-empty trusted_hash stamped
/// with this codex version. ponytail ceiling: a manual hooks.json edit
/// between installs changes the hash without changing the version — but
/// install_codex rewrites hooks.json right before this runs, so kore's own
/// writes are always fresh; foreign edits get codex's trust prompt (same as
/// before HC6).
fn codex_trust_is_current(doc: &toml_edit::DocumentMut, version: &str) -> bool {
    let Some(state) = doc
        .get("hooks")
        .and_then(|h| h.get("state"))
        .and_then(|s| s.as_table_like())
    else {
        return false;
    };
    [
        "session_start",
        "user_prompt_submit",
        "post_tool_use",
        "stop",
    ]
    .iter()
    .all(|slot| {
        state.iter().any(|(key, item)| {
            key.contains(slot)
                && item
                    .get("trusted_hash")
                    .and_then(|v| v.as_str())
                    .is_some_and(|h| !h.is_empty())
                && item.get(KORE_CODEX_VERSION_KEY).and_then(|v| v.as_str()) == Some(version)
        })
    })
}

/// Merge trust entries into config.toml, kore's keys only — toml_edit keeps
/// every byte of the user's config intact (same rule as kimi's HC10 merge).
fn write_codex_trust_state(
    doc: &mut toml_edit::DocumentMut,
    entries: &[CodexTrustEntry],
    version: &str,
) {
    if doc.get("hooks").is_none_or(|h| !h.is_table_like()) {
        doc["hooks"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if doc["hooks"].get("state").is_none_or(|s| !s.is_table_like()) {
        doc["hooks"]["state"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let state = doc["hooks"]["state"]
        .as_table_like_mut()
        .expect("just ensured");
    for e in entries {
        if state.get(&e.key).is_none_or(|i| !i.is_table_like()) {
            state.insert(&e.key, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let item = state.get_mut(&e.key).expect("just inserted");
        item["trusted_hash"] = toml_edit::value(e.current_hash.clone());
        item["enabled"] = toml_edit::value(true);
        item[KORE_CODEX_VERSION_KEY] = toml_edit::value(version);
    }
}

/// Ask the codex binary itself for the hashes: spawn `codex app-server`,
/// JSON-RPC initialize → hooks/list, keep only rows whose command is OURS
/// (never touch trust for foreign hooks). The hash isn't kore's to compute —
/// it's version-specific to the codex build (why HC6 was gated on an install).
fn fetch_codex_hook_entries(command: &str) -> anyhow::Result<Vec<CodexTrustEntry>> {
    use std::io::{BufRead, Write};
    let mut child = std::process::Command::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("no app-server stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no app-server stdout"))?;
    let mut reader = std::io::BufReader::new(stdout);

    let read_response = |reader: &mut std::io::BufReader<std::process::ChildStdout>,
                         id: u64|
     -> anyhow::Result<serde_json::Value> {
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                anyhow::bail!("codex app-server closed before responding (id {id})");
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                return Ok(v);
            }
        }
    };

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "method": "initialize", "id": 1,
            "params": {
                "clientInfo": {"name": "centaury", "title": "kore", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }
        })
    )?;
    read_response(&mut reader, 1)?;
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"method": "initialized", "params": {}})
    )?;
    let cwd = std::env::current_dir()?;
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "method": "hooks/list", "id": 2, "params": {"cwds": [cwd]}
        })
    )?;
    stdin.flush()?;
    let resp = read_response(&mut reader, 2);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();

    let hooks = resp?;
    let hooks = hooks
        .pointer("/result/data/0/hooks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("hooks/list response carried no hooks"))?
        .iter()
        .filter(|h| h.get("command").and_then(|c| c.as_str()) == Some(command))
        .filter_map(|h| {
            Some(CodexTrustEntry {
                key: h.get("key")?.as_str()?.to_string(),
                current_hash: h.get("currentHash")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if hooks.len() < 4 {
        anyhow::bail!(
            "hooks/list returned {} kore hooks, expected 4 — hooks.json not picked up?",
            hooks.len()
        );
    }
    Ok(hooks)
}

// ---- cursor (HC8, hcom 134e0ba) ----

/// Cursor events kore wires (hooks.json keys are camelCase; the hook COMMAND
/// carries the lowercased event as argv). stop is the blocking-wait slot →
/// generous timeout + `loop_limit: null` (unlimited follow-up injections,
/// upstream shape); the rest are quick.
const CURSOR_HOOK_EVENTS: &[(&str, u64)] = &[
    ("sessionStart", 30),
    ("postToolUse", 30), // W1 mid-turn drain
    ("stop", 86400),
];

/// Auto-approved kore verbs (cursor permission rules, copilot
/// PermissionRequest) — the read-and-send surface only; kill/unregister stay
/// behind D10's --go and human hands.
const SAFE_KORE_VERBS: &[&str] = &[
    "send", "list", "history", "listen", "events", "threads", "bundle",
];

fn cursor_home() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default().join(".cursor")
}

fn is_kore_cursor_entry(e: &serde_json::Value) -> bool {
    // Command-suffix match, exe-path-independent (same rule as claude/codex).
    e.get("command")
        .and_then(|c| c.as_str())
        .is_some_and(|c| c.contains(" hook cursor"))
}

fn merge_cursor_hooks(root: &mut serde_json::Value, exe: &str) {
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    let obj = root.as_object_mut().unwrap();
    obj.entry("version").or_insert(serde_json::json!(1));
    let hooks = obj.entry("hooks").or_insert(serde_json::json!({}));
    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for (event, timeout) in CURSOR_HOOK_EVENTS {
        let entries = hooks
            .entry((*event).to_string())
            .or_insert(serde_json::json!([]));
        if !entries.is_array() {
            *entries = serde_json::json!([]);
        }
        let entries = entries.as_array_mut().unwrap();
        entries.retain(|e| !is_kore_cursor_entry(e));
        let mut entry = serde_json::json!({
            "command": format!("{exe} hook cursor {}", event.to_lowercase()),
            "timeout": timeout,
        });
        if *event == "stop" {
            entry["loop_limit"] = serde_json::Value::Null;
        }
        entries.push(entry);
    }
}

fn strip_cursor_kore(root: &mut serde_json::Value) {
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for entries in hooks.values_mut() {
            if let Some(a) = entries.as_array_mut() {
                a.retain(|e| !is_kore_cursor_entry(e));
            }
        }
        hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
    }
}

fn cursor_has_kore(root: &serde_json::Value) -> bool {
    CURSOR_HOOK_EVENTS.iter().all(|(event, _)| {
        root.get("hooks")
            .and_then(|h| h.get(*event))
            .and_then(|v| v.as_array())
            .is_some_and(|a| a.iter().any(is_kore_cursor_entry))
    })
}

fn cursor_permission_rules() -> Vec<String> {
    SAFE_KORE_VERBS
        .iter()
        .map(|v| format!("Shell(centaury {v})"))
        .collect()
}

/// Add/remove kore's allow rules in `~/.cursor/cli-config.json` — without
/// them every agent reply stalls on a shell-permission prompt (same failure
/// mode as claude without `Bash(centaury:*)`). Only kore rules touched.
fn update_cursor_permissions(add: bool) -> anyhow::Result<()> {
    let path = cursor_home().join("cli-config.json");
    if !add && !path.exists() {
        return Ok(());
    }
    let mut root = read_json(&path).unwrap_or_else(|| serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let rules = cursor_permission_rules();
    let obj = root.as_object_mut().unwrap();
    let perms = obj.entry("permissions").or_insert(serde_json::json!({}));
    if !perms.is_object() {
        *perms = serde_json::json!({});
    }
    let perms = perms.as_object_mut().unwrap();
    let allow = perms.entry("allow").or_insert(serde_json::json!([]));
    if !allow.is_array() {
        *allow = serde_json::json!([]);
    }
    let arr = allow.as_array_mut().unwrap();
    arr.retain(|e| e.as_str().is_none_or(|s| !rules.iter().any(|r| r == s)));
    if add {
        for r in &rules {
            arr.push(serde_json::json!(r));
        }
    } else if arr.is_empty() {
        perms.remove("allow");
    }
    std::fs::create_dir_all(cursor_home())?;
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn install_cursor() -> anyhow::Result<()> {
    let dir = cursor_home();
    let path = dir.join("hooks.json");
    let mut root = read_json(&path).unwrap_or_else(|| serde_json::json!({}));
    merge_cursor_hooks(&mut root, &std::env::current_exe()?.display().to_string());
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    update_cursor_permissions(true)?;
    println!("kore installed: cursor hooks → {}", path.display());
    Ok(())
}

// ---- copilot (HC9, hcom 2ad34cd) ----

/// Copilot events kore wires. Keys are PascalCase in the hooks file; the
/// command carries the lowercased event as argv. Stop blocks (wait slot).
const COPILOT_HOOK_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 30),
    ("Stop", 86400),
    ("PostToolUse", 30),       // W1 mid-turn drain
    ("PermissionRequest", 30), // auto-allow safe centaury commands
];

fn copilot_hooks_dir() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default().join(".copilot/hooks")
}

/// Copilot reads every JSON file in `~/.copilot/hooks/` — kore gets its OWN
/// file (`kore.json`), so install is a whole-file write and uninstall a
/// delete; user hooks in other files are never touched.
fn copilot_hooks_value(exe: &str) -> serde_json::Value {
    let mut hooks = serde_json::Map::new();
    for (event, timeout) in COPILOT_HOOK_EVENTS {
        hooks.insert(
            (*event).to_string(),
            serde_json::json!([{
                "type": "command",
                "command": format!("{exe} hook copilot {}", event.to_lowercase()),
                "timeoutSec": timeout,
            }]),
        );
    }
    serde_json::json!({ "version": 1, "hooks": hooks })
}

fn install_copilot() -> anyhow::Result<()> {
    let dir = copilot_hooks_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("kore.json");
    let root = copilot_hooks_value(&std::env::current_exe()?.display().to_string());
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    println!("kore installed: copilot hooks → {}", path.display());
    Ok(())
}

// ---- kimi (HC10, hcom 7cbe0d9) — TOML config ----

/// Kimi events kore wires (config.toml `[[hooks]]` entries; `event` = the
/// PascalCase name, `command` carries `kimi-<lower>` argv). Stop is the
/// blocking wait → generous timeout.
const KIMI_HOOK_EVENTS: &[(&str, u64)] =
    &[("SessionStart", 30), ("PostToolUse", 30), ("Stop", 86400)];

fn kimi_config_dir() -> std::path::PathBuf {
    match std::env::var("KIMI_CODE_HOME") {
        Ok(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => dirs::home_dir().unwrap_or_default().join(".kimi-code"),
    }
}

fn kimi_config_path() -> std::path::PathBuf {
    kimi_config_dir().join("config.toml")
}

fn kimi_command(exe: &str, event: &str) -> String {
    format!("{exe} hook kimi kimi-{}", event.to_lowercase())
}

fn is_kore_kimi_command(cmd: &str) -> bool {
    cmd.contains(" hook kimi kimi-")
}

fn kimi_permission_patterns() -> Vec<String> {
    SAFE_KORE_VERBS
        .iter()
        .map(|v| format!("Bash(centaury {v}*)"))
        .collect()
}

/// Merge kore's `[[hooks]]` + `[[permission.rules]]` into kimi's config.toml,
/// leaving every user entry intact. Idempotent (kore rows replaced by marker).
fn kimi_merge(doc: &mut toml_edit::DocumentMut, exe: &str) {
    use toml_edit::{ArrayOfTables, Item, Table, value};

    let hooks = doc
        .entry("hooks")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    if let Item::ArrayOfTables(arr) = hooks {
        let mut kept = ArrayOfTables::new();
        for i in 0..arr.len() {
            if let Some(t) = arr.get(i) {
                let ours = t
                    .get("command")
                    .and_then(|v| v.as_str())
                    .is_some_and(is_kore_kimi_command);
                if !ours {
                    kept.push(t.clone());
                }
            }
        }
        for (event, timeout) in KIMI_HOOK_EVENTS {
            let mut t = Table::new();
            t.insert("event", value(*event));
            t.insert("command", value(kimi_command(exe, event)));
            t.insert("timeout", value(*timeout as i64));
            kept.push(t);
        }
        *arr = kept;
    }

    // Permission allow-rules first (kimi matches top-to-bottom, first wins) so
    // a launched agent runs its own centaury without --yolo.
    let permission = doc
        .entry("permission")
        .or_insert_with(|| Item::Table(Table::new()));
    if let Item::Table(perm) = permission {
        let rules = perm
            .entry("rules")
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
        if let Item::ArrayOfTables(arr) = rules {
            let mut rebuilt = ArrayOfTables::new();
            for pat in kimi_permission_patterns() {
                let mut t = Table::new();
                t.insert("decision", value("allow"));
                t.insert("pattern", value(pat));
                t.insert("reason", value("kore auto-approve"));
                rebuilt.push(t);
            }
            for i in 0..arr.len() {
                if let Some(t) = arr.get(i) {
                    let ours = t
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .is_some_and(|p| kimi_permission_patterns().iter().any(|k| k == p));
                    if !ours {
                        rebuilt.push(t.clone());
                    }
                }
            }
            *arr = rebuilt;
        }
    }
}

fn kimi_strip(doc: &mut toml_edit::DocumentMut) {
    use toml_edit::{ArrayOfTables, Item};
    if let Some(Item::ArrayOfTables(arr)) = doc.get_mut("hooks") {
        let mut kept = ArrayOfTables::new();
        for i in 0..arr.len() {
            if let Some(t) = arr.get(i)
                && !t
                    .get("command")
                    .and_then(|v| v.as_str())
                    .is_some_and(is_kore_kimi_command)
            {
                kept.push(t.clone());
            }
        }
        *arr = kept;
        if arr.is_empty() {
            doc.remove("hooks");
        }
    }
    if let Some(Item::Table(perm)) = doc.get_mut("permission")
        && let Some(Item::ArrayOfTables(arr)) = perm.get_mut("rules")
    {
        let mut kept = ArrayOfTables::new();
        for i in 0..arr.len() {
            if let Some(t) = arr.get(i) {
                let ours = t
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .is_some_and(|p| kimi_permission_patterns().iter().any(|k| k == p));
                if !ours {
                    kept.push(t.clone());
                }
            }
        }
        *arr = kept;
    }
}

fn kimi_has_kore(doc: &toml_edit::DocumentMut) -> bool {
    matches!(doc.get("hooks"), Some(toml_edit::Item::ArrayOfTables(arr))
        if (0..arr.len()).filter_map(|i| arr.get(i))
            .any(|t| t.get("command").and_then(|v| v.as_str()).is_some_and(is_kore_kimi_command)))
}

fn read_kimi_doc(path: &std::path::Path) -> toml_edit::DocumentMut {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

fn install_kimi() -> anyhow::Result<()> {
    let path = kimi_config_path();
    let mut doc = read_kimi_doc(&path);
    kimi_merge(&mut doc, &std::env::current_exe()?.display().to_string());
    std::fs::create_dir_all(kimi_config_dir())?;
    std::fs::write(&path, doc.to_string())?;
    println!("kore installed: kimi hooks → {}", path.display());
    Ok(())
}

// ---- hermes (HC17) — Nous Hermes Agent, shell-script hooks in config.yaml ----

/// Hermes events kore wires (config.yaml `hooks:` block, event → [{command,
/// timeout}]). One command serves all — the event name arrives in stdin.
/// Timeouts are clamped by hermes to [1, 300]; pre_verify gets the max (it
/// is the opportunistic blocking-wait slot).
const HERMES_HOOK_EVENTS: &[(&str, u64)] = &[
    ("on_session_start", 30),
    ("pre_llm_call", 30),
    ("pre_verify", 300),
];

const HERMES_MARK_BEGIN: &str =
    "# >>> kore hermes hooks (managed by `centaury hook install --tool hermes`)";
const HERMES_MARK_END: &str = "# <<< kore hermes hooks";
/// Match installs on the stable prefix so a wording tweak in BEGIN never
/// orphans a previously-installed region.
const HERMES_MARK_BEGIN_PREFIX: &str = "# >>> kore hermes hooks";

fn hermes_home() -> std::path::PathBuf {
    match std::env::var("HERMES_HOME") {
        Ok(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => dirs::home_dir().unwrap_or_default().join(".hermes"),
    }
}

fn hermes_config_path() -> std::path::PathBuf {
    hermes_home().join("config.yaml")
}

fn hermes_hooks_block(exe: &str) -> String {
    // YAML double-quoted scalar (exe paths may hold spaces/backslashes).
    let cmd = format!("{exe} hook hermes")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut s = format!("{HERMES_MARK_BEGIN}\nhooks:\n");
    for (event, timeout) in HERMES_HOOK_EVENTS {
        s.push_str(&format!(
            "  {event}:\n  - command: \"{cmd}\"\n    timeout: {timeout}\n"
        ));
    }
    s.push_str(HERMES_MARK_END);
    s.push('\n');
    s
}

/// Byte range of the kore marker region: start of BEGIN's line → end of
/// END's line, trailing newline included.
fn hermes_marker_range(text: &str) -> Option<std::ops::Range<usize>> {
    let b = text.find(HERMES_MARK_BEGIN_PREFIX)?;
    let start = text[..b].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let e = b + text[b..].find(HERMES_MARK_END)?;
    let end = text[e..]
        .find('\n')
        .map(|i| e + i + 1)
        .unwrap_or(text.len());
    Some(start..end)
}

/// Pure text merge. config.yaml is user-owned and comment-heavy, and Rust
/// has no comment-preserving YAML editor (kimi got toml_edit; YAML has no
/// equivalent) — so kore owns exactly ONE marker-fenced region and refuses
/// to guess at anything else:
/// - kore region present → replace it (idempotent reinstall, moved exe)
/// - `hooks: {}` (the shipped default) → swap that line for the region
/// - no top-level `hooks:` key → append the region at EOF
/// - a real user `hooks:` block → Err carrying the exact YAML to paste;
///   string surgery inside someone else's YAML corrupts configs.
fn hermes_merge_config(text: &str, exe: &str) -> anyhow::Result<String> {
    let block = hermes_hooks_block(exe);
    if let Some(r) = hermes_marker_range(text) {
        return Ok(format!("{}{}{}", &text[..r.start], block, &text[r.end..]));
    }

    let mut out = String::with_capacity(text.len() + block.len() + 2);
    let mut swapped = false;
    for line in text.split_inclusive('\n') {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        if !swapped && bare.starts_with("hooks:") {
            let rest = bare["hooks:".len()..].trim();
            let empty = rest == "{}"
                || (rest.starts_with("{}") && rest["{}".len()..].trim_start().starts_with('#'));
            if !empty {
                anyhow::bail!(
                    "{} already has a custom top-level `hooks:` block — kore won't rewrite \
                     YAML it doesn't own. Add these entries to it yourself:\n\n{}",
                    hermes_config_path().display(),
                    block
                );
            }
            out.push_str(&block); // swap the empty default for the kore region
            swapped = true;
            continue;
        }
        out.push_str(line);
    }
    if !swapped {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&block);
    }
    Ok(out)
}

fn hermes_strip_config(text: &str) -> String {
    match hermes_marker_range(text) {
        Some(r) => format!("{}{}", &text[..r.start], &text[r.end..]),
        None => text.to_string(),
    }
}

/// Pre-seed hermes's shell-hook consent allowlist for kore's own hooks —
/// running `hook install --tool hermes` IS the user consenting, and a
/// non-TTY hermes run (gateway/cron) can't answer the prompt at all.
/// Consent matches on (event, command) (shell_hooks._is_allowlisted); the
/// mtime field is advisory (`hermes hooks doctor`). Only kore rows touched.
fn update_hermes_allowlist(add: bool) -> anyhow::Result<()> {
    let path = hermes_home().join("shell-hooks-allowlist.json");
    if !add && !path.exists() {
        return Ok(());
    }
    let mut root = read_json(&path).unwrap_or_else(|| serde_json::json!({ "approvals": [] }));
    if !root.is_object() {
        root = serde_json::json!({ "approvals": [] });
    }
    let exe = std::env::current_exe()?;
    let command = format!("{} hook hermes", exe.display());
    let obj = root.as_object_mut().unwrap();
    let approvals = obj.entry("approvals").or_insert(serde_json::json!([]));
    if !approvals.is_array() {
        *approvals = serde_json::json!([]);
    }
    let arr = approvals.as_array_mut().unwrap();
    arr.retain(|e| {
        !e["command"]
            .as_str()
            .is_some_and(|c| c.contains(" hook hermes"))
    });
    if add {
        let now = iso8601_utc(std::time::SystemTime::now());
        let mtime = std::fs::metadata(&exe)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(iso8601_utc);
        for (event, _) in HERMES_HOOK_EVENTS {
            arr.push(serde_json::json!({
                "event": event,
                "command": command,
                "approved_at": now,
                "script_mtime_at_approval": mtime,
            }));
        }
    }
    std::fs::create_dir_all(hermes_home())?;
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

/// `2026-07-07T12:00:00Z` without a chrono dep (Hinnant civil-from-days).
/// Hermes matches consent on (event, command); these fields are surfaced by
/// `hooks list`/`doctor`, which may parse them — so they must be real ISO.
fn iso8601_utc(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60
    )
}

fn install_hermes() -> anyhow::Result<()> {
    let path = hermes_config_path();
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = hermes_merge_config(&before, &std::env::current_exe()?.display().to_string())?;
    std::fs::create_dir_all(hermes_home())?;
    std::fs::write(&path, merged)?;
    update_hermes_allowlist(true)?;
    println!(
        "kore installed: hermes hooks → {} (consent pre-seeded; verify with `hermes hooks list`)",
        path.display()
    );
    Ok(())
}

// ---- opencode / kilo (TS plugin) + cline (hook scripts) — B7 ----

/// One plugin source serves both: kilo is an opencode fork with the same
/// plugin API under its own package name, so install rewrites the import and
/// the TOOL constant (legacy kept two near-identical .ts files instead).
const OPENCODE_PLUGIN: &str = include_str!("plugins/centaury-opencode.ts");

/// Project-local plugin install (mirrors `.claude/settings.json` scoping):
/// opencode auto-loads `.opencode/plugin/*.ts`, kilo `.kilocode/plugins/*.ts`.
fn install_ts_plugin(tool: &str, dir: &str) -> anyhow::Result<()> {
    let source = match tool {
        "kilo" => OPENCODE_PLUGIN
            .replace("@opencode-ai/plugin", "@kilocode/plugin")
            .replace("const TOOL = \"opencode\"", "const TOOL = \"kilo\""),
        _ => OPENCODE_PLUGIN.to_string(),
    };
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir)?;
    let path = dir.join("kore.ts");
    std::fs::write(&path, source)?;
    println!("kore installed: {tool} plugin → {}", path.display());
    Ok(())
}

// ---- pi / omp (HC11/HC12, hcom ceb8c81 + 2613033) — extension plugins ----

/// Pi Coding Agent (and oh-my-pi) load extensions from a home-scoped dir.
/// One source, omp rewrites the import (like kilo↔opencode). Pi auto-loads
/// `~/.pi/agent/extensions/*.ts`; omp `~/.omp/agent/extensions/*.ts`
/// (PI_CODING_AGENT_DIR overrides the parent for both).
const PI_PLUGIN: &str = include_str!("plugins/centaury-pi.ts");

fn pi_extensions_dir(tool: &str) -> std::path::PathBuf {
    if let Ok(d) = std::env::var("PI_CODING_AGENT_DIR")
        && !d.is_empty()
    {
        return std::path::PathBuf::from(d).join("extensions");
    }
    let home = dirs::home_dir().unwrap_or_default();
    let base = if tool == "omp" { ".omp" } else { ".pi" };
    home.join(base).join("agent").join("extensions")
}

fn pi_plugin_source(tool: &str) -> String {
    if tool == "omp" {
        PI_PLUGIN
            .replace(
                "@earendil-works/pi-coding-agent",
                "@oh-my-pi/pi-coding-agent",
            )
            .replace("const TOOL = \"pi\"", "const TOOL = \"omp\"")
    } else {
        PI_PLUGIN.to_string()
    }
}

fn install_pi(tool: &str) -> anyhow::Result<()> {
    let dir = pi_extensions_dir(tool);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("kore.ts");
    std::fs::write(&path, pi_plugin_source(tool))?;
    println!("kore installed: {tool} extension → {}", path.display());
    Ok(())
}

/// Cline hooks are standalone executables named after the event, dropped in
/// `.clinerules/hooks/` (project scope). stdin JSON in, JSON out with
/// `contextModification` — the scripts wrap our plain-text dialect.
/// No wait/stop hook: cline has no async injection, queued messages arrive
/// with the next prompt instead.
const CLINE_HOOKS: &[(&str, &str)] = &[
    ("TaskStart", "session-start"),
    ("UserPromptSubmit", "drain"),
];

fn cline_hook_script(event: &str) -> String {
    // POSIX sh; awk JSON-escapes the payload (pattern proven in legacy's
    // clinehook.sh). Any failure degrades to {"cancel":false} — hooks must
    // never break the host tool.
    format!(
        r#"#!/bin/sh
# kore {event} hook (generated by `centaury hook install --tool cline`)
set -eu
cat >/dev/null 2>&1 || true
B=$(centaury hook cline {event} 2>/dev/null) || B=""
[ -z "$B" ] && {{ printf '{{"cancel":false}}\n'; exit 0; }}
E=$(printf '%s' "$B" | awk '{{gsub(/\\/,"\\\\");gsub(/"/,"\\\"");if(NR>1)printf "\\n";printf "%s",$0}}')
printf '{{"cancel":false,"contextModification":"%s"}}\n' "$E"
"#
    )
}

fn install_cline() -> anyhow::Result<()> {
    let dir = std::path::Path::new(".clinerules/hooks");
    std::fs::create_dir_all(dir)?;
    for (hook_name, event) in CLINE_HOOKS {
        let path = dir.join(hook_name);
        std::fs::write(&path, cline_hook_script(event))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    println!("kore installed: cline hooks → .clinerules/hooks/ (TaskStart, UserPromptSubmit)");
    Ok(())
}

// ---- openclaw (Steinberger's gateway daemon) — standalone SDK plugin ----
//
// Unlike opencode/pi (drop a file in an auto-loaded dir), OpenClaw loads
// standalone plugins by explicit path: the .ts goes in a kore-owned dir and
// two keys go into ~/.openclaw/openclaw.json — `plugins.load.paths` (the file)
// and `plugins.entries.kore.enabled` (turn it on). Both merges are surgical and
// share the pure enable/disable fns below with the round-trip test.
const OPENCLAW_PLUGIN: &str = include_str!("plugins/centaury-openclaw.ts");

fn openclaw_home() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default().join(".openclaw")
}

/// kore-owned path (NOT ~/.openclaw/extensions — that root skips top-level
/// script files; a load.paths entry loads a standalone .ts by design).
fn openclaw_plugin_path() -> std::path::PathBuf {
    openclaw_home().join("kore").join("kore.ts")
}

fn openclaw_config_path() -> std::path::PathBuf {
    openclaw_home().join("openclaw.json")
}

/// Merge kore's load-path + enable flag into an OpenClaw config value. Pure
/// (no I/O) so install and the round-trip test exercise the same code. Bails
/// rather than overwrite if any node kore needs is present but the wrong type.
fn openclaw_enable(cfg: &mut serde_json::Value, plugin_path: &str) -> anyhow::Result<()> {
    let root = cfg
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("openclaw config root is not a JSON object"))?;
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("plugins is not an object"))?;
    let paths = plugins
        .entry("load")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("plugins.load is not an object"))?
        .entry("paths")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("plugins.load.paths is not an array"))?;
    if !paths.iter().any(|p| p.as_str() == Some(plugin_path)) {
        paths.push(serde_json::json!(plugin_path));
    }
    let kore = plugins
        .entry("entries")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("plugins.entries is not an object"))?
        .entry("kore")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("plugins.entries.kore is not an object"))?;
    kore.insert("enabled".into(), serde_json::json!(true));
    Ok(())
}

/// Reverse of openclaw_enable: drop the load-path and the kore entry, leave
/// everything else the user has. Silent on shapes it didn't write.
fn openclaw_disable(cfg: &mut serde_json::Value, plugin_path: &str) {
    let Some(plugins) = cfg.get_mut("plugins").and_then(|p| p.as_object_mut()) else {
        return;
    };
    if let Some(paths) = plugins
        .get_mut("load")
        .and_then(|l| l.get_mut("paths"))
        .and_then(|p| p.as_array_mut())
    {
        paths.retain(|p| p.as_str() != Some(plugin_path));
    }
    if let Some(entries) = plugins.get_mut("entries").and_then(|e| e.as_object_mut()) {
        entries.remove("kore");
    }
}

fn install_openclaw() -> anyhow::Result<()> {
    let plugin = openclaw_plugin_path();
    std::fs::create_dir_all(plugin.parent().unwrap())?;
    std::fs::write(&plugin, OPENCLAW_PLUGIN)?;
    let plugin_str = plugin.to_string_lossy().to_string();

    let cfg_path = openclaw_config_path();
    // Non-destructive guard: OpenClaw also accepts JSONC (comments). A config
    // that exists but won't parse as plain JSON is left untouched — clobbering
    // a user's daemon config is never worth an automated edit.
    let mut cfg = if cfg_path.exists() {
        read_json(&cfg_path).ok_or_else(|| {
            anyhow::anyhow!(
                "{} exists but isn't plain JSON (JSONC/comments?). Add manually:\n  \
                 plugins.load.paths += \"{}\"\n  plugins.entries.kore.enabled = true",
                cfg_path.display(),
                plugin_str
            )
        })?
    } else {
        std::fs::create_dir_all(cfg_path.parent().unwrap())?;
        serde_json::json!({})
    };
    openclaw_enable(&mut cfg, &plugin_str)?;
    std::fs::write(&cfg_path, serde_json::to_string_pretty(&cfg)?)?;
    println!("kore installed: openclaw plugin → {}", plugin.display());
    println!("  enabled in {}", cfg_path.display());
    println!("  note: if your config uses a plugins.allow allowlist, add \"kore\" to it.");
    Ok(())
}

// ---- D8: hook status + uninstall ----
//
// Status and uninstall read the SAME shapes install writes (shared consts +
// path helpers above) — legacy invariant: the verbs must never disagree.
// Uninstall is surgical: only kore entries leave shared config files;
// everything the user put there survives.

const SUPPORTED_TOOLS: &[&str] = &[
    "claude",
    "gemini",
    "antigravity",
    "codex",
    "centaury",
    "cursor",
    "copilot",
    "kimi",
    "hermes",
    "pi",
    "omp",
    "opencode",
    "kilo",
    "cline",
    "openclaw",
];

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Does a claude/codex-shape hook entry belong to kore? Matched on the
/// command suffix, not the exe path — the binary may have moved since install.
fn entry_is_kore(entry: &serde_json::Value, marker: &str) -> bool {
    entry["hooks"].as_array().is_some_and(|hooks| {
        hooks
            .iter()
            .any(|h| h["command"].as_str().is_some_and(|c| c.contains(marker)))
    })
}

/// Claude/codex share the entry shape; only the file layout differs.
fn settings_has_kore(root: &serde_json::Value, events: &[&str], marker: &str) -> bool {
    events.iter().any(|ev| {
        root["hooks"][ev]
            .as_array()
            .is_some_and(|arr| arr.iter().any(|e| entry_is_kore(e, marker)))
    })
}

fn strip_kore_entries(root: &mut serde_json::Value, events: &[&str], marker: &str) {
    let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return;
    };
    for ev in events {
        if let Some(arr) = hooks.get_mut(*ev).and_then(|a| a.as_array_mut()) {
            arr.retain(|e| !entry_is_kore(e, marker));
        }
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
}

fn strip_claude_kore(settings: &mut serde_json::Value) {
    let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
    strip_kore_entries(settings, &events, "hook claude");
    if let Some(allow) = settings
        .get_mut("permissions")
        .and_then(|p| p.get_mut("allow"))
        .and_then(|a| a.as_array_mut())
    {
        allow.retain(|r| r.as_str() != Some(CLAUDE_ALLOW_RULE));
    }
}

fn gemini_has_kore(settings: &serde_json::Value) -> bool {
    GEMINI_HOOKS.iter().any(|&(event, ..)| {
        settings["hooks"][event].as_array().is_some_and(|arr| {
            arr.iter().any(|e| {
                e["hooks"][0]["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("kore-"))
            })
        })
    })
}

/// Inverse of `merge_gemini_hooks`' own filter. The enableHooks/hooksConfig
/// switches stay on — the user's other hooks may depend on them.
fn strip_gemini_kore(settings: &mut serde_json::Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return;
    };
    for &(event, ..) in GEMINI_HOOKS {
        if let Some(arr) = hooks.get_mut(event).and_then(|a| a.as_array_mut()) {
            arr.retain(|e| {
                e["hooks"][0]["name"]
                    .as_str()
                    .is_none_or(|n| !n.starts_with("kore-"))
            });
        }
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
}

const CODEX_HOOK_EVENTS: &[&str] = &["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"];

/// Per-tool: (config path, kore installed there?).
fn tool_status(tool: &str, user_scope: bool) -> anyhow::Result<(std::path::PathBuf, bool)> {
    Ok(match tool {
        "claude" => {
            let path = claude_base(user_scope)?.join("settings.json");
            let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
            let installed =
                read_json(&path).is_some_and(|v| settings_has_kore(&v, &events, "hook claude"));
            (path, installed)
        }
        "gemini" => {
            let path = gemini_config_dir().join("settings.json");
            let installed = read_json(&path).is_some_and(|v| gemini_has_kore(&v));
            (path, installed)
        }
        "antigravity" => {
            let path = gemini_config_dir().join("config").join("hooks.json");
            let installed = read_json(&path).is_some_and(|v| v.get("kore-lifecycle").is_some());
            (path, installed)
        }
        "codex" => {
            let path = codex_home().join("hooks.json");
            let installed = read_json(&path)
                .is_some_and(|v| settings_has_kore(&v, CODEX_HOOK_EVENTS, "hook codex"));
            (path, installed)
        }
        "centaury" => {
            let path = centaury_hooks_path();
            let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
            let installed =
                read_json(&path).is_some_and(|v| settings_has_kore(&v, &events, "hook centaury"));
            (path, installed)
        }
        "cursor" | "cursor-agent" => {
            let path = cursor_home().join("hooks.json");
            let installed = read_json(&path).is_some_and(|v| cursor_has_kore(&v));
            (path, installed)
        }
        "copilot" => {
            // kore-owned file: existing + parseable = installed.
            let path = copilot_hooks_dir().join("kore.json");
            let installed = read_json(&path).is_some_and(|v| v.get("hooks").is_some());
            (path, installed)
        }
        "kimi" => {
            let path = kimi_config_path();
            let installed = path.exists() && kimi_has_kore(&read_kimi_doc(&path));
            (path, installed)
        }
        "hermes" => {
            let path = hermes_config_path();
            let installed =
                std::fs::read_to_string(&path).is_ok_and(|s| hermes_marker_range(&s).is_some());
            (path, installed)
        }
        "pi" | "omp" => {
            let path = pi_extensions_dir(tool).join("kore.ts");
            (path.clone(), path.exists())
        }
        "opencode" => {
            let path = std::path::PathBuf::from(".opencode/plugin/kore.ts");
            let installed = path.exists();
            (path, installed)
        }
        "kilo" | "kilocode" => {
            let path = std::path::PathBuf::from(".kilocode/plugins/kore.ts");
            let installed = path.exists();
            (path, installed)
        }
        "cline" => {
            let path = std::path::PathBuf::from(".clinerules/hooks");
            // Ours only if the generated marker is present — the filenames
            // are generic and the user may have their own hooks there.
            let installed = CLINE_HOOKS.iter().any(|(hook_name, _)| {
                std::fs::read_to_string(path.join(hook_name))
                    .is_ok_and(|s| s.contains("centaury hook cline"))
            });
            (path, installed)
        }
        "openclaw" => {
            // kore-owned file: existing = installed (config keys ride with it).
            let path = openclaw_plugin_path();
            (path.clone(), path.exists())
        }
        other => anyhow::bail!("unknown tool '{other}' ({})", SUPPORTED_TOOLS.join("|")),
    })
}

pub fn status(user_scope: bool) -> anyhow::Result<()> {
    for tool in SUPPORTED_TOOLS {
        let (path, installed) = tool_status(tool, user_scope)?;
        let state = if installed { "installed" } else { "-" };
        println!("{tool:<12} {state:<10} {}", path.display());
    }
    Ok(())
}

/// Rewrite a JSON config with `strip` applied; missing/unparseable file = done.
fn strip_json_file(
    path: &std::path::Path,
    strip: impl FnOnce(&mut serde_json::Value),
) -> anyhow::Result<()> {
    let Some(mut v) = read_json(path) else {
        return Ok(());
    };
    strip(&mut v);
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

pub fn uninstall(tool: &str, user_scope: bool) -> anyhow::Result<()> {
    if tool == "all" {
        for t in SUPPORTED_TOOLS {
            uninstall(t, user_scope)?;
        }
        return Ok(());
    }
    let (path, installed) = tool_status(tool, user_scope)?;
    if !installed {
        println!("{tool:<12} nothing to remove");
        return Ok(());
    }
    match tool {
        "claude" => {
            strip_json_file(&path, strip_claude_kore)?;
            let _ = std::fs::remove_dir_all(claude_base(user_scope)?.join("skills/kore"));
        }
        "gemini" => strip_json_file(&path, strip_gemini_kore)?,
        "antigravity" => {
            strip_json_file(&path, |v| {
                if let Some(o) = v.as_object_mut() {
                    o.remove("kore-lifecycle");
                }
            })?;
            update_antigravity_permissions(false)?;
        }
        "codex" => strip_json_file(&path, |v| {
            strip_kore_entries(v, CODEX_HOOK_EVENTS, "hook codex")
        })?,
        "centaury" => strip_json_file(&path, |v| {
            let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
            strip_kore_entries(v, &events, "hook centaury")
        })?,
        "cursor" | "cursor-agent" => {
            strip_json_file(&path, strip_cursor_kore)?;
            update_cursor_permissions(false)?;
        }
        "copilot" => {
            std::fs::remove_file(&path)?; // kore-owned file
        }
        "kimi" => {
            let mut doc = read_kimi_doc(&path);
            kimi_strip(&mut doc);
            std::fs::write(&path, doc.to_string())?;
        }
        "hermes" => {
            let before = std::fs::read_to_string(&path)?;
            std::fs::write(&path, hermes_strip_config(&before))?;
            update_hermes_allowlist(false)?;
        }
        "pi" | "omp" | "opencode" | "kilo" | "kilocode" => {
            std::fs::remove_file(&path)?;
        }
        "openclaw" => {
            let plugin_str = path.to_string_lossy().to_string(); // path = plugin file
            std::fs::remove_file(&path)?;
            strip_json_file(&openclaw_config_path(), |v| {
                openclaw_disable(v, &plugin_str)
            })?;
        }
        "cline" => {
            for (hook_name, _) in CLINE_HOOKS {
                let p = path.join(hook_name);
                // Only delete scripts we generated (marker-checked).
                if std::fs::read_to_string(&p).is_ok_and(|s| s.contains("centaury hook cline")) {
                    std::fs::remove_file(&p)?;
                }
            }
        }
        _ => unreachable!("tool_status validated the name"),
    }
    println!("{tool:<12} kore removed ({})", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Consent gate: with no marker and no tty (cargo test), install must
    /// refuse; recording consent (what `hook install` does) unlocks it.
    #[test]
    fn install_refuses_without_consent() {
        let dir = std::env::temp_dir().join(format!("kore-consent-test-{}", std::process::id()));
        let marker = dir.join("hooks-consent");
        let err = ensure_consent_at(&marker, "claude")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("hook install"),
            "points at the consent command: {err}"
        );
        record_consent_at(&marker).unwrap();
        ensure_consent_at(&marker, "claude").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// HC6: trust write must be surgical (user config + foreign hook entries
    /// survive byte-for-byte) and the is-current check must key on version —
    /// a codex upgrade re-triggers the fetch, a foreign entry never satisfies
    /// kore's slots.
    #[test]
    fn codex_trust_state_roundtrip() {
        let mut doc: toml_edit::DocumentMut =
            "model = \"o4\"\n[hooks.state.\"/x/hooks.json:pre_tool_use:0:0\"]\ntrusted_hash = \"sha256:foreign\"\n"
                .parse()
                .unwrap();
        assert!(
            !codex_trust_is_current(&doc, "0.143.0"),
            "foreign entry must not satisfy kore's slots"
        );

        let entries: Vec<CodexTrustEntry> = [
            "session_start",
            "user_prompt_submit",
            "post_tool_use",
            "stop",
        ]
        .iter()
        .map(|slot| CodexTrustEntry {
            key: format!("/x/hooks.json:{slot}:0:0"),
            current_hash: format!("sha256:{slot}"),
        })
        .collect();
        write_codex_trust_state(&mut doc, &entries, "0.143.0");

        assert!(codex_trust_is_current(&doc, "0.143.0"));
        assert!(
            !codex_trust_is_current(&doc, "0.144.0"),
            "codex upgrade must invalidate"
        );
        let out = doc.to_string();
        assert!(out.contains("model = \"o4\""), "user config survives");
        assert!(
            out.contains("sha256:foreign"),
            "foreign hook entry survives"
        );
        assert!(out.contains("[hooks.state.\"/x/hooks.json:stop:0:0\"]"));
    }

    #[test]
    fn openclaw_enable_disable_is_surgical() {
        // The user's own load-path + their own entry must survive the round-trip.
        let mut cfg = serde_json::json!({
            "plugins": {
                "load": { "paths": ["/home/u/my-plugin.ts"] },
                "entries": { "mine": { "enabled": true } }
            },
            "other": 1
        });
        let before = cfg.clone();
        let kore = "/home/u/.openclaw/kore/kore.ts";

        openclaw_enable(&mut cfg, kore).unwrap();
        let paths = cfg["plugins"]["load"]["paths"].as_array().unwrap();
        assert!(paths.iter().any(|p| p.as_str() == Some(kore)));
        assert!(
            paths
                .iter()
                .any(|p| p.as_str() == Some("/home/u/my-plugin.ts"))
        );
        assert_eq!(
            cfg["plugins"]["entries"]["kore"]["enabled"],
            serde_json::json!(true)
        );

        // Idempotent: re-enable must not duplicate the load-path.
        openclaw_enable(&mut cfg, kore).unwrap();
        assert_eq!(cfg["plugins"]["load"]["paths"].as_array().unwrap().len(), 2);

        openclaw_disable(&mut cfg, kore);
        assert_eq!(cfg, before);
    }

    #[test]
    fn openclaw_enable_from_empty_config() {
        let mut cfg = serde_json::json!({});
        openclaw_enable(&mut cfg, "/p/kore.ts").unwrap();
        assert_eq!(
            cfg["plugins"]["entries"]["kore"]["enabled"],
            serde_json::json!(true)
        );
        assert_eq!(
            cfg["plugins"]["load"]["paths"][0],
            serde_json::json!("/p/kore.ts")
        );
    }

    #[test]
    fn uninstall_reverses_install_claude() {
        // User's own hook + permission must survive the round-trip.
        let mut s = serde_json::json!({
            "hooks": { "PreCompact": [{ "hooks": [{ "type": "command", "command": "mine" }] }] },
            "permissions": { "allow": ["Bash(ls:*)"] }
        });
        merge_claude_hooks(&mut s, "/bin/centaury").unwrap();
        let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
        assert!(settings_has_kore(&s, &events, "hook claude"));

        strip_claude_kore(&mut s);
        assert!(!settings_has_kore(&s, &events, "hook claude"));
        assert_eq!(s["hooks"]["PreCompact"][0]["hooks"][0]["command"], "mine");
        assert_eq!(s["permissions"]["allow"], serde_json::json!(["Bash(ls:*)"]));
    }

    #[test]
    fn uninstall_reverses_install_centaury() {
        // Foreign hook entries in OTHER events must survive the round-trip.
        let mut s = serde_json::json!({
            "hooks": { "PreCompact": [{ "hooks": [{ "type": "command", "command": "mine" }] }] }
        });
        merge_centaury_hooks(&mut s, "/bin/centaury").unwrap();
        let events: Vec<&str> = CLAUDE_HOOK_EVENTS.iter().map(|&(e, _)| e).collect();
        assert!(settings_has_kore(&s, &events, "hook centaury"));
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["timeout"], 86400);

        strip_kore_entries(&mut s, &events, "hook centaury");
        assert!(!settings_has_kore(&s, &events, "hook centaury"));
        assert_eq!(s["hooks"]["PreCompact"][0]["hooks"][0]["command"], "mine");
    }

    #[test]
    fn uninstall_reverses_install_gemini() {
        let mut s = serde_json::json!({});
        merge_gemini_hooks(&mut s, "/bin/centaury");
        assert!(gemini_has_kore(&s));

        strip_gemini_kore(&mut s);
        assert!(!gemini_has_kore(&s));
        // Switches stay on — the user's other hooks may depend on them.
        assert_eq!(s["tools"]["enableHooks"], true);
    }

    #[test]
    fn uninstall_reverses_install_cursor() {
        // User's own hook entries must survive the round-trip.
        let mut s = serde_json::json!({
            "version": 1,
            "hooks": { "stop": [{ "command": "./custom-stop.sh", "timeout": 5 }] }
        });
        merge_cursor_hooks(&mut s, "/bin/centaury");
        assert!(cursor_has_kore(&s));
        let stops = s["hooks"]["stop"].as_array().unwrap();
        let kore_stop = stops.iter().find(|e| is_kore_cursor_entry(e)).unwrap();
        assert!(
            kore_stop["loop_limit"].is_null(),
            "stop needs loop_limit: null"
        );

        // Re-install is idempotent (old entry replaced, not duplicated).
        merge_cursor_hooks(&mut s, "/moved/centaury");
        let kore_stops = s["hooks"]["stop"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| is_kore_cursor_entry(e))
            .count();
        assert_eq!(kore_stops, 1);

        strip_cursor_kore(&mut s);
        assert!(!cursor_has_kore(&s));
        assert_eq!(s["hooks"]["stop"][0]["command"], "./custom-stop.sh");
    }

    #[test]
    fn copilot_hooks_file_and_safe_commands() {
        let v = copilot_hooks_value("/bin/centaury");
        for (event, _) in COPILOT_HOOK_EVENTS {
            let entry = &v["hooks"][*event][0];
            assert_eq!(entry["type"], "command");
            assert!(entry["timeoutSec"].is_u64());
            assert!(
                entry["command"]
                    .as_str()
                    .unwrap()
                    .contains(" hook copilot ")
            );
        }
        // Stop gets the generous blocking-wait budget.
        assert_eq!(v["hooks"]["Stop"][0]["timeoutSec"], 86400);

        // PermissionRequest gate: safe verbs pass, destructive verbs don't.
        assert!(kore_command_is_safe("centaury send @luna -- \"hi\""));
        assert!(kore_command_is_safe("centaury list"));
        assert!(!kore_command_is_safe("centaury kill luna --go"));
        assert!(!kore_command_is_safe("rm -rf /"));
        assert!(!kore_command_is_safe("centaury-evil send"));
    }

    #[test]
    fn uninstall_reverses_install_kimi() {
        // A pre-existing user hook + permission rule must survive untouched.
        let src = "\
[[hooks]]\nevent = \"Stop\"\ncommand = \"./my-stop.sh\"\n\n\
[[permission.rules]]\ndecision = \"deny\"\npattern = \"Bash(rm*)\"\n";
        let mut doc: toml_edit::DocumentMut = src.parse().unwrap();
        kimi_merge(&mut doc, "/bin/centaury");
        assert!(kimi_has_kore(&doc));
        // kore's Stop timeout is the blocking-wait budget.
        let s = doc.to_string();
        assert!(s.contains("hook kimi kimi-stop"));
        assert!(s.contains("86400"));
        // kore allow-rules come BEFORE the user deny (first-match-wins).
        let kore_pos = s.find("kore auto-approve").unwrap();
        let deny_pos = s.find("Bash(rm*)").unwrap();
        assert!(kore_pos < deny_pos, "kore allows must precede user deny");

        // Idempotent: re-merge doesn't duplicate.
        kimi_merge(&mut doc, "/moved/centaury");
        let n = doc.to_string().matches("hook kimi kimi-stop").count();
        assert_eq!(n, 1);

        kimi_strip(&mut doc);
        assert!(!kimi_has_kore(&doc));
        let s = doc.to_string();
        assert!(s.contains("./my-stop.sh"), "user hook survives");
        assert!(s.contains("Bash(rm*)"), "user rule survives");
        assert!(!s.contains("kore auto-approve"), "kore rules gone");
    }

    #[test]
    fn omp_source_rewrites_import_and_tool() {
        // pi is the template; omp is the same file with two substitutions
        // (like kilo↔opencode). If either upstream string drifts, the rewrite
        // silently no-ops and omp ships pi's identity — this catches that.
        let pi = pi_plugin_source("pi");
        assert!(pi.contains("@earendil-works/pi-coding-agent"));
        assert!(pi.contains("const TOOL = \"pi\""));

        let omp = pi_plugin_source("omp");
        assert!(omp.contains("@oh-my-pi/pi-coding-agent"));
        assert!(omp.contains("const TOOL = \"omp\""));
        assert!(
            !omp.contains("@earendil-works/pi-coding-agent"),
            "import must be rewritten"
        );
        assert!(
            !omp.contains("const TOOL = \"pi\""),
            "TOOL must be rewritten"
        );
    }

    #[test]
    fn human_and_system_senders_are_tagged() {
        let mk = |from: &str, kind: &str| Delivery {
            id: 1,
            message: kore_protocol::Message {
                from: from.into(),
                sender_kind: kind.into(),
                scope: kore_protocol::MessageScope::Broadcast,
                text: "hi".into(),
                mentions: vec![],
                delivered_to: vec![],
                intent: None,
                thread: None,
                reply_to: None,
                bundle_id: None,
            },
        };
        let s = format_messages(&[
            mk("luna", "agent"),
            mk("solar", "human"),
            mk("kore", "system"),
        ]);
        assert!(s.contains("luna: hi"), "agents stay untagged");
        assert!(!s.contains("[agent]"));
        assert!(s.contains("solar [human]: hi"));
        assert!(s.contains("kore [system]: hi"));
    }

    // Live evidence (DECISIONS "Agent CLI UX"): agents copy the hint verbatim,
    // so it must model quoted text and never show the `#42` display form as
    // a flag value.
    #[test]
    fn reply_hint_models_quoted_text() {
        let s = format_messages(&[Delivery {
            id: 7,
            message: kore_protocol::Message {
                from: "luna".into(),
                sender_kind: "agent".into(),
                scope: kore_protocol::MessageScope::Broadcast,
                text: "hi".into(),
                mentions: vec![],
                delivered_to: vec![],
                intent: None,
                thread: None,
                reply_to: None,
                bundle_id: None,
            },
        }]);
        assert!(s.contains(r#"--reply-to <id> -- "your answer""#));
        assert!(!s.contains("<#id>"));
        assert!(s.contains("double-quote"));
    }

    // First sentence of the bootstrap IS the transcript identity marker —
    // resume/`fork` session discovery breaks if it drifts.
    #[test]
    fn bootstrap_starts_with_transcript_marker_and_teaches_quoting() {
        let b = bootstrap_text("luna");
        assert!(b.starts_with(&crate::transcript::session_marker("luna")));
        assert!(b.contains(r#"--reply-to <id> -- "your answer""#));
        assert!(b.contains("double-quote"));
    }

    #[test]
    fn gemini_hook_merge_is_idempotent_and_complete() {
        let mut s = serde_json::json!({
            "theme": "dark",
            "hooks": { "AfterAgent": [{ "matcher": "*", "hooks": [{ "name": "user-thing", "type": "command", "command": "x" }] }] }
        });
        merge_gemini_hooks(&mut s, "/bin/centaury");
        merge_gemini_hooks(&mut s, "/bin/centaury"); // second run must not duplicate

        assert_eq!(s["theme"], "dark"); // untouched user settings survive
        assert_eq!(s["tools"]["enableHooks"], true);
        assert_eq!(s["hooksConfig"]["enabled"], true);
        for (event, suffix, _) in GEMINI_HOOKS {
            let kore: Vec<_> = s["hooks"][event]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| {
                    e["hooks"][0]["name"]
                        .as_str()
                        .unwrap_or("")
                        .starts_with("kore-")
                })
                .collect();
            assert_eq!(kore.len(), 1, "{event} duplicated");
            assert_eq!(
                kore[0]["hooks"][0]["command"],
                format!("/bin/centaury hook gemini {suffix}")
            );
        }
        // foreign entry on the same event survives
        assert_eq!(s["hooks"]["AfterAgent"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn antigravity_hook_merge_preserves_foreign_keys() {
        let mut root = serde_json::json!({ "other-group": { "PreInvocation": [] } });
        merge_antigravity_hooks(&mut root, "/bin/centaury");
        merge_antigravity_hooks(&mut root, "/bin/centaury"); // idempotent overwrite

        assert!(root["other-group"].is_object(), "foreign groups survive");
        let kl = &root["kore-lifecycle"];
        assert_eq!(kl["PreInvocation"].as_array().unwrap().len(), 2);
        assert_eq!(
            kl["PreInvocation"][0]["command"],
            "/bin/centaury hook antigravity sessionstart"
        );
        assert_eq!(
            kl["PreInvocation"][1]["command"],
            "/bin/centaury hook antigravity beforeagent"
        );
        assert_eq!(
            kl["PostInvocation"][0]["command"],
            "/bin/centaury hook antigravity afteragent"
        );
        assert_eq!(kl["PostInvocation"][0]["timeout"], 86400); // blocking wait
        assert_eq!(kl["PreInvocation"][0]["type"], "command");
    }

    #[test]
    fn antigravity_permissions_round_trip_preserves_user_entries() {
        // A user's own always-allow rule must survive install AND uninstall.
        let mut root = serde_json::json!({
            "theme": "dark",
            "permissions": { "allow": ["command(ls)"] }
        });
        merge_antigravity_permissions(&mut root, true);
        merge_antigravity_permissions(&mut root, true); // idempotent: no dup
        let allow = root["permissions"]["allow"].as_array().unwrap();
        assert!(
            allow.iter().any(|e| e == "command(ls)"),
            "user rule survives"
        );
        assert!(allow.iter().any(|e| e == "command(centaury send)"));
        assert_eq!(
            allow
                .iter()
                .filter(|e| *e == "command(centaury send)")
                .count(),
            1,
            "no duplicate on re-install"
        );

        merge_antigravity_permissions(&mut root, false);
        assert_eq!(root["theme"], "dark", "unrelated keys untouched");
        let allow = root["permissions"]["allow"].as_array().unwrap();
        assert_eq!(
            allow,
            &vec![serde_json::json!("command(ls)")],
            "only kore rules removed"
        );

        // With no user rules left, removal cleans the empty scaffolding.
        let mut bare = serde_json::json!({});
        merge_antigravity_permissions(&mut bare, true);
        merge_antigravity_permissions(&mut bare, false);
        assert!(
            bare.get("permissions").is_none(),
            "empty permissions cleaned"
        );
    }

    /// kilo = opencode fork: the single plugin source must fully rewrite to
    /// the kilo dialect (import + TOOL), and the original must be untouched.
    #[test]
    fn kilo_plugin_rewrite_is_complete() {
        assert!(OPENCODE_PLUGIN.contains("@opencode-ai/plugin"));
        assert!(OPENCODE_PLUGIN.contains("const TOOL = \"opencode\""));
        assert!(OPENCODE_PLUGIN.contains("KorePlugin"));

        let kilo = OPENCODE_PLUGIN
            .replace("@opencode-ai/plugin", "@kilocode/plugin")
            .replace("const TOOL = \"opencode\"", "const TOOL = \"kilo\"");
        assert!(kilo.contains("@kilocode/plugin"));
        assert!(kilo.contains("const TOOL = \"kilo\""));
        assert!(!kilo.contains("opencode-ai"));
    }

    /// The cline wrapper script must emit valid JSON with the hook output
    /// (quotes, backslashes, newlines) correctly escaped — run the real
    /// script under sh with a fake centaury on PATH (B2's trick).
    #[test]
    #[cfg(unix)]
    fn cline_script_escapes_hook_output_into_json() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("kore-cline-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();

        // fake centaury: multiline output with JSON-hostile characters
        let fake = base.join("centaury");
        std::fs::write(
            &fake,
            "#!/bin/sh\nprintf 'say \"hi\" to C:\\\\path\\nline two\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let script = base.join("TaskStart");
        std::fs::write(&script, cline_hook_script("session-start")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        // base first so the fake centaury shadows any real one; the rest
        // of PATH stays for sh/awk.
        let path = format!(
            "{}:{}",
            base.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = std::process::Command::new("/bin/sh")
            .arg(&script)
            .env("PATH", &path)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("script must print valid JSON");
        assert_eq!(v["cancel"], false);
        assert_eq!(v["contextModification"], "say \"hi\" to C:\\path\nline two");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// W3 pin: marker present while the guard lives, gone after; a marker
    /// older than the hook timeout reads as absent (timeout 0 makes any
    /// marker stale without mtime games).
    #[test]
    fn wait_marker_lifecycle() {
        let dir = std::env::temp_dir().join(format!("kore-wait-test-{}", std::process::id()));
        unsafe {
            std::env::set_var("KORE_DIR", &dir);
            std::env::set_var("KORE_NAME", "testwait");
            std::env::remove_var("KORE_PROJECT");
            std::env::remove_var("KORE_HOOK_TIMEOUT");
        }

        let path = config::agent_wait_path("testwait", None);
        {
            let _m = WaitMarker::engage().expect("engage");
            assert!(path.exists(), "marker present during wait");
            assert!(wait_marker_active("testwait", None));
            unsafe { std::env::set_var("KORE_HOOK_TIMEOUT", "0") };
            assert!(
                !wait_marker_active("testwait", None),
                "stale marker treated absent"
            );
            unsafe { std::env::remove_var("KORE_HOOK_TIMEOUT") };
        }
        assert!(!path.exists(), "marker gone after wait");
        assert!(!wait_marker_active("testwait", None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G2 pin: SubagentStop (and junk) must return instantly and silently —
    /// never block on the inbox or register under the parent's identity.
    /// stop_poll would need a server and ~KORE_HOOK_TIMEOUT secs; the 2s
    /// budget catches any accidental rewiring to it. Same pin for a
    /// subagent's PostToolUse (agent_id present): the W1 drain must never
    /// consume the parent's inbox from inside a subagent.
    #[tokio::test]
    async fn subagent_stop_is_a_silent_noop() {
        for input in [
            r#"{"hook_event_name":"SubagentStop","session_id":"s1"}"#,
            r#"{"hook_event_name":"PostToolUse","session_id":"s1","agent_id":"a1","tool_name":"Bash"}"#,
            "not json at all",
        ] {
            let fut = dispatch_stdin_event("claude", input);
            let res = tokio::time::timeout(std::time::Duration::from_secs(2), fut).await;
            assert!(res.expect("must not block").is_ok(), "input: {input}");
        }
    }

    /// antigravity pin: its PostToolUse must emit exactly `{}` — a stray
    /// aftertool argv call must be a silent noop, never a drain (whose JSON
    /// output would break the tool). Gemini-only guard in run_argv_hook.
    #[tokio::test]
    async fn antigravity_aftertool_is_a_silent_noop() {
        let fut = run_argv_hook("antigravity", "aftertool");
        let res = tokio::time::timeout(std::time::Duration::from_secs(2), fut).await;
        assert!(res.expect("must not block").is_ok());
    }

    /// W1 follow-up pin: codex install wires the mid-turn PostToolUse hook.
    #[test]
    fn codex_merge_wires_posttooluse() {
        let mut root = serde_json::json!({ "other": true });
        merge_codex_hooks(&mut root, "/bin/centaury").expect("merge");
        assert_eq!(root["other"], true, "foreign keys survive");
        for event in ["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"] {
            assert_eq!(
                root["hooks"][event][0]["hooks"][0]["command"], "/bin/centaury hook codex",
                "{event} missing"
            );
        }
    }

    /// Hermes config.yaml is comment-heavy user property: the marker-fenced
    /// merge must append/replace/strip its own region ONLY.
    #[test]
    fn hermes_config_merge_round_trips() {
        let original = "model: claude\n# user comment\nhooks_auto_accept: false\n";
        let merged = hermes_merge_config(original, "/bin/centaury").unwrap();
        assert!(merged.contains("# user comment"), "user comments survive");
        assert!(merged.contains("\"/bin/centaury hook hermes\""));
        for (event, timeout) in HERMES_HOOK_EVENTS {
            assert!(merged.contains(&format!("  {event}:")), "{event} missing");
            assert!(merged.contains(&format!("timeout: {timeout}")));
        }

        // Idempotent re-merge with a moved exe: one region, updated path.
        let remerged = hermes_merge_config(&merged, "/moved/centaury").unwrap();
        assert_eq!(remerged.matches(HERMES_MARK_BEGIN_PREFIX).count(), 1);
        assert!(remerged.contains("/moved/centaury hook hermes"));
        assert!(!remerged.contains("/bin/centaury"));

        let stripped = hermes_strip_config(&remerged);
        assert_eq!(
            stripped.trim_end(),
            original.trim_end(),
            "strip restores the file"
        );
    }

    #[test]
    fn hermes_merge_swaps_empty_hooks_and_refuses_foreign() {
        // The shipped default `hooks: {}` is swapped in place, not duplicated.
        let cfg = "a: 1\nhooks: {}\nb: 2\n";
        let merged = hermes_merge_config(cfg, "/bin/centaury").unwrap();
        assert_eq!(
            merged.matches("\nhooks:").count() + usize::from(merged.starts_with("hooks:")),
            1,
            "no duplicate hooks key"
        );
        assert!(merged.contains("a: 1\n") && merged.contains("b: 2\n"));

        // A real user hooks block is never rewritten — the Err carries the
        // paste-ready YAML instead.
        let foreign = "hooks:\n  pre_tool_call:\n  - command: \"my-guard\"\n";
        let err = hermes_merge_config(foreign, "/bin/centaury").unwrap_err();
        assert!(
            err.to_string().contains("hook hermes"),
            "error must carry the YAML"
        );
    }

    /// G2 pin, hermes-shaped: payloads carrying parent_session_id (delegate
    /// child sessions) answer {} before any identity/network path — same
    /// invariant as claude's SubagentStop/agent_id rule. Junk stays silent.
    #[tokio::test]
    async fn hermes_subagent_and_junk_are_silent_noops() {
        for input in [
            r#"{"hook_event_name":"pre_verify","session_id":"s1","parent_session_id":"p1"}"#,
            r#"{"hook_event_name":"pre_llm_call","session_id":"s1","parent_session_id":"p1"}"#,
            r#"{"hook_event_name":"subagent_stop","session_id":"s1"}"#,
            "not json at all",
        ] {
            let fut = dispatch_hermes_event(input);
            let res = tokio::time::timeout(std::time::Duration::from_secs(2), fut).await;
            assert!(res.expect("must not block").is_ok(), "input: {input}");
        }
    }

    /// Consent rows must be surgical (foreign approvals survive add+remove)
    /// and carry real ISO-8601 stamps — hermes's doctor may parse them.
    #[test]
    fn hermes_allowlist_rows_are_surgical() {
        let dir = std::env::temp_dir().join(format!("kore-hermes-allow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("shell-hooks-allowlist.json"),
            r#"{"approvals":[{"event":"pre_tool_call","command":"my-guard","approved_at":"2026-01-01T00:00:00Z"}]}"#,
        )
        .unwrap();
        unsafe { std::env::set_var("HERMES_HOME", &dir) };

        update_hermes_allowlist(true).unwrap();
        update_hermes_allowlist(true).unwrap(); // idempotent: no duplicate rows
        let v = read_json(&dir.join("shell-hooks-allowlist.json")).unwrap();
        let arr = v["approvals"].as_array().unwrap();
        assert!(
            arr.iter().any(|e| e["command"] == "my-guard"),
            "foreign row survives"
        );
        let kore: Vec<_> = arr
            .iter()
            .filter(|e| e["command"].as_str().unwrap_or("").contains(" hook hermes"))
            .collect();
        assert_eq!(kore.len(), HERMES_HOOK_EVENTS.len(), "one row per event");
        for e in &kore {
            let ts = e["approved_at"].as_str().unwrap();
            assert!(ts.ends_with('Z') && ts.len() == 20, "real ISO-8601: {ts}");
        }

        update_hermes_allowlist(false).unwrap();
        let v = read_json(&dir.join("shell-hooks-allowlist.json")).unwrap();
        assert_eq!(
            v["approvals"].as_array().unwrap().len(),
            1,
            "only kore rows removed"
        );
        unsafe { std::env::remove_var("HERMES_HOME") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn iso8601_matches_known_dates() {
        assert_eq!(iso8601_utc(std::time::UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_730_000_000);
        assert_eq!(iso8601_utc(t), "2024-10-27T03:33:20Z");
    }

    fn inject_item(
        kind: &str,
        name: &str,
        mode: &str,
        cadence_kind: &str,
        cadence_value: i64,
    ) -> InjectItem {
        InjectItem {
            kind: kind.into(),
            name: name.into(),
            description: "trigger desc".into(),
            content: "BODY".into(),
            skill_kind: None,
            inject_mode: mode.into(),
            cadence_kind: cadence_kind.into(),
            cadence_value,
        }
    }

    #[test]
    fn cadence_messages_fires_at_threshold_and_resets() {
        let mut c = Counter::default();
        // cadence 3 messages, min-interval 0 for the test
        assert!(!advance_counter(&mut c, "messages", 3, 1, 0, 100, 0));
        assert!(!advance_counter(&mut c, "messages", 3, 1, 0, 100, 0));
        assert!(advance_counter(&mut c, "messages", 3, 1, 0, 100, 0)); // 3rd → due
        assert_eq!(c.count, 0, "counter resets on fire");
        assert!(!advance_counter(&mut c, "messages", 3, 1, 0, 100, 0)); // counting again
    }

    #[test]
    fn cadence_min_interval_blocks_rapid_refire() {
        let mut c = Counter::default();
        // threshold 1 would fire every event, but min-interval gates re-fire
        assert!(advance_counter(&mut c, "messages", 1, 1, 0, 100, 10)); // fires, last=100
        assert!(!advance_counter(&mut c, "messages", 1, 1, 0, 100, 10)); // same sec → blocked
        assert!(advance_counter(&mut c, "messages", 1, 1, 0, 110, 10)); // +10s → allowed
    }

    #[test]
    fn cadence_tokens_uses_delta_not_turns() {
        let mut c = Counter::default();
        // cadence 100 tokens; each event delivers 40 tokens
        assert!(!advance_counter(&mut c, "tokens", 100, 1, 40, 0, 0)); // 40
        assert!(!advance_counter(&mut c, "tokens", 100, 1, 40, 0, 0)); // 80
        assert!(advance_counter(&mut c, "tokens", 100, 1, 40, 0, 0)); // 120 → due
    }

    #[test]
    fn format_role_is_full_skill_respects_mode() {
        let role = inject_item("role", "Reviewer", "full", "messages", 20);
        let r = format_inject_item(&role);
        assert!(r.contains("[kore role] Reviewer") && r.contains("BODY"));

        let ptr = inject_item("skill", "graphify", "pointer", "messages", 15);
        let s = format_inject_item(&ptr);
        assert!(s.contains("Use 'graphify' when relevant: trigger desc"));
        assert!(!s.contains("BODY"), "pointer omits the body");

        let full = inject_item("skill", "graphify", "full", "messages", 15);
        assert!(
            format_inject_item(&full).contains("BODY"),
            "full includes the body"
        );
    }
}
