//! Self-update from the public mirror's GitHub Releases (P5 pipeline).
//!
//! Latest version comes from the `/releases/latest` redirect (GitHub answers
//! with `/releases/tag/vX.Y.Z`) — no API call, no rate limit, no token. The
//! passive hint (`print_hint`) checks at most once per day via a cache file
//! and never breaks or delays the command it rides on.

use crate::config;

const REPO: &str = "Solar2004/kore-client";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Release asset for this platform — names must match .github/workflows/release.yml.
fn asset_name() -> anyhow::Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "kore-client-x86_64-linux",
        ("macos", "aarch64") => "kore-client-aarch64-macos",
        ("macos", "x86_64") => "kore-client-x86_64-macos",
        ("windows", "x86_64") => "kore-client-x86_64-windows.exe",
        (os, arch) => anyhow::bail!("no prebuilt binary for {os}-{arch} — build from source"),
    })
}

/// "v1.2.3" → (1,2,3). Non-digit suffixes per component are dropped
/// ("3-rc1" → 3); anything unparseable sorts as 0, so a garbage tag can
/// never look newer than a real version.
fn parse(v: &str) -> (u64, u64, u64) {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    });
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

async fn latest_version(timeout: std::time::Duration) -> anyhow::Result<String> {
    let resp = config::http_client()
        .get(format!("https://github.com/{REPO}/releases/latest"))
        .timeout(timeout)
        .send()
        .await?
        .error_for_status()?;
    // reqwest followed the redirect; the final URL's last segment is the tag.
    let tag = resp
        .url()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    if !tag.starts_with('v') {
        anyhow::bail!("no releases found for {REPO}");
    }
    Ok(tag)
}

/// `kore-client update`: download this platform's asset from the latest
/// release and swap it over the running binary.
pub async fn run() -> anyhow::Result<()> {
    let latest = latest_version(std::time::Duration::from_secs(10)).await?;
    if parse(&latest) <= parse(CURRENT) {
        println!("kore-client v{CURRENT} is up to date (latest: {latest})");
        return Ok(());
    }
    let asset = asset_name()?;
    let url = format!("https://github.com/{REPO}/releases/download/{latest}/{asset}");
    println!("updating v{CURRENT} → {latest} ...");
    let bytes = config::http_client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let exe = std::env::current_exe()?;
    // Write next to the exe (same filesystem) so the final rename is atomic.
    let new = exe.with_extension("new");
    std::fs::write(&new, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    }
    // Windows can't overwrite a running exe but CAN rename it away first.
    #[cfg(windows)]
    {
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)?;
    }
    std::fs::rename(&new, &exe)?;
    println!("updated to {latest} ({})", exe.display());
    Ok(())
}

/// One-line "update available" hint after human commands. At most one
/// network check per 24h (cache: `$KORE_DIR/update-check` = "unix_ts tag");
/// silent on every failure — a hint must never turn a working command into
/// a failing or slow one. Agents (KORE_NAME set) never see it: update noise
/// in a hook-driven context is at best wasted tokens.
pub async fn print_hint() {
    if config::inside_agent_context() {
        return;
    }
    let path = config::kore_dir().join("update-check");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = std::fs::read_to_string(&path).ok();
    let latest = match cached.as_deref().and_then(|s| s.trim().split_once(' ')) {
        Some((ts, v))
            if ts
                .parse::<u64>()
                .is_ok_and(|t| now.saturating_sub(t) < 86_400) =>
        {
            v.to_string()
        }
        _ => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Cache failures as "current" too — no releases yet (or offline)
            // must not cost every command a failed HTTP for the next 24h.
            let v = latest_version(std::time::Duration::from_secs(3))
                .await
                .unwrap_or_else(|_| format!("v{CURRENT}"));
            let _ = std::fs::write(&path, format!("{now} {v}"));
            v
        }
    };
    if parse(&latest) > parse(CURRENT) {
        println!("[kore] update available: v{CURRENT} → {latest} — run `kore-client update`");
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn version_parse_and_compare() {
        assert_eq!(parse("v1.2.3"), (1, 2, 3));
        assert_eq!(parse("0.1.0"), (0, 1, 0));
        assert_eq!(parse("v0.2.0-rc1"), (0, 2, 0));
        assert_eq!(parse("garbage"), (0, 0, 0));
        assert!(parse("v0.2.0") > parse("0.1.9"));
        assert!(parse("v0.10.0") > parse("v0.9.9")); // numeric, not lexicographic
        assert!(parse("garbage") <= parse(env!("CARGO_PKG_VERSION")));
    }
}
