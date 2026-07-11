use std::io::{IsTerminal, Read as IoRead};

use kore_protocol::api::{SendRequest, SendResponse};

use crate::config;

#[derive(clap::Parser, Debug)]
#[command(about = "Send a message to agents")]
pub struct SendArgs {
    /// Positional args: @targets and/or bare message text (backward compat)
    pub positionals: Vec<String>,

    /// Message text (after --)
    #[arg(last = true)]
    pub message: Vec<String>,

    /// Read message from stdin
    #[arg(long)]
    pub stdin: bool,

    /// Read message from file
    #[arg(long)]
    pub file: Option<String>,

    /// Read message from base64-encoded string
    #[arg(long)]
    pub base64: Option<String>,

    /// Message intent (request|inform|ack)
    #[arg(long)]
    pub intent: Option<String>,

    /// Thread name to attach this message to
    #[arg(long)]
    pub thread: Option<String>,

    /// Server id of the message this replies to (bare `42` or `#42`)
    #[arg(long, value_parser = parse_msg_id)]
    pub reply_to: Option<i64>,

    /// Attach a bundle by id (receiver runs `bundle cat <id>` to load it)
    #[arg(long)]
    pub bundle: Option<String>,

    /// Confirm a large broadcast (agents sending to the whole project are
    /// asked to re-run with --go; humans never need it)
    #[arg(long)]
    pub go: bool,

    /// Block until a reply (reply_to = this message) arrives, then print it
    #[arg(long)]
    pub wait: bool,

    /// Seconds to wait for the reply (with --wait)
    #[arg(long, default_value_t = 60)]
    pub timeout: u64,
}

/// Message ids display as `#42`; accept that form back on the CLI.
fn parse_msg_id(s: &str) -> Result<i64, String> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("'{s}' is not a message id (use 42 or #42)"))
}

/// Separate `@target` positionals from bare message words.
/// Mirrors legacy `process_positionals`: a single arg with `@` + a space is
/// backward-compat for "whole text with inline @mentions".
fn process_positionals(positionals: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    if positionals.len() == 1 && positionals[0].starts_with('@') && positionals[0].contains(' ') {
        return Ok((vec![], vec![positionals[0].clone()]));
    }

    let mut targets = Vec::new();
    let mut remaining = Vec::new();
    for arg in positionals {
        if let Some(name) = arg.strip_prefix('@') {
            if name.trim().is_empty() {
                return Err("Empty target '@' is not allowed".to_string());
            }
            targets.push(name.to_string());
        } else {
            remaining.push(arg.clone());
        }
    }
    Ok((targets, remaining))
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_ok() && !buf.is_empty() {
        Ok(buf)
    } else {
        Err("No input received on stdin".to_string())
    }
}

fn resolve_message_text(
    args: &SendArgs,
    bare_words: Vec<String>,
    has_separator: bool,
) -> Result<String, String> {
    let source_count = [has_separator, args.stdin, args.file.is_some(), args.base64.is_some()]
        .iter()
        .filter(|&&x| x)
        .count();
    if source_count > 1 {
        return Err("Only one of --, --stdin, --file, --base64 can be used".to_string());
    }

    if has_separator {
        // `send luna -- text`: dropping bare words would silently broadcast.
        if !bare_words.is_empty() {
            return Err(format!(
                "Unexpected argument(s) before '--': {}. Targets must start with '@' (did you mean @{}?)",
                bare_words.join(" "),
                bare_words[0],
            ));
        }
        let text = args.message.join(" ");
        return if text.trim().is_empty() {
            Err("No message after --".to_string())
        } else {
            Ok(text)
        };
    }

    if args.stdin {
        return read_stdin();
    }

    if let Some(ref path) = args.file {
        return std::fs::read_to_string(path).map_err(|e| format!("Cannot read file: {e}"));
    }

    if let Some(ref b64) = args.base64 {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| "Invalid base64 encoding".to_string())?;
        return String::from_utf8(bytes).map_err(|_| "Base64 decoded to invalid UTF-8".to_string());
    }

    match bare_words.len() {
        1 => return Ok(bare_words.into_iter().next().unwrap()),
        n if n > 1 => {
            return Err(
                "Multiple bare words — put your message after '--':\n  kore-client send @target -- your message"
                    .to_string(),
            );
        }
        _ => {}
    }

    if !std::io::stdin().is_terminal() {
        return read_stdin();
    }

    Err("No message provided.\nUse: kore-client send @target -- your message".to_string())
}

pub async fn run(args: SendArgs) -> anyhow::Result<()> {
    let (targets, bare_words) = process_positionals(&args.positionals).map_err(|e| anyhow::anyhow!(e))?;
    // clap's `last = true` can't distinguish `--` with nothing after it from no
    // `--` at all, so check the raw argv (the first `--` is always the separator).
    let has_separator = std::env::args().any(|a| a == "--");
    let text = resolve_message_text(&args, bare_words, has_separator).map_err(|e| anyhow::anyhow!(e))?;

    // For --wait, open the peek socket *before* sending so a fast reply can't
    // slip through the gap. Peek never consumes the instance's real inbox.
    let mut wait_ws = if args.wait {
        Some(crate::ws::connect(true).await?)
    } else {
        None
    };

    let token = config::load_token()?;
    let resp = config::http_client()
        .post(format!("{}/v1/messages", config::server_url()))
        .bearer_auth(token)
        .json(&SendRequest {
            targets,
            text,
            intent: args.intent.map(|i| i.to_lowercase()),
            thread: args.thread,
            reply_to: args.reply_to,
            bundle_id: args.bundle,
            go: args.go,
        })
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("send failed ({}): {}", resp.status(), resp.text().await?);
    }

    let body: SendResponse = resp.json().await?;
    println!(
        "sent #{} [{}]{}",
        body.id,
        body.scope.as_str(),
        if body.mentions.is_empty() {
            String::new()
        } else {
            format!(" → {}", body.mentions.join(", "))
        }
    );

    if let Some(ws) = wait_ws.as_mut() {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(args.timeout), async {
            while let Some(d) = crate::ws::next_delivery(ws).await? {
                if d.message.reply_to == Some(body.id) {
                    return Ok::<_, anyhow::Error>(Some(d));
                }
            }
            Ok(None)
        })
        .await;

        match reply {
            Ok(Ok(Some(d))) => println!("reply #{} from {}: {}", d.id, d.message.from, d.message.text),
            Ok(Ok(None)) => anyhow::bail!("connection closed before a reply arrived"),
            Ok(Err(e)) => return Err(e),
            Err(_) => anyhow::bail!(
                "no reply to #{} within {}s — recipient may answer later (kore-client send --reply-to {})",
                body.id, args.timeout, body.id
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_to_accepts_display_form() {
        assert_eq!(parse_msg_id("42"), Ok(42));
        assert_eq!(parse_msg_id("#42"), Ok(42));
        assert!(parse_msg_id("abc").is_err());
    }

    fn sv(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn positionals_split_targets_from_words() {
        let (t, w) = process_positionals(&sv(&["@luna", "@nova", "stray"])).unwrap();
        assert_eq!(t, sv(&["luna", "nova"]));
        assert_eq!(w, sv(&["stray"]));
    }

    #[test]
    fn single_arg_with_inline_mentions_is_whole_message() {
        // Backward compat: `send "@luna can you look at this"` (one quoted arg).
        let (t, w) = process_positionals(&sv(&["@luna can you look at this"])).unwrap();
        assert!(t.is_empty());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn empty_target_rejected() {
        assert!(process_positionals(&sv(&["@"])).is_err());
    }

    fn args(positionals: &[&str], message: &[&str]) -> SendArgs {
        SendArgs {
            positionals: sv(positionals),
            message: sv(message),
            stdin: false,
            file: None,
            base64: None,
            intent: None,
            thread: None,
            reply_to: None,
            bundle: None,
            go: false,
            wait: false,
            timeout: 60,
        }
    }

    #[test]
    fn separator_text_wins_and_bare_words_before_it_error() {
        let a = args(&[], &["hello", "world"]);
        assert_eq!(resolve_message_text(&a, vec![], true).unwrap(), "hello world");
        // `send luna -- hi`: silently dropping "luna" would broadcast.
        let err = resolve_message_text(&a, sv(&["luna"]), true).unwrap_err();
        assert!(err.contains("@luna"), "must suggest the @ form: {err}");
    }

    #[test]
    fn empty_after_separator_errors() {
        assert!(resolve_message_text(&args(&[], &[]), vec![], true).is_err());
    }

    #[test]
    fn multiple_bare_words_demand_separator() {
        let err = resolve_message_text(&args(&[], &[]), sv(&["two", "words"]), false).unwrap_err();
        assert!(err.contains("--"));
    }

    #[test]
    fn multiple_sources_rejected() {
        let mut a = args(&[], &["hi"]);
        a.stdin = true;
        assert!(resolve_message_text(&a, vec![], true).is_err());
    }

    #[test]
    fn base64_roundtrip() {
        let mut a = args(&[], &[]);
        a.base64 = Some("aG9sYQ==".into()); // "hola"
        assert_eq!(resolve_message_text(&a, vec![], false).unwrap(), "hola");
        a.base64 = Some("not-base64!".into());
        assert!(resolve_message_text(&a, vec![], false).is_err());
    }
}
