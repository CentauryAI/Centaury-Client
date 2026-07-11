use std::path::PathBuf;

/// Base kore dir: `KORE_DIR` if set, else `~/.kore`.
pub fn kore_dir() -> PathBuf {
    std::env::var("KORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".kore"))
}

/// Token location for an identity. Global agents live at `$KORE_DIR/token`;
/// project agents at `$KORE_DIR/<project>/token`, one identity per project.
pub fn token_path_for(project: Option<&str>) -> PathBuf {
    let mut dir = kore_dir();
    if let Some(p) = project {
        dir = dir.join(p);
    }
    dir.join("token")
}

/// Resolution order: `KORE_TOKEN_FILE` (explicit override) → named-agent path
/// (`KORE_NAME` set, i.e. anything spawned via `launch`) → `KORE_PROJECT`
/// path → global `$KORE_DIR/token`.
///
/// Named agents get `.../agents/<name>.token` so a launched agent NEVER picks
/// up the human's own token at `$KORE_DIR/<project>/token` (which made agents
/// speak as their owner).
/// D10: are we running inside a launched agent? Same signal the hooks use —
/// `launch` always sets KORE_NAME. Destructive verbs (unregister/kill) demand
/// --go in this context so an AI can't wipe an instance on a whim; a human
/// shell (no KORE_NAME) is never gated.
pub fn inside_agent_context() -> bool {
    std::env::var("KORE_NAME").is_ok_and(|v| !v.is_empty())
}

fn token_path() -> PathBuf {
    if let Ok(path) = std::env::var("KORE_TOKEN_FILE") {
        return PathBuf::from(path);
    }
    let project = std::env::var("KORE_PROJECT").ok();
    if let Ok(name) = std::env::var("KORE_NAME") {
        let mut dir = kore_dir();
        if let Some(p) = &project {
            dir = dir.join(p);
        }
        return dir.join("agents").join(format!("{name}.token"));
    }
    token_path_for(project.as_deref())
}

/// Stable derived agent name (host+cwd+project hash) — deterministic so
/// retries never mint duplicates. 4-letter CVCV words like legacy
/// instance_names.rs (luna, vega, ...) instead of agent-XXXXXX.
pub fn derived_name() -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::env::var("HOSTNAME").unwrap_or_default().hash(&mut h);
    std::env::current_dir().unwrap_or_default().hash(&mut h);
    std::env::var("KORE_PROJECT").unwrap_or_default().hash(&mut h);
    cvcv_fresh(h.finish())
}

/// Fresh random CVCV name per call — `launch` uses this when no --name is
/// given so two launches from the same directory never collide (legacy
/// behavior; the deterministic hash is only for hook self-registration).
pub fn random_name() -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        .hash(&mut h);
    std::process::id().hash(&mut h);
    cvcv_fresh(h.finish())
}

fn cvcv_fresh(seed: u64) -> String {
    let mut n = seed;
    loop {
        let name = cvcv(n);
        // The only CVCV-producible words from legacy's banned list (the rest
        // can't match the consonant-vowel pattern): re-roll deterministically.
        if !["sudo", "kilo", "meta"].contains(&name.as_str()) {
            return name;
        }
        n = n.wrapping_add(1);
    }
}

/// Pronounceable consonant-vowel-consonant-vowel word from a hash.
fn cvcv(mut n: u64) -> String {
    const C: &[u8] = b"bdfghklmnprstvz";
    const V: &[u8] = b"aeiou";
    let mut s = String::new();
    for set in [C, V, C, V] {
        s.push(set[(n % set.len() as u64) as usize] as char);
        n /= set.len() as u64;
    }
    s
}

/// Pid of the tool process `launch` spawned for an agent, next to its token.
/// `kill` signals it so headless/windowed agents actually die (G1).
pub fn agent_pid_path(name: &str, project: Option<&str>) -> PathBuf {
    agent_file_path(name, project, "pid")
}

/// Token file of a named (launched) agent. Written by `save_token` the moment
/// the server accepts registration — `launch --wait` polls it as the
/// registration receipt (G4).
pub fn agent_token_path(name: &str, project: Option<&str>) -> PathBuf {
    agent_file_path(name, project, "token")
}

/// Wait-marker: present while the agent's blocking Stop hook holds the
/// consuming WS (hook.rs `WaitMarker`, W3). The local waker checks it before
/// poking — marker present means messages will inject themselves.
pub fn agent_wait_path(name: &str, project: Option<&str>) -> PathBuf {
    agent_file_path(name, project, "wait")
}

/// Pid of the agent's local waker process (W2) — dedup at waker start,
/// SIGTERM target for `kill`.
pub fn agent_waker_pid_path(name: &str, project: Option<&str>) -> PathBuf {
    agent_file_path(name, project, "waker.pid")
}

fn agent_file_path(name: &str, project: Option<&str>, ext: &str) -> PathBuf {
    let mut dir = kore_dir();
    if let Some(p) = project {
        dir = dir.join(p);
    }
    dir.join("agents").join(format!("{name}.{ext}"))
}

pub fn server_url() -> String {
    std::env::var("KORE_SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Shared org secret required by the server to register an instance.
pub fn reg_secret() -> anyhow::Result<String> {
    std::env::var("KORE_REG_SECRET")
        .map_err(|_| anyhow::anyhow!("KORE_REG_SECRET must be set to register (ask your org admin)"))
}

/// HTTP client with a timeout so a stalled server fails fast instead of
/// wedging the calling agent's turn.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

/// Derive the WebSocket URL from the server URL (http→ws, https→wss).
pub fn ws_url() -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(&server_url())?;
    let scheme = match url.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        other => anyhow::bail!("unsupported server URL scheme '{other}'"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("cannot derive ws scheme for {url}"))?;
    Ok(url.join("/v1/ws")?.to_string())
}

pub fn save_token(token: &str, project: Option<&str>) -> anyhow::Result<()> {
    // KORE_TOKEN_FILE and KORE_NAME must route writes exactly like reads,
    // or the token lands where load_token never looks.
    let path = if std::env::var("KORE_TOKEN_FILE").is_ok() || std::env::var("KORE_NAME").is_ok() {
        token_path()
    } else {
        token_path_for(project)
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    Ok(())
}

/// Instance name from the stored token's `sub` claim (decoded, not verified —
/// the server is the verifier; this is display/bootstrap only).
pub fn instance_name() -> anyhow::Result<String> {
    use base64::Engine;
    let token = load_token()?;
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("malformed token"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes)?;
    claims["sub"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("token missing sub claim"))
}

pub fn load_token() -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(token_path())
        .map_err(|_| anyhow::anyhow!("not registered — run `kore-client register <name>` first"))?
        .trim()
        .to_string())
}

/// Human session token (from login/signup), separate from instance tokens.
fn session_path() -> PathBuf {
    kore_dir().join("session")
}

pub fn save_session(token: &str) -> anyhow::Result<()> {
    let path = session_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    Ok(())
}

pub fn load_session() -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(session_path())
        .map_err(|_| anyhow::anyhow!("not logged in — run `kore-client login` first"))?
        .trim()
        .to_string())
}
