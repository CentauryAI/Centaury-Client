//! Read-only parsers for tool session logs, plus session discovery for
//! `resume`. One dispatch pair (`find_session`/`parse`) covers claude,
//! gemini and codex.
//!
//! Where each tool keeps sessions (verified against real files / vendor docs):
//! - claude: `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl`. We never
//!   reproduce the cwd encoding: entry lines carry a real `cwd` field, so we
//!   scan all project dirs and match on that (legacy-proven, encoding-proof).
//! - gemini: `~/.gemini/tmp/<project>/chats/session-<ts>-<idprefix>.jsonl`
//!   (older versions: `.json`). Line 1 is a header with the full `sessionId`;
//!   `<project>` is the lowercased project basename on current versions, an
//!   opaque hash on older ones.
//! - codex: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`. The
//!   `session_meta` line carries `payload.cwd` and `payload.id` (= the resume
//!   id, stable across resumes).
//! - omp: `($PI_CODING_AGENT_DIR|~/.omp/agent)/sessions/<cwd-slug>/<ts>_<uuid>.jsonl`.
//!   A `type:"session"` header line carries `id` + `cwd` (live-verified,
//!   omp 16.3.8). opencode/kilo store sessions in SQLite (opencode.db) —
//!   discovery there needs a rusqlite dep, deferred until asked (HC13 gate).

use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One user/assistant turn extracted from a transcript.
pub struct Exchange {
    pub ts: String,
    pub role: String,
    pub text: String,
}

/// The SessionStart bootstrap line hooks inject (hook.rs) — it lands in every
/// tool's transcript, so it doubles as an identity marker: the one reliable
/// signal that a session file belongs to a given kore agent and not, say, its
/// owner working in the same directory.
pub fn session_marker(name: &str) -> String {
    format!("[kore] You are agent '{name}'")
}

/// Newest session file provably belonging to agent `name` in directory `dir`.
/// No marker → no guess: resuming the newest unmarked session could hijack
/// the owner's own work in the same directory (all three finders enforce it).
pub fn find_session(tool: &str, name: &str, dir: &str) -> Option<(String, PathBuf)> {
    match tool {
        "claude" => find_claude_session(name, dir),
        "gemini" => find_gemini_session(name, dir),
        "codex" => find_codex_session(name, dir),
        "omp" => find_omp_session(name, dir),
        _ => None,
    }
}

/// Parse a transcript into user/assistant exchanges (tool-dispatched).
pub fn parse(tool: &str, path: &Path) -> anyhow::Result<Vec<Exchange>> {
    match tool {
        "claude" => parse_claude(path),
        "gemini" => parse_gemini(path),
        "codex" => parse_codex(path),
        "omp" => parse_omp(path),
        other => anyhow::bail!("no transcript parser for tool '{other}'"),
    }
}

/// All files under `roots` (recursive) whose extension is in `exts`,
/// newest-modified first — the shared walk behind all three finders.
fn files_newest_first(roots: impl IntoIterator<Item = PathBuf>, exts: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut stack: Vec<PathBuf> = roots.into_iter().collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e))
                && let Ok(mtime) = entry.metadata().and_then(|m| m.modified())
            {
                files.push((mtime, path));
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().map(|(_, p)| p).collect()
}

// ---- claude ----

/// Config roots holding `projects/`: CLAUDE_CONFIG_DIR (if set) and the
/// default `~/.claude` — a machine can have both in use (e.g. subscription
/// vs default installs), and the agent may have run under either.
fn claude_dirs() -> Vec<PathBuf> {
    let mut dirs_: Vec<PathBuf> = std::env::var("CLAUDE_CONFIG_DIR").map(PathBuf::from).into_iter().collect();
    let default = dirs::home_dir().unwrap_or_default().join(".claude");
    if !dirs_.contains(&default) {
        dirs_.push(default);
    }
    dirs_
}

/// Buffered line iterator — discovery scans many multi-MB transcripts, so
/// finders stream and early-exit instead of `read_to_string`ing every
/// candidate (stops at the first unreadable/non-UTF-8 line).
fn lines_of(path: &Path) -> Option<impl Iterator<Item = String> + use<>> {
    let file = std::fs::File::open(path).ok()?;
    Some(std::io::BufReader::new(file).lines().map_while(Result::ok))
}

/// Working directory of a session: `cwd` sits on most entry lines but not
/// necessarily line 1, so scan a few lines and take the first non-empty one.
pub fn cwd_of(path: &Path) -> Option<String> {
    lines_of(path)?.take(20).find_map(|line| {
        let v = serde_json::from_str::<Value>(&line).ok()?;
        v.get("cwd")
            .and_then(|c| c.as_str())
            .filter(|c| !c.is_empty())
            .map(str::to_string)
    })
}

fn find_claude_session(name: &str, dir: &str) -> Option<(String, PathBuf)> {
    let marker = session_marker(name);
    let roots = claude_dirs().into_iter().map(|r| r.join("projects"));
    for path in files_newest_first(roots, &["jsonl"]) {
        if cwd_of(&path).as_deref() == Some(dir) && has_session_start_marker(&path, &marker) {
            let id = path.file_stem()?.to_str()?.to_string();
            return Some((id, path));
        }
    }
    None
}

/// True only for a *genuine* SessionStart hook injection: an `attachment`
/// entry with `hookEvent == "SessionStart"` whose stdout carries the marker.
/// A naive substring scan false-positives on sessions that merely DISCUSS the
/// marker text (an agent reading kore's own source, say); a conversation
/// cannot fabricate a top-level hook attachment entry.
fn has_session_start_marker(path: &Path, marker: &str) -> bool {
    let Some(lines) = lines_of(path) else { return false };
    // The injection sits near the top of the file, so this exits early.
    lines.into_iter().any(|line| {
        // cheap pre-filter before JSON parse
        line.contains(marker)
            && line.contains("hookEvent")
            && serde_json::from_str::<Value>(&line).is_ok_and(|v| {
                let att = &v["attachment"];
                att["hookEvent"] == "SessionStart"
                    && att["stdout"].as_str().is_some_and(|s| s.contains(marker))
            })
    })
}

/// Parse a claude `.jsonl` transcript into user/assistant exchanges.
/// Meta, sidechain and compact-summary entries are skipped, as are
/// tool-use-only turns (no text blocks).
pub fn parse_claude(path: &Path) -> anyhow::Result<Vec<Exchange>> {
    let content = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if ["isMeta", "isSidechain", "isCompactSummary"]
            .iter()
            .any(|f| v.get(f).and_then(|b| b.as_bool()).unwrap_or(false))
        {
            continue;
        }
        let role = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if role != "user" && role != "assistant" {
            continue;
        }
        let text = claude_text(v.pointer("/message/content").unwrap_or(&Value::Null));
        if text.trim().is_empty() {
            continue;
        }
        out.push(Exchange {
            ts: v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            role: role.to_string(),
            text,
        });
    }
    Ok(out)
}

/// Content is either a plain string (user turns) or an array of typed blocks;
/// only `text` blocks carry prose.
fn claude_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

// ---- gemini ----

/// `~/.gemini/tmp` (or under GEMINI_CLI_HOME, same env legacy honored).
fn gemini_tmp_root() -> PathBuf {
    match std::env::var("GEMINI_CLI_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h).join(".gemini"),
        _ => dirs::home_dir().unwrap_or_default().join(".gemini"),
    }
    .join("tmp")
}

/// Gemini session files have no cwd field, only a header `projectHash` we
/// can't cheaply reproduce — the marker (which embeds the unique agent name)
/// is the gate. Among marked sessions we prefer the one filed under this
/// project's own tmp dir (current gemini names it after the lowercased
/// project basename; on older hash-named dirs the preference simply never
/// fires and newest-marked wins).
fn find_gemini_session(name: &str, dir: &str) -> Option<(String, PathBuf)> {
    let marker = session_marker(name);
    let candidates: Vec<(String, PathBuf)> = files_newest_first([gemini_tmp_root()], &["json", "jsonl"])
        .into_iter()
        .filter(|p| {
            p.parent().is_some_and(|d| d.file_name().is_some_and(|n| n == "chats"))
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("session-"))
        })
        .filter_map(|p| {
            let id = gemini_marked_session_id(&p, &marker)?;
            Some((id, p))
        })
        .collect();

    let base = Path::new(dir).file_name()?.to_str()?.to_lowercase();
    let project_of = |p: &Path| p.ancestors().nth(2)?.file_name()?.to_str().map(str::to_lowercase);
    candidates
        .iter()
        .find(|(_, p)| project_of(p) == Some(base.clone()))
        .or_else(|| candidates.first())
        .cloned()
}

/// One streaming pass: resume id from the line-1 header (`sessionId` — the
/// filename only carries an 8-char prefix of it), then Some only if the
/// marker sits inside a `type:"user"` entry (hook context is recorded as user
/// content — verified on a real session file). Weaker than claude's
/// attachment check (gemini has no structural hook entries), but the marker
/// embeds the agent's unique name, so a collision needs another session
/// quoting that exact bootstrap line verbatim.
fn gemini_marked_session_id(path: &Path, marker: &str) -> Option<String> {
    let mut lines = lines_of(path)?;
    let header: Value = serde_json::from_str(&lines.next()?).ok()?;
    let id = header.get("sessionId")?.as_str()?.to_string();
    lines
        .any(|line| {
            line.contains(marker)
                && serde_json::from_str::<Value>(&line).is_ok_and(|v| {
                    v["type"] == "user" && gemini_text(v.get("content").unwrap_or(&Value::Null)).contains(marker)
                })
        })
        .then_some(id)
}

/// Gemini content blocks are `{text: ...}` with no `type` field.
fn gemini_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Parse a gemini chats `.jsonl` (or legacy `.json` — same entries, one
/// document) into exchanges. Header, `info`/`error` entries and `$set`
/// update lines are skipped.
pub fn parse_gemini(path: &Path) -> anyhow::Result<Vec<Exchange>> {
    let content = std::fs::read_to_string(path)?;
    // Older gemini wrote one JSON document with a `messages` array; current
    // writes JSONL. Normalize to an entry iterator over Values.
    let entries: Vec<Value> = match serde_json::from_str::<Value>(&content) {
        Ok(doc) if doc.get("messages").is_some() => {
            doc["messages"].as_array().cloned().unwrap_or_default()
        }
        _ => content.lines().filter_map(|l| serde_json::from_str(l).ok()).collect(),
    };
    let mut out = Vec::new();
    for v in entries {
        let role = match v.get("type").and_then(|t| t.as_str()) {
            Some("user") => "user",
            Some("gemini") | Some("model") => "assistant",
            _ => continue,
        };
        let text = gemini_text(v.get("content").unwrap_or(&Value::Null));
        if text.trim().is_empty() {
            continue;
        }
        out.push(Exchange {
            ts: v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            role: role.to_string(),
            text,
        });
    }
    Ok(out)
}

// ---- codex ----

fn codex_sessions_root() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".codex"))
        .join("sessions")
}

/// Rollouts carry `cwd` and the resume id in their `session_meta` line —
/// structural, like claude's `cwd` match. The marker gate is a substring scan
/// (codex records injected hook context as user input, no attachment
/// structure to pin it to); unverified against a live codex install — see
/// plan B4 notes.
fn find_codex_session(name: &str, dir: &str) -> Option<(String, PathBuf)> {
    let marker = session_marker(name);
    for path in files_newest_first([codex_sessions_root()], &["jsonl"]) {
        let Some(mut lines) = lines_of(&path) else { continue };
        // session_meta is line 1 in practice; scan a few in case of prefixes.
        // A cwd mismatch drops the file after these few lines — the bulk of
        // foreign rollouts is never read.
        let Some(meta) = lines.by_ref().take(5).find_map(|l| {
            let v: Value = serde_json::from_str(&l).ok()?;
            (v["type"] == "session_meta").then(|| v["payload"].clone())
        }) else {
            continue;
        };
        if meta.get("cwd").and_then(|c| c.as_str()) != Some(dir) {
            continue;
        }
        // Hook injections always land after session_meta, so scanning the
        // remainder of the iterator can't miss the marker.
        if lines.any(|l| l.contains(&marker)) {
            let id = meta.get("id")?.as_str()?.to_string();
            return Some((id, path));
        }
    }
    None
}

/// Parse a codex rollout `.jsonl`. Handles both line formats — `response_item`
/// (`payload.type == "message"`, typed content blocks) and `event_msg`
/// (`user_message`/`agent_message`, plain `message` string). Newer rollouts
/// contain BOTH for the same turn, so adjacent same-role duplicates collapse.
pub fn parse_codex(path: &Path) -> anyhow::Result<Vec<Exchange>> {
    let content = std::fs::read_to_string(path)?;
    let mut out: Vec<Exchange> = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let payload = v.get("payload").unwrap_or(&v);
        let (role, text) = match payload.get("type").and_then(|t| t.as_str()) {
            Some("message") => {
                let role = match payload.get("role").and_then(|r| r.as_str()) {
                    Some(r @ ("user" | "assistant")) => r,
                    _ => continue,
                };
                (role, codex_text(payload))
            }
            Some("user_message") => ("user", codex_text(payload)),
            Some("agent_message") => ("assistant", codex_text(payload)),
            _ => continue,
        };
        let trimmed = text.trim();
        // codex front-loads environment/instruction blobs as fake user turns
        if trimmed.is_empty()
            || trimmed.starts_with("<environment_context>")
            || trimmed.starts_with("<permissions")
            || trimmed.starts_with("<user_instructions>")
            || trimmed.starts_with("# AGENTS.md")
        {
            continue;
        }
        if out.last().is_some_and(|e| e.role == role && e.text.trim() == trimmed) {
            continue; // response_item/event_msg double-logging of one turn
        }
        out.push(Exchange {
            ts: v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            role: role.to_string(),
            text,
        });
    }
    Ok(out)
}

/// event_msg carries a plain `message` string; response_item carries typed
/// blocks (`input_text`/`output_text`) — both end in a `text` field.
fn codex_text(payload: &Value) -> String {
    if let Some(s) = payload.get("message").and_then(|m| m.as_str()) {
        return s.to_string();
    }
    match payload.get("content") {
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

// ---- omp (oh-my-pi; pi-family jsonl) ----

/// omp keeps sessions at `<agent dir>/sessions/<cwd-slug>/<ts>_<uuid>.jsonl`
/// where the agent dir is `$PI_CODING_AGENT_DIR` or `~/.omp/agent` (same
/// resolution as hook.rs's extensions dir). We never reproduce the cwd slug:
/// a `{"type":"session","id":…,"cwd":…}` header line carries both the resume
/// id and the real cwd (verified against live omp 16.3.8 sessions,
/// 2026-07-08). pi shares the format but is uninstalled/unverified here —
/// resume keeps bailing for it (HC13-resume gate; its verb also differs:
/// upstream pi takes `--session`, omp renamed it `--resume`).
fn omp_sessions_root() -> PathBuf {
    if let Ok(d) = std::env::var("PI_CODING_AGENT_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d).join("sessions");
        }
    }
    dirs::home_dir().unwrap_or_default().join(".omp").join("agent").join("sessions")
}

/// Marker gate: the kore extension's hidden bootstrap is recorded as a
/// `custom_message` entry with `customType == "kore-bootstrap"` — a
/// conversation can't fabricate that entry shape (same class as claude's
/// hook-attachment gate), so a session merely QUOTING the marker never counts.
fn find_omp_session(name: &str, dir: &str) -> Option<(String, PathBuf)> {
    let marker = session_marker(name);
    for path in files_newest_first([omp_sessions_root()], &["jsonl"]) {
        let Some(mut lines) = lines_of(&path) else { continue };
        // The session header sits in the first lines (after a `title` entry).
        let Some(header) = lines.by_ref().take(5).find_map(|l| {
            let v: Value = serde_json::from_str(&l).ok()?;
            (v["type"] == "session").then_some(v)
        }) else {
            continue;
        };
        if header.get("cwd").and_then(|c| c.as_str()) != Some(dir) {
            continue;
        }
        if lines.any(|l| {
            l.contains(&marker)
                && serde_json::from_str::<Value>(&l).is_ok_and(|v| {
                    v["type"] == "custom_message"
                        && v["customType"] == "kore-bootstrap"
                        && v["content"].as_str().is_some_and(|c| c.contains(&marker))
                })
        }) {
            let id = header.get("id")?.as_str()?.to_string();
            return Some((id, path));
        }
    }
    None
}

/// Parse an omp session `.jsonl`: `type == "message"` entries carry
/// `message.{role, content}`; content is a block list (`text`/`thinking`) or a
/// plain string. Only user/assistant text blocks survive (toolResult skipped).
pub fn parse_omp(path: &Path) -> anyhow::Result<Vec<Exchange>> {
    let content = std::fs::read_to_string(path)?;
    let mut out: Vec<Exchange> = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["type"] != "message" {
            continue;
        }
        let m = &v["message"];
        let role = match m.get("role").and_then(|r| r.as_str()) {
            Some(r @ ("user" | "assistant")) => r,
            _ => continue,
        };
        let text = match m.get("content") {
            Some(Value::Array(blocks)) => blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        if text.trim().is_empty() {
            continue;
        }
        out.push(Exchange {
            ts: v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            role: role.to_string(),
            text,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marker must match what hook.rs actually injects at SessionStart —
    /// if the bootstrap wording changes, resume's session discovery breaks.
    #[test]
    fn marker_matches_hook_bootstrap() {
        assert!(include_str!("hook.rs").contains("[kore] You are agent '{name}'"));
    }

    #[test]
    fn finds_and_parses_marked_session() {
        let base = std::env::temp_dir().join(format!("kore-transcript-test-{}", std::process::id()));
        let proj = base.join("projects/-tmp-work");
        std::fs::create_dir_all(&proj).unwrap();
        let session = "0b7a3c1e-1111-2222-3333-444455556666";
        let jsonl = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            // genuine injection shape: attachment entry from the SessionStart hook
            serde_json::json!({"type":"attachment","cwd":"/tmp/work","attachment":{
                "type":"hook_success","hookEvent":"SessionStart",
                "stdout":format!("{{\"hookSpecificOutput\":{{\"additionalContext\":\"{}\"}}}}", session_marker("luna"))}}),
            serde_json::json!({"type":"user","cwd":"/tmp/work","timestamp":"t1",
                "message":{"content":"hello"}}),
            serde_json::json!({"type":"assistant","cwd":"/tmp/work","timestamp":"t2",
                "message":{"content":[{"type":"tool_use","id":"x"},{"type":"text","text":"done"}]}}),
            // an agent DISCUSSING the marker must not count as being that agent
            serde_json::json!({"type":"user","cwd":"/tmp/work","timestamp":"t3",
                "message":{"content":format!("look: {} and hookEvent stuff", session_marker("nova"))}}),
            // meta entries (queued-command echoes etc) must not become exchanges
            serde_json::json!({"type":"user","cwd":"/tmp/work","timestamp":"t4","isMeta":true,
                "message":{"content":"noise"}}),
        );
        std::fs::write(proj.join(format!("{session}.jsonl")), jsonl).unwrap();

        // temp-env: this test is the only user of CLAUDE_CONFIG_DIR
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };

        let (id, path) = find_session("claude", "luna", "/tmp/work").expect("session found");
        assert_eq!(id, session);
        assert_eq!(cwd_of(&path).as_deref(), Some("/tmp/work"));
        assert!(find_session("claude", "nova", "/tmp/work").is_none(), "marker is per-agent");
        assert!(find_session("claude", "luna", "/elsewhere").is_none(), "cwd must match");

        let ex = parse("claude", &path).unwrap();
        assert_eq!(ex.len(), 3, "hello / done / discussion — attachment + isMeta lines skipped");
        assert_eq!((ex[1].role.as_str(), ex[1].text.as_str()), ("assistant", "done"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn finds_and_parses_gemini_session() {
        let base = std::env::temp_dir().join(format!("kore-gemini-test-{}", std::process::id()));
        // two projects: session in "other" is newer, but dir preference must win
        let mine = base.join(".gemini/tmp/myproj/chats");
        let other = base.join(".gemini/tmp/other/chats");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&other).unwrap();

        // shapes copied from a real ~/.gemini/tmp session file
        let file_for = |sid: &str, name: &str| {
            format!(
                "{}\n{}\n{}\n{}\n{}\n",
                serde_json::json!({"sessionId":sid,"projectHash":"abc","startTime":"s","kind":"main"}),
                serde_json::json!({"id":"i1","timestamp":"t0","type":"info","content":"update failed"}),
                serde_json::json!({"$set":{"lastUpdated":"t0"}}),
                serde_json::json!({"id":"i2","timestamp":"t1","type":"user",
                    "content":[{"text":format!("{} on the kore network.", session_marker(name))}]}),
                serde_json::json!({"id":"i3","timestamp":"t2","type":"gemini","content":"hi, boss"}),
            )
        };
        let sid_mine = "63eab157-0000-0000-0000-000000000001";
        let sid_other = "63eab157-0000-0000-0000-000000000002";
        std::fs::write(mine.join("session-2026-07-05T10-00-63eab157.jsonl"), file_for(sid_mine, "luna")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20)); // distinct mtimes — "other" must sort newer
        std::fs::write(other.join("session-2026-07-05T11-00-63eab157.jsonl"), file_for(sid_other, "luna")).unwrap();

        unsafe { std::env::set_var("GEMINI_CLI_HOME", &base) };

        // MyProj basename matches tmp dir "myproj" case-insensitively
        let (id, path) = find_session("gemini", "luna", "/tmp/MyProj").expect("session found");
        assert_eq!(id, sid_mine, "project-dir match beats newer session elsewhere");
        assert!(find_session("gemini", "nova", "/tmp/MyProj").is_none(), "marker is per-agent");
        // no dir match → newest marked session wins
        let (id, _) = find_session("gemini", "luna", "/somewhere/else").unwrap();
        assert_eq!(id, sid_other);

        let ex = parse("gemini", &path).unwrap();
        assert_eq!(ex.len(), 2, "info/$set/header skipped");
        assert_eq!((ex[1].role.as_str(), ex[1].text.as_str()), ("assistant", "hi, boss"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn finds_and_parses_codex_session() {
        let base = std::env::temp_dir().join(format!("kore-codex-test-{}", std::process::id()));
        let day = base.join("sessions/2026/07/05");
        std::fs::create_dir_all(&day).unwrap();
        let sid = "6f9a2b3c-1111-2222-3333-444455556666";
        let jsonl = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n",
            serde_json::json!({"timestamp":"t0","type":"session_meta",
                "payload":{"id":sid,"cwd":"/tmp/work","originator":"codex_cli_rs"}}),
            // hook bootstrap recorded as injected user input
            serde_json::json!({"timestamp":"t1","type":"response_item","payload":{
                "type":"message","role":"user",
                "content":[{"type":"input_text","text":format!("{} on the kore network.", session_marker("luna"))}]}}),
            serde_json::json!({"timestamp":"t2","type":"event_msg",
                "payload":{"type":"user_message","message":"fix the bug"}}),
            serde_json::json!({"timestamp":"t2","type":"response_item","payload":{
                "type":"message","role":"user","content":[{"type":"input_text","text":"fix the bug"}]}}),
            serde_json::json!({"timestamp":"t3","type":"response_item","payload":{
                "type":"message","role":"assistant","content":[{"type":"output_text","text":"fixed"}]}}),
            serde_json::json!({"timestamp":"t4","type":"response_item","payload":{
                "type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>stuff"}]}}),
        );
        std::fs::write(day.join(format!("rollout-2026-07-05T10-00-00-{sid}.jsonl")), jsonl).unwrap();

        unsafe { std::env::set_var("CODEX_HOME", &base) };

        let (id, path) = find_session("codex", "luna", "/tmp/work").expect("session found");
        assert_eq!(id, sid, "resume id comes from session_meta, not the filename");
        assert!(find_session("codex", "nova", "/tmp/work").is_none(), "marker is per-agent");
        assert!(find_session("codex", "luna", "/elsewhere").is_none(), "cwd must match");

        let ex = parse("codex", &path).unwrap();
        // marker turn + "fix the bug" (dupe collapsed) + "fixed"; env blob skipped
        assert_eq!(ex.len(), 3);
        assert_eq!((ex[1].role.as_str(), ex[1].text.as_str()), ("user", "fix the bug"));
        assert_eq!((ex[2].role.as_str(), ex[2].text.as_str()), ("assistant", "fixed"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn finds_and_parses_omp_session() {
        let base = std::env::temp_dir().join(format!("kore-omp-test-{}", std::process::id()));
        let dir = base.join("sessions/-tmp-work");
        std::fs::create_dir_all(&dir).unwrap();
        let sid = "019f3ec9-7cd7-7000-adf7-8aeee9ba0c4d";
        // shapes copied from a real omp 16.3.8 session file (2026-07-08)
        let jsonl = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            serde_json::json!({"type":"title","v":1,"title":""}),
            serde_json::json!({"type":"session","version":3,"id":sid,"timestamp":"t0","cwd":"/tmp/work"}),
            serde_json::json!({"type":"message","id":"m1","timestamp":"t1",
                "message":{"role":"user","content":[{"type":"text","text":"hello"}]}}),
            // the extension's hidden bootstrap — the identity gate
            serde_json::json!({"type":"custom_message","customType":"kore-bootstrap","display":false,
                "content":format!("{} on the kore network.", session_marker("luna"))}),
            serde_json::json!({"type":"message","id":"m2","timestamp":"t2",
                "message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"done"}]}}),
            serde_json::json!({"type":"message","id":"m3","timestamp":"t3",
                "message":{"role":"toolResult","content":[{"type":"text","text":"tool noise"}]}}),
            // a session QUOTING the marker in a plain message must not count
            serde_json::json!({"type":"message","id":"m4","timestamp":"t4",
                "message":{"role":"user","content":[{"type":"text","text":format!("look: {}", session_marker("nova"))}]}}),
        );
        std::fs::write(dir.join(format!("2026-07-08T00-00-00-000Z_{sid}.jsonl")), jsonl).unwrap();

        unsafe { std::env::set_var("PI_CODING_AGENT_DIR", &base) };

        let (id, path) = find_session("omp", "luna", "/tmp/work").expect("session found");
        assert_eq!(id, sid, "resume id comes from the session header line");
        assert!(find_session("omp", "nova", "/tmp/work").is_none(), "quoted marker must not count");
        assert!(find_session("omp", "luna", "/elsewhere").is_none(), "cwd must match");

        let ex = parse("omp", &path).unwrap();
        // hello / done (thinking block dropped) / quoted-marker msg; toolResult skipped
        assert_eq!(ex.len(), 3);
        assert_eq!((ex[0].role.as_str(), ex[0].text.as_str()), ("user", "hello"));
        assert_eq!((ex[1].role.as_str(), ex[1].text.as_str()), ("assistant", "done"));

        let _ = std::fs::remove_dir_all(&base);
    }
}
