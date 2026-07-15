//! `bundle` — structured context handoff (docs/PLAN-B6-BUNDLES.md).
//!
//! Create materializes everything the refs point at INTO the bundle right
//! here, because this machine is the only place those refs resolve; the
//! server stores the blob and the refs stay provenance. Receivers on any
//! machine get the full content with `bundle cat <id>`.

use kore_protocol::bundle::{
    Bundle, BundleRefs, BundleSummary, CreateBundleRequest, CreateBundleResponse, TranscriptRange,
};

use crate::{config, transcript};

/// Client-side cap, mirrored server-side; the transport body limit is 256 KB.
const MAX_CONTENT: usize = 200 * 1024;

#[derive(clap::Subcommand, Debug)]
pub enum BundleCmd {
    /// Create a bundle from local refs (files, own-transcript ranges, message ids)
    Create {
        title: String,
        #[arg(long)]
        description: String,
        /// Comma-separated file paths to embed
        #[arg(long)]
        files: Option<String>,
        /// Exchange ranges from YOUR session, "3-14:normal,20:full"
        #[arg(long)]
        transcript: Option<String>,
        /// Message ids to embed, "5,7" or "5-9"
        #[arg(long)]
        events: Option<String>,
        /// Parent bundle id this one extends
        #[arg(long)]
        extends: Option<String>,
    },
    /// List bundles visible in your project
    List {
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Show a bundle's metadata and provenance
    Show { id: String },
    /// Print a bundle's content (pipeable)
    Cat { id: String },
}

pub async fn run(cmd: BundleCmd) -> anyhow::Result<()> {
    match cmd {
        BundleCmd::Create {
            title,
            description,
            files,
            transcript,
            events,
            extends,
        } => create(title, description, files, transcript, events, extends).await,
        BundleCmd::List { limit } => {
            let list: Vec<BundleSummary> =
                crate::get_json(&format!("/v1/bundles?limit={limit}")).await?;
            for b in list {
                println!(
                    "{:<16} {:<28} {:<12} {:>7}B  {}",
                    b.id, b.title, b.created_by, b.size, b.created_at
                );
            }
            Ok(())
        }
        BundleCmd::Show { id } => {
            let b: Bundle = crate::get_json(&format!("/v1/bundles/{id}")).await?;
            println!(
                "{}  {}\nby {} at {}\n{}",
                b.id, b.title, b.created_by, b.created_at, b.description
            );
            if let Some(parent) = &b.extends {
                println!("extends: {parent}");
            }
            println!(
                "refs: {} file(s), {} event(s), {} transcript range(s) — `bundle cat {}` for the {}B content",
                b.refs.files.len(),
                b.refs.events.len(),
                b.refs.transcript.len(),
                b.id,
                b.content.len()
            );
            Ok(())
        }
        BundleCmd::Cat { id } => {
            let b: Bundle = crate::get_json(&format!("/v1/bundles/{id}")).await?;
            println!("{}", b.content);
            Ok(())
        }
    }
}

async fn create(
    title: String,
    description: String,
    files: Option<String>,
    transcript_spec: Option<String>,
    events: Option<String>,
    extends: Option<String>,
) -> anyhow::Result<()> {
    let refs = BundleRefs {
        files: csv(&files),
        events: csv(&events),
        transcript: transcript_spec
            .as_deref()
            .map(parse_transcript_spec)
            .transpose()?
            .unwrap_or_default(),
    };
    if refs.files.is_empty() && refs.events.is_empty() && refs.transcript.is_empty() {
        anyhow::bail!("empty bundle — give at least one of --files/--transcript/--events");
    }

    let mut content = String::new();
    for path in &refs.files {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read '{path}': {e}"))?;
        content.push_str(&format!("== file {path} ==\n{text}\n"));
    }
    if !refs.events.is_empty() {
        materialize_events(&refs.events, &mut content).await?;
    }
    if !refs.transcript.is_empty() {
        materialize_transcript(&refs.transcript, &mut content)?;
    }

    if content.len() > MAX_CONTENT {
        anyhow::bail!(
            "bundle content is {} KB (max {} KB) — split it, or reference fewer/leaner files",
            content.len() / 1024,
            MAX_CONTENT / 1024
        );
    }

    let req = CreateBundleRequest {
        title,
        description,
        refs,
        content,
        extends,
    };
    let resp = config::http_client()
        .post(format!("{}/v1/bundles", config::server_url()))
        .bearer_auth(config::load_token()?)
        .json(&req)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "bundle create failed ({}): {}",
            resp.status(),
            resp.text().await?
        );
    }
    let body: CreateBundleResponse = resp.json().await?;
    println!(
        "{}  — share it: centaury send @who --bundle {} -- context ready",
        body.id, body.id
    );
    Ok(())
}

fn csv(raw: &Option<String>) -> Vec<String> {
    raw.as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// "3-14:normal,20:full" → ranges. Detail label rides along as provenance;
/// rendering is full text either way (legacy detail matrix cut, see design doc).
fn parse_transcript_spec(spec: &str) -> anyhow::Result<Vec<TranscriptRange>> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|part| {
            let (range, detail) = part.split_once(':').ok_or_else(|| {
                anyhow::anyhow!("transcript ref '{part}' needs range:detail, e.g. 3-14:normal")
            })?;
            parse_range(range)?; // fail early on garbage
            Ok(TranscriptRange {
                range: range.to_string(),
                detail: detail.to_string(),
            })
        })
        .collect()
}

/// "3-14" or "6" → inclusive 1-based bounds.
fn parse_range(s: &str) -> anyhow::Result<(usize, usize)> {
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    let lo: usize = a
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("bad range '{s}'"))?;
    let hi: usize = b
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("bad range '{s}'"))?;
    anyhow::ensure!(lo >= 1 && lo <= hi, "bad range '{s}'");
    Ok((lo, hi))
}

/// Embed the caller's OWN session exchanges — the only transcript this
/// machine is guaranteed to have.
fn materialize_transcript(ranges: &[TranscriptRange], out: &mut String) -> anyhow::Result<()> {
    let name = config::instance_name()?;
    let cwd = std::env::current_dir()?.display().to_string();
    let (_, path) = ["claude", "gemini", "codex"]
        .iter()
        .find_map(|tool| transcript::find_session(tool, &name, &cwd))
        .ok_or_else(|| anyhow::anyhow!("no session transcript for '{name}' in {cwd} — --transcript only works from inside a hooked session"))?;
    let exchanges = transcript::parse("claude", &path)
        .or_else(|_| transcript::parse("gemini", &path))
        .or_else(|_| transcript::parse("codex", &path))?;

    for r in ranges {
        let (lo, hi) = parse_range(&r.range)?;
        anyhow::ensure!(
            lo <= exchanges.len(),
            "range {} starts past the last exchange (#{})",
            r.range,
            exchanges.len()
        );
        out.push_str(&format!("== transcript {} ==\n", r.range));
        for ex in &exchanges[lo - 1..hi.min(exchanges.len())] {
            out.push_str(&format!("[{}] {}: {}\n", ex.ts, ex.role, ex.text));
        }
        out.push('\n');
    }
    Ok(())
}

/// Embed messages by server id from project history.
async fn materialize_events(ids: &[String], out: &mut String) -> anyhow::Result<()> {
    let mut wanted = std::collections::BTreeSet::new();
    for spec in ids {
        let (lo, hi) = parse_range(spec)?;
        wanted.extend(lo..=hi);
    }
    let history: Vec<kore_protocol::api::Delivery> =
        crate::get_json("/v1/messages?limit=200").await?;
    for d in &history {
        if wanted.remove(&(d.id as usize)) {
            out.push_str(&format!(
                "== event #{} ==\n{}: {}\n\n",
                d.id, d.message.from, d.message.text
            ));
        }
    }
    anyhow::ensure!(
        wanted.is_empty(),
        "message id(s) not in your project's recent history: {}",
        wanted
            .into_iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_and_spec_parsing() {
        assert_eq!(parse_range("3-14").unwrap(), (3, 14));
        assert_eq!(parse_range("6").unwrap(), (6, 6));
        assert!(parse_range("0").is_err(), "1-based");
        assert!(parse_range("9-2").is_err(), "inverted");
        assert!(parse_range("x").is_err());

        let spec = parse_transcript_spec("3-14:normal, 20:full").unwrap();
        assert_eq!(spec.len(), 2);
        assert_eq!(
            (spec[0].range.as_str(), spec[0].detail.as_str()),
            ("3-14", "normal")
        );
        assert!(
            parse_transcript_spec("3-14").is_err(),
            "detail level required"
        );

        assert_eq!(csv(&Some(" a, ,b ".into())), vec!["a", "b"]);
        assert!(csv(&None).is_empty());
    }
}
