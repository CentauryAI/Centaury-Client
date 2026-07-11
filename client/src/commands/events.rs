//! `events` — typed event stream (life/status) + subscriptions (G5).
//!
//! Query is newest-first. A subscription delivers a normal kore system
//! message when it fires, so notifications arrive like any other message
//! (hooks, TUI, listen) — there is no separate delivery channel to babysit.

use kore_protocol::{CreateEventSubRequest, CreateEventSubResponse, EventRecord, EventSubInfo};

use crate::config;

#[derive(clap::Args, Debug)]
pub struct EventsArgs {
    #[command(subcommand)]
    pub subcmd: Option<EventsSubcmd>,
    /// Max events to show (newest first)
    #[arg(long, default_value_t = 20)]
    pub last: i64,
    /// Filter by event type (life|status)
    #[arg(long = "type")]
    pub event_type: Option<String>,
    /// Filter by instance name
    #[arg(long)]
    pub agent: Option<String>,
}

#[derive(clap::Subcommand, Debug)]
pub enum EventsSubcmd {
    /// Subscribe: get a kore message when a matching event fires
    Sub {
        /// life|status (omit = any)
        #[arg(long = "type")]
        event_type: Option<String>,
        /// Instance name (omit = any)
        #[arg(long)]
        agent: Option<String>,
        /// life action: created|stopped (omit = any)
        #[arg(long)]
        action: Option<String>,
        /// Remove the subscription after its first match
        #[arg(long)]
        once: bool,
    },
    /// List your subscriptions
    Subs,
    /// Remove a subscription by id
    Unsub { id: String },
}

pub async fn run(args: EventsArgs) -> anyhow::Result<()> {
    match args.subcmd {
        None => {
            let mut path = format!("/v1/events?last={}", args.last);
            if let Some(t) = &args.event_type {
                path.push_str(&format!("&type={t}"));
            }
            if let Some(a) = &args.agent {
                path.push_str(&format!("&instance={a}"));
            }
            let events: Vec<EventRecord> = crate::get_json(&path).await?;
            if events.is_empty() {
                println!("no events");
            }
            for e in events {
                println!(
                    "#{:<6} {:<27} {:<7} {:<16} {}",
                    e.id,
                    e.timestamp,
                    e.r#type,
                    e.instance,
                    summary(&e.data)
                );
            }
        }
        Some(EventsSubcmd::Sub {
            event_type,
            agent,
            action,
            once,
        }) => {
            let req = CreateEventSubRequest {
                event_type,
                instance: agent,
                action,
                once,
            };
            let resp = config::http_client()
                .post(format!("{}/v1/events/subs", config::server_url()))
                .bearer_auth(config::load_token()?)
                .json(&req)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "subscribe failed ({}): {}",
                    resp.status(),
                    resp.text().await?
                );
            }
            let created: CreateEventSubResponse = resp.json().await?;
            println!(
                "subscribed: {} — you'll get a kore message when it fires",
                created.id
            );
        }
        Some(EventsSubcmd::Subs) => {
            let subs: Vec<EventSubInfo> = crate::get_json("/v1/events/subs").await?;
            if subs.is_empty() {
                println!("no subscriptions — `kore-client events sub --type life` to add one");
            }
            for s in subs {
                let any = || "*".to_string();
                println!(
                    "{:<14} type={:<7} agent={:<16} action={:<9} {}",
                    s.id,
                    s.event_type.unwrap_or_else(any),
                    s.instance.unwrap_or_else(any),
                    s.action.unwrap_or_else(any),
                    if s.once { "once" } else { "" }
                );
            }
        }
        Some(EventsSubcmd::Unsub { id }) => {
            let resp = config::http_client()
                .delete(format!("{}/v1/events/subs/{id}", config::server_url()))
                .bearer_auth(config::load_token()?)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("unsub failed ({}): {}", resp.status(), resp.text().await?);
            }
            println!("unsubscribed {id}");
        }
    }
    Ok(())
}

/// One-line human summary of an event's data payload.
fn summary(data: &serde_json::Value) -> String {
    if let Some(action) = data.get("action").and_then(|v| v.as_str()) {
        let killed = data
            .get("killed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return if killed {
            format!("{action} (killed)")
        } else {
            action.to_string()
        };
    }
    if let Some(ctx) = data.get("context").and_then(|v| v.as_str()) {
        return format!("→ \"{ctx}\"");
    }
    data.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_covers_all_event_shapes() {
        assert_eq!(
            summary(&serde_json::json!({"action": "created", "kind": "agent"})),
            "created"
        );
        assert_eq!(
            summary(&serde_json::json!({"action": "stopped", "killed": true})),
            "stopped (killed)"
        );
        assert_eq!(
            summary(&serde_json::json!({"context": "reviewing PR"})),
            "→ \"reviewing PR\""
        );
    }
}
