//! HTTP load generator for benchmarking the openplace backends (Rust vs
//! legacy Node.js) under identical conditions.
//!
//! Examples:
//!   loadgen --url http://127.0.0.1:3000 --scenario health --conns 64 --duration 30
//!   loadgen --url http://127.0.0.1:3000 --scenario tile --tile 0,0 --conns 64 --duration 30
//!   loadgen --url http://127.0.0.1:3000 --scenario paint --paint 25 --logins 50 --conns 50 --duration 30
//!   loadgen --url http://127.0.0.1:3000 --scenario me --logins 50 --conns 50 --duration 30
//!   loadgen --url http://127.0.0.1:3000 --scenario leaderboard --conns 64 --duration 30
//!   loadgen --url http://127.0.0.1:3000 --scenario mixed --conns 64 --duration 30

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use rand::Rng;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    url: String,
    #[arg(long, default_value = "health")]
    scenario: String,
    #[arg(long, default_value = "16")]
    conns: usize,
    #[arg(long, default_value = "30")]
    duration: u64,
    /// How many bench users to register/login (paint, me scenarios).
    #[arg(long, default_value = "0")]
    logins: usize,
    /// Pixels per paint request.
    #[arg(long, default_value = "25")]
    paint: usize,
    /// Tile to hammer for the tile/pixel-info scenarios.
    #[arg(long, default_value = "0,0")]
    tile: String,
    /// warm = reuse keep-alive connections (default), cold = new conn per req.
    #[arg(long, default_value = "warm")]
    connections: String,
}

#[derive(Default)]
struct Stats {
    ok: AtomicU64,
    err: AtomicU64,
}

impl Stats {
    fn record(&self, ok: bool) {
        if ok {
            self.ok.fetch_add(1, Ordering::Relaxed);
        } else {
            self.err.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn percentile(v: &[u64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<u64> = v.to_vec();
    sorted.sort_unstable();
    let idx = ((sorted.len() as f64) * p).min((sorted.len() - 1) as f64) as usize;
    sorted[idx] as f64 / 1000.0 // µs → ms
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let base = args.url.trim_end_matches('/').to_string();
    let scenario = args.scenario.clone();

    // Auth phase: register/login bench users, collect cookies.
    let mut cookies: Vec<String> = Vec::new();
    if args.logins > 0 {
        let client = reqwest::Client::new();
        for i in 0..args.logins {
            let name = format!("bench{}", i + 1);
            let _ = client
                .post(format!("{base}/register"))
                .header("content-type", "application/json")
                .body(format!(r#"{{"username":"{name}","password":"benchpass"}}"#))
                .send()
                .await;
            let login = client
                .post(format!("{base}/login"))
                .header("content-type", "application/json")
                .body(format!(r#"{{"username":"{name}","password":"benchpass"}}"#))
                .send()
                .await
                .expect("login request");
            let cookie = login
                .headers()
                .get("set-cookie")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.split(';').next().unwrap_or("").to_string())
                .unwrap_or_default();
            if cookie.is_empty() {
                let status = login.status();
                let body = login.text().await.unwrap_or_default();
                eprintln!("login failed for {name}: {status} {body}");
                std::process::exit(1);
            }
            cookies.push(cookie);
        }
        println!("[loadgen] logged in {} users", cookies.len());
    }

    let tile = args
        .tile
        .split(',')
        .filter_map(|v| v.parse::<i32>().ok())
        .collect::<Vec<_>>();
    let (tile_x, tile_y) = (
        tile.first().copied().unwrap_or(0),
        tile.get(1).copied().unwrap_or(0),
    );

    let stats = Arc::new(Stats::default());
    let debug_first = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Pre-generated paint payloads (deterministic 25-px batches by default).
    let mut rng = rand::thread_rng();
    let paint_payloads: Vec<String> = (0..64)
        .map(|_| {
            let n = args.paint.max(1);
            let mut colors = Vec::with_capacity(n);
            let mut coords = Vec::with_capacity(n * 2);
            for _ in 0..n {
                colors.push(rng.gen_range(1..32u32));
                coords.push(rng.gen_range(0..1000u32));
                coords.push(rng.gen_range(0..1000u32));
            }
            let colors: Vec<String> = colors.iter().map(|c| c.to_string()).collect();
            let coords: Vec<String> = coords.iter().map(|c| c.to_string()).collect();
            format!(
                r#"{{"colors":[{}],"coords":[{}]}}"#,
                colors.join(","),
                coords.join(",")
            )
        })
        .collect();

    println!(
        "[loadgen] scenario={} conns={} duration={}s url={}",
        scenario, args.conns, args.duration, base
    );
    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let conns_total = args.conns;
    let mut handles = Vec::new();
    for w in 0..args.conns {
        let stats = stats.clone();
        let base = base.clone();
        let scenario = scenario.clone();
        let cookies = cookies.clone();
        let payloads = paint_payloads.clone();
        let keepalive = args.connections != "cold";
        let debug_first = debug_first.clone();
        handles.push(tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .pool_max_idle_per_host(if keepalive { conns_total.max(8) } else { 0 })
                .build()
                .unwrap();
            let mut latencies: Vec<u64> = Vec::with_capacity(4096);
            let pick = |len: usize| -> usize {
                let mut rng = rand::thread_rng();
                rng.gen_range(0..len.max(1))
            };
            while Instant::now() < deadline {
                let cookie = if cookies.is_empty() {
                    None
                } else {
                    cookies.get(w % cookies.len()).cloned()
                };
                let (method, url, body) = match scenario.as_str() {
                    "health" => ("GET", format!("{base}/health"), None),
                    "tile" => (
                        "GET",
                        format!("{base}/files/s0/tiles/{tile_x}/{tile_y}.png"),
                        None,
                    ),
                    "pixel-info" => (
                        "GET",
                        format!("{base}/s0/pixel/{tile_x}/{tile_y}?x=100&y=100"),
                        None,
                    ),
                    "leaderboard" => ("GET", format!("{base}/leaderboard/player/all-time"), None),
                    "leaderboard-region" => (
                        "GET",
                        format!("{base}/leaderboard/region/players/1/all-time"),
                        None,
                    ),
                    "me" => ("GET", format!("{base}/me"), None),
                    "paint" => (
                        "POST",
                        format!("{base}/s0/pixel/{tile_x}/{tile_y}"),
                        Some(payloads[pick(payloads.len())].clone()),
                    ),
                    "mixed" => match pick(10) {
                        0 | 1 => ("GET", format!("{base}/leaderboard/player/all-time"), None),
                        2 => (
                            "GET",
                            format!("{base}/s0/pixel/{tile_x}/{tile_y}?x=100&y=100"),
                            None,
                        ),
                        _ => (
                            "GET",
                            format!("{base}/files/s0/tiles/{tile_x}/{tile_y}.png"),
                            None,
                        ),
                    },
                    other => {
                        eprintln!("unknown scenario {other}");
                        std::process::exit(1);
                    }
                };
                let mut req = match method {
                    "GET" => client.get(&url),
                    _ => client.post(&url),
                };
                if let Some(c) = &cookie {
                    req = req.header("cookie", c);
                }
                if let Some(b) = &body {
                    // text/plain, like the wplace frontend.
                    req = req.header("content-type", "text/plain").body(b.clone());
                }
                let start = Instant::now();
                let ok = match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let bytes = resp.bytes().await.unwrap_or_default();
                        let good = status.is_success() || status.as_u16() == 304;
                        if !good && !debug_first.swap(true, Ordering::Relaxed) {
                            eprintln!(
                                "[loadgen] first failure: {} {} -> {} {}",
                                method,
                                url,
                                status,
                                String::from_utf8_lossy(&bytes)
                                    .chars()
                                    .take(200)
                                    .collect::<String>()
                            );
                        }
                        good
                    }
                    Err(e) => {
                        if !debug_first.swap(true, Ordering::Relaxed) {
                            eprintln!("[loadgen] first transport error: {e}");
                        }
                        false
                    }
                };
                latencies.push(start.elapsed().as_micros() as u64);
                stats.record(ok);
            }
            latencies
        }));
    }

    let mut all_latencies: Vec<u64> = Vec::new();
    for h in handles {
        if let Ok(l) = h.await {
            all_latencies.extend(l);
        }
    }

    let total_ok = stats.ok.load(Ordering::Relaxed);
    let total_err = stats.err.load(Ordering::Relaxed);
    let mean = if all_latencies.is_empty() {
        0.0
    } else {
        all_latencies.iter().sum::<u64>() as f64 / all_latencies.len() as f64 / 1000.0
    };
    println!(
        "{{\"scenario\":\"{}\",\"conns\":{},\"duration\":{},\"ok\":{},\"errors\":{},\"rps\":{:.1},\"avg_ms\":{:.2},\"p50_ms\":{:.2},\"p95_ms\":{:.2},\"p99_ms\":{:.2}}}",
        scenario,
        args.conns,
        args.duration,
        total_ok,
        total_err,
        total_ok as f64 / args.duration as f64,
        mean,
        percentile(&all_latencies, 0.5),
        percentile(&all_latencies, 0.95),
        percentile(&all_latencies, 0.99)
    );
}
