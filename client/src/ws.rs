use futures_util::{SinkExt, StreamExt};
use kore_protocol::api::Delivery;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::config;

pub type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const RECONNECT_MAX_SECS: u64 = 60;

/// Open an authenticated WebSocket. `peek=true` observes live messages
/// without consuming them (no replay, watermark untouched).
pub async fn connect(peek: bool) -> anyhow::Result<WsStream> {
    let token = config::load_token()?;
    let mut url = config::ws_url()?;
    if peek {
        url.push_str("?peek=true");
    }
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(ws_stream)
}

/// Server close code meaning another connection took over this identity.
pub const CLOSE_REPLACED: u16 = 4000;

/// Read the next Delivery frame, skipping non-text frames. None = stream ended.
/// Errors with "replaced" if the server kicked us for a newer connection.
pub async fn next_delivery(ws: &mut WsStream) -> anyhow::Result<Option<Delivery>> {
    use tokio_tungstenite::tungstenite::Message as Frame;
    while let Some(frame) = ws.next().await {
        match frame? {
            Frame::Text(text) => match serde_json::from_str::<Delivery>(&text) {
                Ok(d) => return Ok(Some(d)),
                Err(_) => eprintln!("unparseable frame: {text}"),
            },
            Frame::Close(Some(cf)) if u16::from(cf.code) == CLOSE_REPLACED => {
                anyhow::bail!("replaced: another listener connected for this identity");
            }
            _ => {}
        }
    }
    Ok(None)
}

/// D12: cumulative delivery ack — tell the server everything up to `id` has
/// been handed to the tool, so the watermark may advance past it. Send AFTER
/// emitting the messages (stdout/context), never before; a failed ack is fine
/// (at-least-once: the server replays on reconnect).
pub async fn send_ack(ws: &mut WsStream, id: i64) -> anyhow::Result<()> {
    use tokio_tungstenite::tungstenite::Message as Frame;
    let frame = serde_json::to_string(&kore_protocol::api::AckFrame { ack: id })?;
    ws.send(Frame::Text(frame)).await?;
    Ok(())
}

/// D7: `timeout_secs` wraps the WHOLE loop (connect + reads), not per
/// message; 0 = forever. Timeout is a normal exit (0). `json` = one
/// Delivery per line (NDJSON), banner suppressed.
///
/// HC3 (hcom fc9f8d7): a short timeout must never beat the first replay —
/// setup overhead could eat the whole budget and report "no messages" while
/// the backlog sat queued. One connect + backlog drain always completes
/// BEFORE the deadline is honored.
pub async fn listen(timeout_secs: u64, json: bool) -> anyhow::Result<()> {
    if timeout_secs == 0 {
        return listen_loop(json).await;
    }
    let start = std::time::Instant::now();
    quick_drain(json).await;
    let remaining = std::time::Duration::from_secs(timeout_secs).saturating_sub(start.elapsed());
    if remaining.is_zero() {
        return Ok(());
    }
    match tokio::time::timeout(remaining, listen_loop(json)).await {
        Ok(r) => r,
        Err(_) => Ok(()), // timeout reached: clean exit
    }
}

/// One connection, read until the backlog goes quiet (300ms of silence =
/// replay done), print + ack each delivery. Connection errors are left for
/// the main loop to surface — this pass only guarantees the backlog check.
async fn quick_drain(json: bool) {
    const IDLE_GAP: std::time::Duration = std::time::Duration::from_millis(300);
    let Ok(mut ws) = connect(false).await else {
        return;
    };
    while let Ok(Ok(Some(d))) = tokio::time::timeout(IDLE_GAP, next_delivery(&mut ws)).await {
        emit_delivery(&d, json);
        let _ = send_ack(&mut ws, d.id).await; // after print (D12)
    }
}

fn emit_delivery(d: &kore_protocol::api::Delivery, json: bool) {
    if json {
        if let Ok(line) = serde_json::to_string(d) {
            println!("{line}");
        }
    } else {
        println!("{}: {}", d.message.from, d.message.text);
    }
}

async fn listen_loop(json: bool) -> anyhow::Result<()> {
    let mut backoff_secs = 1;
    loop {
        match listen_once(json).await {
            // Clean close (server restart, idle close): reconnect quickly.
            Ok(()) => backoff_secs = 1,
            Err(e) if e.to_string().starts_with("replaced:") => {
                eprintln!("{e} — exiting (run one listener per identity)");
                std::process::exit(1);
            }
            Err(e) => eprintln!("connection lost: {e} — retrying in {backoff_secs}s"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(RECONNECT_MAX_SECS);
    }
}

/// One connection lifetime; the server replays missed messages on connect.
async fn listen_once(json: bool) -> anyhow::Result<()> {
    let mut ws = connect(false).await?;
    if !json {
        println!("listening for messages...");
    }
    while let Some(d) = next_delivery(&mut ws).await? {
        emit_delivery(&d, json);
        let _ = send_ack(&mut ws, d.id).await; // after print (D12)
    }
    Ok(())
}
