use runtime::demo::ChatApp;
use runtime::{App, Event, Runtime, Viewport};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

const HOST_INFO: &str = r#"{
    "safeTop": 0,
    "safeBottom": 0,
    "keyboardInset": 0,
    "overlayChrome": true,
    "glass": true,
    "scale": 1
}"#;

static FILE_COUNTER: AtomicU32 = AtomicU32::new(0);

fn main() {
    let viewport = Viewport {
        width: 393.0,
        height: 852.0,
    };

    println!("Cheetah runtime benchmark");
    println!("viewport: {}x{} dp", viewport.width, viewport.height);
    println!("host: floating chrome, 500-message dataset");
    println!();

    let cold_start = measure_cold_start(viewport);
    print_stats("cold_start_200_ms", &cold_start);

    let path = temp_path();
    let mut runtime = build_runtime(&path, 300);
    let initial_start = Instant::now();
    let initial_ops = runtime.render(viewport);
    let initial_ms = elapsed_ms(initial_start);
    let initial_json = serde_json::to_vec(&initial_ops).expect("serialize initial ops");
    let initial_wire = runtime::encode_ops(&initial_ops);

    println!("initial_render_500_ms: {initial_ms:.2}");
    println!(
        "initial_ops: count={} wire_bytes={} debug_json_bytes={}",
        initial_ops.len(),
        initial_wire.len(),
        initial_json.len()
    );
    let metrics = runtime.metrics();
    println!(
        "runtime_metrics: build_us={} diff_us={} prime_us={} layout_us={} nodes={} ops={} text_requests={}",
        metrics.build_us,
        metrics.diff_us,
        metrics.prime_us,
        metrics.layout_us,
        metrics.nodes,
        metrics.ops,
        metrics.text_requests
    );

    let scroll_max = scroll_max_offset(&initial_json).unwrap_or(40_000.0);
    let mut scroll_ms = Vec::new();
    let mut scroll_bytes = Vec::new();
    let mut scroll_ops = Vec::new();
    for step in 0..40 {
        let offset = scroll_max * (1.0 - step as f32 / 39.0);
        let event = Event {
            id: 0,
            key: Some("messages".into()),
            kind: "scroll".into(),
            payload: offset.to_string(),
        };
        let start = Instant::now();
        let ops = runtime.dispatch(event);
        scroll_ms.push(elapsed_ms(start));
        scroll_ops.push(ops.len());
        scroll_bytes.push(runtime::encode_ops(&ops).len());
    }
    let scroll_stats = stats(scroll_ms);
    println!(
        "scroll_frame_ms: p50={:.2} p95={:.2} max={:.2} ops_p50={} wire_bytes_p50={}",
        scroll_stats.p50,
        scroll_stats.p95,
        scroll_stats.max,
        median(&scroll_ops),
        median(&scroll_bytes)
    );

    let mut empty_ms = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let ops = runtime.render(viewport);
        empty_ms.push(elapsed_ms(start));
        debug_assert!(ops.is_empty(), "unchanged render should be empty");
    }
    print_stats("empty_frame_ms", &empty_ms);

    let mut like_ms = Vec::new();
    let mut like_bytes = Vec::new();
    for _ in 0..20 {
        let event = Event {
            id: 0,
            key: Some("like-slot-0".into()),
            kind: "click".into(),
            payload: String::new(),
        };
        let start = Instant::now();
        let ops = runtime.dispatch(event);
        like_ms.push(elapsed_ms(start));
        like_bytes.push(runtime::encode_ops(&ops).len());
    }
    let like_stats = stats(like_ms);
    println!(
        "like_frame_ms: p50={:.2} p95={:.2} max={:.2} wire_bytes_p50={}",
        like_stats.p50,
        like_stats.p95,
        like_stats.max,
        median(&like_bytes)
    );

    let budget_ms = std::env::var("CHEETAH_BENCH_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(50.0);
    println!("frame_budget_ms: {budget_ms:.2}");
    let mut failed = false;
    for (name, value) in [
        ("scroll_p95", scroll_stats.p95),
        ("like_p95", like_stats.p95),
    ] {
        if value > budget_ms {
            eprintln!("benchmark budget exceeded: {name}={value:.2}ms > {budget_ms:.2}ms");
            failed = true;
        }
    }

    drop(runtime);
    let _ = fs::remove_file(path);

    println!();
    println!("note: measurements include layout, diff, and wire encoding; they exclude native text measurement and host rendering");
    if failed {
        std::process::exit(1);
    }
}

fn build_runtime(path: &Path, extra_messages: usize) -> Runtime {
    let mut app = ChatApp::new(path.to_str().expect("temp path"));
    for index in 0..extra_messages {
        app.update(Event {
            id: 0,
            key: None,
            kind: "incoming".into(),
            payload: format!("bench\u{1f}bench message {index}"),
        });
    }

    let mut runtime = Runtime::new(app);
    runtime.event(0, "host", HOST_INFO);
    runtime
}

fn measure_cold_start(viewport: Viewport) -> Vec<f64> {
    let mut times = Vec::new();
    for _ in 0..3 {
        let path = temp_path();
        let start = Instant::now();
        let mut runtime = build_runtime(&path, 0);
        let _ = runtime.render(viewport);
        times.push(elapsed_ms(start));
        drop(runtime);
        let _ = fs::remove_file(path);
    }
    times
}

fn temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "cheetah-bench-{}-{}.redb",
        std::process::id(),
        FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

struct Stats {
    p50: f64,
    p95: f64,
    max: f64,
}

fn stats(mut values: Vec<f64>) -> Stats {
    values.sort_by(|left, right| left.total_cmp(right));
    Stats {
        p50: percentile(&values, 0.50),
        p95: percentile(&values, 0.95),
        max: *values.last().unwrap_or(&0.0),
    }
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * fraction).round() as usize;
    values[index.min(values.len() - 1)]
}

fn median<T: Copy + Ord>(values: &[T]) -> T {
    assert!(!values.is_empty(), "empty sample");
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted[sorted.len() / 2]
}

fn scroll_max_offset(ops: &[u8]) -> Option<f32> {
    let value: Value = serde_json::from_slice(ops).ok()?;
    value.as_array()?.iter().find_map(|op| {
        (op.get("kind")?.as_str()? == "list")
            .then(|| op.get("props")?.get("scrollTo")?.as_f64())
            .flatten()
            .map(|offset| offset as f32)
    })
}

fn print_stats(label: &str, values: &[f64]) {
    let stats = stats(values.to_vec());
    println!(
        "{label}: p50={:.2} p95={:.2} max={:.2}",
        stats.p50, stats.p95, stats.max
    );
}
