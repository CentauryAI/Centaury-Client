//! Load test for the kore send path.
//!
//! Usage: bench <server-url> <token> <concurrency> <seconds> [message]
//! Run the server with KORE_RATE_LIMIT=0 or the limiter will (correctly) say no.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("server url");
    let token = args.next().expect("token");
    let concurrency: usize = args.next().expect("concurrency").parse()?;
    let seconds: u64 = args.next().expect("seconds").parse()?;
    let message = args.next().unwrap_or_else(|| "bench broadcast".to_string());

    let endpoint = format!("{url}/v1/messages");
    let body = serde_json::json!({ "targets": [], "text": message });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let errors = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for _ in 0..concurrency {
        let client = reqwest::Client::new();
        let endpoint = endpoint.clone();
        let token = token.clone();
        let body = body.clone();
        let errors = errors.clone();
        handles.push(tokio::spawn(async move {
            let mut latencies_us: Vec<u64> = Vec::with_capacity(8192);
            while Instant::now() < deadline {
                let t0 = Instant::now();
                let res = client
                    .post(&endpoint)
                    .bearer_auth(&token)
                    .json(&body)
                    .send()
                    .await;
                match res {
                    Ok(r) if r.status().is_success() => {
                        latencies_us.push(t0.elapsed().as_micros() as u64)
                    }
                    _ => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            latencies_us
        }));
    }

    let mut all: Vec<u64> = Vec::new();
    for h in handles {
        all.extend(h.await?);
    }
    all.sort_unstable();

    let total = all.len();
    let errs = errors.load(Ordering::Relaxed);
    let pct =
        |p: f64| all[((total as f64 * p) as usize).min(total.saturating_sub(1))] as f64 / 1000.0;
    println!(
        "requests={} errors={} rps={:.0} p50={:.1}ms p95={:.1}ms p99={:.1}ms max={:.1}ms",
        total,
        errs,
        total as f64 / seconds as f64,
        pct(0.50),
        pct(0.95),
        pct(0.99),
        pct(0.999).max(*all.last().unwrap_or(&0) as f64 / 1000.0),
    );
    Ok(())
}
