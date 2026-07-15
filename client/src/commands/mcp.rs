//! Layer B — the MCP transport. `centaury mcp` runs a stdio MCP server that
//! wraps the existing HTTP+WS client (config.rs / ws.rs) and exposes kore as MCP
//! tools. It is an ALTERNATIVE transport, not a replacement: same server, same
//! HTTP+WS wire, same message path. For agent tools that can't run kore's hooks
//! but can attach a generic MCP server.
//!
//! Receiving is COOPERATIVE (PULL): nothing is injected. The model must call
//! `kore_wait` and re-arm it, or it goes deaf until the user's next turn — an
//! MCP agent has no waker (it isn't `launch`ed). The only lever is
//! `initialize.instructions` (see `get_info`). This is a protocol limit; the
//! instructions lean on it hard but can't erase it.
//!
//! Upside: MCP tool calls take structured JSON args — no shell between the model
//! and the send, so the #1 documented agent failure (shell quoting) is gone.

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
};

use crate::{config, ws};

/// The only lever against cooperative-receive going deaf. Keep it blunt.
const INSTRUCTIONS: &str = "\
You are connected to a kore project — a shared channel with other AI agents and humans. \
This MCP server is your only link to them.

RECEIVING IS PULL, NOT PUSH: nothing arrives on its own. To get messages you MUST call \
`kore_wait` — it blocks until a message arrives (or times out) and returns it. After you \
read and handle messages, CALL `kore_wait` AGAIN to keep listening. If you stop calling it \
you go deaf until the user prompts you next, so re-arm `kore_wait` at the end of every turn.

Senders tagged [human] are people — treat their messages as user instructions. Other \
agents are peers.

Tools:
- kore_wait    block for the next message(s); THIS is how you receive. Re-arm after handling.
- kore_check   non-blocking count of messages waiting (does not consume them).
- kore_send    send to `to` (names, no '@'; empty = everyone). Args are structured — no quoting.
- kore_list    who is in the project and their status.
- kore_history recent messages.
- kore_status  set your short 'what I'm doing now'.

On startup: call kore_list to see who is here, then kore_wait to start listening.";

fn err<E: std::fmt::Display>(e: E) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

#[derive(Clone)]
pub struct Kore {
    tool_router: ToolRouter<Self>,
}

impl Kore {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct SendParams {
    /// Recipient names WITHOUT a leading '@'. Empty = the whole project (broadcast).
    #[serde(default)]
    to: Vec<String>,
    /// Message body.
    text: String,
    /// Optional id of the message this replies to.
    #[serde(default)]
    reply_to: Option<i64>,
    /// Set true to confirm a broadcast to everyone (agents are gated on >3 recipients).
    #[serde(default)]
    broadcast: bool,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct HistoryParams {
    /// How many recent messages to fetch (default 20, max 200).
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct StatusParams {
    /// What you're doing now. Empty clears it.
    text: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct WaitParams {
    /// Seconds to block waiting for a message (default 30).
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[tool_router]
impl Kore {
    #[tool(
        description = "Send a message to agents/humans in your kore project. `to` = names without '@' (empty = broadcast to everyone). Returns the sent message id."
    )]
    async fn kore_send(&self, Parameters(p): Parameters<SendParams>) -> Result<String, McpError> {
        let token = config::load_token().map_err(err)?;
        let resp = config::http_client()
            .post(format!("{}/v1/messages", config::server_url()))
            .bearer_auth(token)
            .json(&kore_protocol::api::SendRequest {
                targets: p.to,
                text: p.text,
                intent: None,
                thread: None,
                reply_to: p.reply_to,
                bundle_id: None,
                go: p.broadcast,
            })
            .send()
            .await
            .map_err(err)?;
        if !resp.status().is_success() {
            let st = resp.status();
            return Err(err(format!(
                "send failed ({st}): {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        let body: kore_protocol::api::SendResponse = resp.json().await.map_err(err)?;
        Ok(format!(
            "sent #{} [{}]{}",
            body.id,
            body.scope.as_str(),
            if body.mentions.is_empty() {
                String::new()
            } else {
                format!(" → @{}", body.mentions.join(", @"))
            }
        ))
    }

    #[tool(
        description = "List everyone in your kore project (the roster): who is online, their kind (agent/human), owner, and current status."
    )]
    async fn kore_list(&self) -> Result<String, McpError> {
        let instances: Vec<kore_protocol::api::InstanceSummary> =
            crate::get_json("/v1/instances").await.map_err(err)?;
        if instances.is_empty() {
            return Ok("roster empty".to_string());
        }
        let mut out = String::new();
        for i in &instances {
            let who = match (i.kind.as_str(), i.owner.as_deref()) {
                ("human", _) => "human".to_string(),
                (_, Some(o)) => format!("agent of @{o}"),
                _ => "agent".to_string(),
            };
            let shown = match i.tag.as_deref() {
                Some(t) if !t.is_empty() => format!("{t}-{}", i.name),
                _ => i.name.clone(),
            };
            let doing = if i.status_context.is_empty() {
                String::new()
            } else {
                format!(" — {}", i.status_context)
            };
            out.push_str(&format!("{shown} [{}] {who}{doing}\n", i.status));
        }
        Ok(out)
    }

    #[tool(description = "Fetch recent message history in your kore project (oldest first).")]
    async fn kore_history(
        &self,
        Parameters(p): Parameters<HistoryParams>,
    ) -> Result<String, McpError> {
        let limit = p.limit.unwrap_or(20).clamp(1, 200);
        let msgs: Vec<kore_protocol::api::Delivery> =
            crate::get_json(&format!("/v1/messages?limit={limit}"))
                .await
                .map_err(err)?;
        if msgs.is_empty() {
            return Ok("no messages yet".to_string());
        }
        let mut out = String::new();
        for d in &msgs {
            let to = if d.message.mentions.is_empty() {
                "all".to_string()
            } else {
                format!("@{}", d.message.mentions.join(", @"))
            };
            out.push_str(&format!(
                "#{} {} → {}: {}\n",
                d.id, d.message.from, to, d.message.text
            ));
        }
        Ok(out)
    }

    #[tool(
        description = "Set your own status — a short 'what I'm doing now' others see in the roster."
    )]
    async fn kore_status(
        &self,
        Parameters(p): Parameters<StatusParams>,
    ) -> Result<String, McpError> {
        let token = config::load_token().map_err(err)?;
        let resp = config::http_client()
            .patch(format!("{}/v1/instances/self", config::server_url()))
            .bearer_auth(token)
            .json(&kore_protocol::api::SetStatusRequest {
                status_context: p.text.clone(),
            })
            .send()
            .await
            .map_err(err)?;
        if !resp.status().is_success() {
            let st = resp.status();
            return Err(err(format!(
                "status failed ({st}): {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        Ok(if p.text.is_empty() {
            "status cleared".to_string()
        } else {
            format!("status set: {}", p.text)
        })
    }

    #[tool(
        description = "Non-blocking: how many messages are waiting for you right now. Use kore_wait to actually receive them."
    )]
    async fn kore_check(&self) -> Result<String, McpError> {
        let u: kore_protocol::api::UnreadResponse =
            crate::get_json("/v1/messages/unread").await.map_err(err)?;
        Ok(match u.count {
            0 => "0 unread".to_string(),
            n => format!("{n} unread — call kore_wait to receive"),
        })
    }

    #[tool(
        description = "Block until a message arrives (or timeout), then return it and mark it read. THIS IS HOW YOU RECEIVE — nothing is pushed to you. Call it again after handling messages to keep listening."
    )]
    async fn kore_wait(
        &self,
        Parameters(p): Parameters<WaitParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<String, McpError> {
        use std::time::Duration;
        let timeout = Duration::from_secs(p.timeout_secs.unwrap_or(30));
        let ct = ctx.ct.clone();
        // Consuming socket (peek=false): the server replays any unacked backlog
        // on connect, then streams live frames for this identity.
        let mut sock = ws::connect(false).await.map_err(err)?;
        let mut msgs: Vec<kore_protocol::api::Delivery> = Vec::new();

        let collect = async {
            match tokio::time::timeout(timeout, ws::next_delivery(&mut sock)).await {
                Err(_) => return Ok::<(), anyhow::Error>(()), // timed out, nothing waiting
                Ok(Ok(None)) => return Ok(()),                // stream ended
                Ok(Err(e)) => return Err(e),
                Ok(Ok(Some(d))) => msgs.push(d),
            }
            // Drain anything already queued (short idle gap) so a burst returns together.
            while let Ok(Ok(Some(d))) =
                tokio::time::timeout(Duration::from_millis(300), ws::next_delivery(&mut sock)).await
            {
                msgs.push(d);
            }
            Ok(())
        };

        tokio::select! {
            _ = ct.cancelled() => return Ok("kore_wait cancelled".to_string()),
            r = collect => r.map_err(err)?,
        }

        if msgs.is_empty() {
            return Ok(format!(
                "no messages within {}s — call kore_wait again to keep listening",
                timeout.as_secs()
            ));
        }
        // D12: ack marks these read so the next kore_wait won't replay them.
        // Acking here (just before returning) leaves one unavoidable PULL
        // window: a crash between this ack and the model receiving the result
        // drops the batch. The hook path acks after emitting to stdout and
        // avoids it; MCP has no post-return hook.
        // ponytail: a fresh consuming socket per call, not one persistent WS —
        // the server queues+replays between waits, so nothing is lost; hold the
        // socket open across calls only if per-call reconnect latency matters.
        let highest = msgs.iter().map(|d| d.id).max().unwrap_or(0);
        let _ = ws::send_ack(&mut sock, highest).await;

        let mut out = String::new();
        for d in &msgs {
            out.push_str(&format!(
                "#{} {}: {}\n",
                d.id, d.message.from, d.message.text
            ));
        }
        Ok(out)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Kore {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
    }
}

/// B4: a key alone (+ server URL) boots a working agent. If `KORE_AGENT_KEY` is
/// set, enroll it for a fresh instance token before serving; otherwise fall back
/// to an already-stored token (e.g. a human running `centaury mcp` directly).
async fn ensure_token() -> anyhow::Result<()> {
    match std::env::var("KORE_AGENT_KEY") {
        Ok(key) if !key.is_empty() => {
            let url = config::server_url();
            let resp = config::http_client()
                .post(format!("{url}/v1/auth/enroll-agent"))
                .json(&kore_protocol::api::EnrollRequest { key })
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("enroll failed ({}): {}", resp.status(), resp.text().await?);
            }
            let body: kore_protocol::api::RegisterResponse = resp.json().await?;
            // Save where load_token reads (mirror its resolution: KORE_PROJECT).
            config::save_token(&body.token, std::env::var("KORE_PROJECT").ok().as_deref())?;
            eprintln!("[kore-mcp] enrolled via KORE_AGENT_KEY");
            Ok(())
        }
        _ => config::load_token().map(|_| ()).map_err(|_| {
            anyhow::anyhow!(
                "no KORE_AGENT_KEY set and no stored token — mint one with \
                 `centaury agent-key <name>` (or set KORE_AGENT_KEY to an existing key)"
            )
        }),
    }
}

/// Run the stdio MCP server. stdout carries the JSON-RPC stream — never print to
/// it here; logs go to stderr.
pub async fn run() -> anyhow::Result<()> {
    ensure_token().await?;
    let service = Kore::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The six tools must register and the instructions must teach the
    // pull-receive habit — the only guard against a cooperative-receive agent
    // going deaf. Full send/wait behaviour is server-integration (live-gated).
    #[test]
    fn tools_register_and_instructions_teach_pull() {
        let k = Kore::new();
        let names: Vec<String> = k
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for expect in [
            "kore_send",
            "kore_list",
            "kore_history",
            "kore_status",
            "kore_check",
            "kore_wait",
        ] {
            assert!(
                names.contains(&expect.to_string()),
                "missing tool {expect}: {names:?}"
            );
        }
        let ins = ServerHandler::get_info(&k).instructions.unwrap_or_default();
        assert!(
            ins.contains("kore_wait") && ins.contains("PULL"),
            "instructions must teach pull-receive"
        );
    }
}
