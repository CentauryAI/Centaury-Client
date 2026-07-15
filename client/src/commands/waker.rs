//! W2 local waker — the "finger" that pokes a deaf agent (PLAN-WAKE-SIGNAL).
//!
//! One detached process per agent. It NEVER reads messages for the agent —
//! consumption always goes through the hook path (watermark, dedupe, single
//! consumer). It only notices "there is a message and nobody is listening"
//! and types one nudge line into the agent's terminal to start a turn.
//!
//! Deaf detection: the Stop hook holds `agents/<name>.wait` while its
//! blocking wait owns the consuming WS (hook.rs W3). Marker present →
//! messages inject themselves, poke would be noise. Before poking we also
//! re-check the server's unread count: W1's mid-turn drain may have already
//! consumed what we saw on the peek socket.
//!
//! Lifecycle: exits when the tool process dies (pid + cmdline guard, same
//! trick as kill_local); `kill` SIGTERMs it via `agents/<name>.waker.pid`.
//! Everything degrades silently — a dead waker must never break the agent.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::{config, hook, ws};

/// How often to check the parent tool is still alive.
const PARENT_POLL: Duration = Duration::from_secs(5);
/// Grace before poking: lets the Stop hook engage its marker and the W1
/// drain advance the server watermark (flushed every ~2s) after a delivery.
const POKE_GRACE: Duration = Duration::from_secs(3);
/// One nudged turn drains everything; don't stack more nudges while the
/// agent is plausibly still working through the last one.
/// ponytail: fixed cooldown, no busy detection — hooks can't see screen
/// state (decided: no PTY revival). Ceiling: a nudge can land as a queued
/// prompt on a busy agent (one noise line; W1 already delivered the text).
const POKE_COOLDOWN: Duration = Duration::from_secs(60);

const NUDGE: &str = "kore: new messages — check them";

/// Terminal poke backend, resolved from env at spawn: the waker is spawned
/// from inside the agent's terminal (launch or the SessionStart hook), so
/// $TMUX_PANE / $WEZTERM_PANE / $KITTY_WINDOW_ID identify the right pane.
enum Poke {
    Tmux(String),
    Wezterm(String),
    Kitty(String),
}

impl Poke {
    fn detect() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if let Some(pane) = var("TMUX_PANE") {
            return Some(Self::Tmux(pane));
        }
        if let Some(id) = var("WEZTERM_PANE") {
            return Some(Self::Wezterm(id));
        }
        if let Some(id) = var("KITTY_WINDOW_ID") {
            return Some(Self::Kitty(id));
        }
        None
    }

    /// Type the nudge + submit it. \r is the submit key for send-text style
    /// APIs; tmux gets an explicit Enter key press.
    fn poke(&self) -> bool {
        let ok = |c: &mut Command| c.status().is_ok_and(|s| s.success());
        match self {
            Self::Tmux(pane) => {
                ok(Command::new("tmux").args(["send-keys", "-t", pane, "-l", NUDGE]))
                    && ok(Command::new("tmux").args(["send-keys", "-t", pane, "Enter"]))
            }
            Self::Wezterm(id) => ok(Command::new("wezterm").args([
                "cli",
                "send-text",
                "--pane-id",
                id,
                "--no-paste",
                &format!("{NUDGE}\r"),
            ])),
            // Needs allow_remote_control in kitty.conf (same as --terminal split).
            Self::Kitty(id) => ok(Command::new("kitty").args([
                "@",
                "send-text",
                "--match",
                &format!("id:{id}"),
                &format!("{NUDGE}\r"),
            ])),
        }
    }
}

/// pid-reuse guard shared with kill_local's idea: a pid is "our" process only
/// while its cmdline still names what we expect.
fn cmdline_contains(pid: u32, needle: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|c| c.replace('\0', " ").contains(needle))
        .unwrap_or(false)
}

/// Server truth for "does this agent still have something unread". 0 on any
/// error — a broken check must never cause a poke.
async fn unread_count() -> i64 {
    let Ok(token) = config::load_token() else {
        return 0;
    };
    let Ok(resp) = config::http_client()
        .get(format!("{}/v1/messages/unread", config::server_url()))
        .bearer_auth(token)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    else {
        return 0;
    };
    if !resp.status().is_success() {
        return 0;
    }
    resp.json::<kore_protocol::api::UnreadResponse>()
        .await
        .map(|v| v.count)
        .unwrap_or(0)
}

/// The poke decision, taken lazily: grace sleep, then marker check (a live
/// Stop wait will inject the message itself), then server unread re-check
/// (W1 may have drained it mid-turn), then cooldown.
async fn maybe_poke(
    poke: &Poke,
    name: &str,
    project: Option<&str>,
    last_poke: &mut Option<Instant>,
) {
    tokio::time::sleep(POKE_GRACE).await;
    if hook::wait_marker_active(name, project) {
        return;
    }
    if unread_count().await == 0 {
        return;
    }
    if last_poke.is_some_and(|t| t.elapsed() < POKE_COOLDOWN) {
        return;
    }
    if poke.poke() {
        *last_poke = Some(Instant::now());
    }
}

pub async fn run(name: String, tool_pid: u32, tool: String) -> anyhow::Result<()> {
    let Some(poke) = Poke::detect() else {
        // Bare terminal: no poke surface. Documented ceiling (G-A stays open
        // there); the agent still gets everything on its next natural turn.
        eprintln!("[waker:{name}] no tmux/wezterm/kitty in env — nothing to poke, exiting");
        return Ok(());
    };
    let project = std::env::var("KORE_PROJECT").ok();

    // One waker per instance: a pid file whose process still IS a waker wins.
    let pid_path = config::agent_waker_pid_path(&name, project.as_deref());
    if let Ok(existing) = std::fs::read_to_string(&pid_path)
        && let Ok(pid) = existing.trim().parse::<u32>()
        && pid != std::process::id()
        && cmdline_contains(pid, "waker")
    {
        return Ok(());
    }
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let mut last_poke: Option<Instant> = None;
    // Outer loop: (re)connect the peek socket. Peek has NO replay, so a
    // backlog accumulated while we were down or the agent was deaf is only
    // visible via the unread count — check it on every (re)connect.
    while cmdline_contains(tool_pid, &tool) {
        if unread_count().await > 0 {
            maybe_poke(&poke, &name, project.as_deref(), &mut last_poke).await;
        }
        // Observe-only socket: can never steal from the real listener.
        let Ok(mut socket) = ws::connect(true).await else {
            tokio::time::sleep(PARENT_POLL).await;
            continue;
        };
        loop {
            match tokio::time::timeout(PARENT_POLL, ws::next_delivery(&mut socket)).await {
                // Idle tick: only checking the parent is still alive.
                Err(_) => {
                    if !cmdline_contains(tool_pid, &tool) {
                        let _ = std::fs::remove_file(&pid_path);
                        return Ok(());
                    }
                }
                // The server already filtered: a peek frame IS for us.
                Ok(Ok(Some(_))) => {
                    maybe_poke(&poke, &name, project.as_deref(), &mut last_poke).await;
                }
                // Stream ended or errored: reconnect.
                _ => break,
            }
        }
    }
    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdline_guard_matches_own_process() {
        let me = std::process::id();
        assert!(
            cmdline_contains(me, "centaury"),
            "test binary path contains 'centaury'"
        );
        assert!(!cmdline_contains(me, "definitely-not-in-any-cmdline"));
        assert!(
            !cmdline_contains(u32::MAX - 1, "anything"),
            "dead pid never matches"
        );
    }

    /// Backend priority: tmux > wezterm > kitty; none of the env vars → None
    /// (bare terminal, waker exits).
    #[test]
    fn poke_backend_detection_priority() {
        unsafe {
            std::env::remove_var("TMUX_PANE");
            std::env::remove_var("WEZTERM_PANE");
            std::env::remove_var("KITTY_WINDOW_ID");
        }
        assert!(Poke::detect().is_none());
        unsafe { std::env::set_var("KITTY_WINDOW_ID", "7") };
        assert!(matches!(Poke::detect(), Some(Poke::Kitty(id)) if id == "7"));
        unsafe { std::env::set_var("WEZTERM_PANE", "2") };
        assert!(matches!(Poke::detect(), Some(Poke::Wezterm(id)) if id == "2"));
        unsafe { std::env::set_var("TMUX_PANE", "%3") };
        assert!(matches!(Poke::detect(), Some(Poke::Tmux(p)) if p == "%3"));
        unsafe {
            std::env::remove_var("TMUX_PANE");
            std::env::remove_var("WEZTERM_PANE");
            std::env::remove_var("KITTY_WINDOW_ID");
        }
    }
}
