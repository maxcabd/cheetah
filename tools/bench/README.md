# Cheetah runtime benchmark

A repeatable, dependency-light benchmark for the Rust runtime path. It measures layout, diffing, and JSON op serialization without a native host.

```bash
cargo run -p cheetah-bench --release
```

## Scenarios

- `cold_start_200_ms`: open a fresh store, seed 200 messages, create the runtime, and render once.
- `initial_render_500_ms`: render a 500-message runtime after the store is ready.
- `scroll_frame_ms`: 40 scroll frames from the bottom to the top of a 500-message list.
- `empty_frame_ms`: 20 unchanged renders.
- `like_frame_ms`: 20 like-button state changes.

Each output includes p50, p95, and max where applicable, plus operation count and JSON payload size for scroll frames.

## Current sample

Windows release build, 393x852 dp viewport:

```text
cold_start_200_ms: p50=8.19 p95=9.32 max=9.32
initial_render_500_ms: 1.43
initial_ops: count=534 json_bytes=50925
scroll_frame_ms: p50=1.01 p95=1.40 max=1.50 ops_p50=76 json_bytes_p50=8690
empty_frame_ms: p50=0.29 p95=0.44 max=0.83
like_frame_ms: p50=1.40 p95=1.96 max=1.97 json_bytes_p50=166
```

These are samples, not a performance contract. CI results vary by runner.

The harness excludes native text measurement and host rendering. Two caches now apply:

- unchanged tree + viewport: skip Taffy and return no ops
- non-layout prop changes (fixed-size text, color, background, radius, input value, scroll offset): reuse frames and emit only update ops

The remaining cost is rebuilding and flattening the view tree, plus diffing and JSON serialization. Scroll frames remain layout-heavy because their absolute row positions change. JSON is still only a few kilobytes per scroll frame.

CI runs this benchmark on Ubuntu and publishes the output in the job summary. It is informational, not a hard timing gate, until budgets are established on stable hardware.
