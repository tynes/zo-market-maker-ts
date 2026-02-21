//! Synthetic microbenchmark: JSON parse + mid price computation
//! Replays a captured bookTicker message 1M times

use std::time::Instant;

use serde::Deserialize;

#[derive(Deserialize)]
struct CombinedStreamMsg {
    data: BookTickerMsg,
}

#[derive(Deserialize)]
struct BookTickerMsg {
    b: String,
    a: String,
}

const ITERATIONS: u64 = 1_000_000;

fn main() {
    let sample = r#"{"stream":"btcusdt@bookTicker","data":{"e":"bookTicker","E":1700000000000,"T":1700000000000,"s":"BTCUSDT","b":"43567.80","B":"1.234","a":"43567.90","A":"2.345"}}"#;

    // Warm up
    for _ in 0..10_000 {
        let msg: CombinedStreamMsg = serde_json::from_str(sample).unwrap();
        let bid: f64 = msg.data.b.parse().unwrap();
        let ask: f64 = msg.data.a.parse().unwrap();
        let _mid = (bid + ask) * 0.5;
        std::hint::black_box(_mid);
    }

    let t0 = Instant::now();

    let mut sum: f64 = 0.0;
    for _ in 0..ITERATIONS {
        let msg: CombinedStreamMsg = serde_json::from_str(sample).unwrap();
        let bid: f64 = msg.data.b.parse().unwrap();
        let ask: f64 = msg.data.a.parse().unwrap();
        let mid = (bid + ask) * 0.5;
        sum += mid;
    }

    // Prevent DCE
    std::hint::black_box(sum);

    let elapsed = t0.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let per_iter_ns = elapsed.as_nanos() as f64 / ITERATIONS as f64;
    let throughput = ITERATIONS as f64 / elapsed.as_secs_f64();

    println!("Rust JSON parse benchmark");
    println!("  Iterations: {}", format_with_commas(ITERATIONS));
    println!("  Total time: {:.1} ms", elapsed_ms);
    println!("  Per iteration: {:.0} ns", per_iter_ns);
    println!("  Throughput: {} ops/sec", format_with_commas(throughput as u64));
    println!("  (sum={} to prevent DCE)", sum);
}

fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
