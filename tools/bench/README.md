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
cold_start_200_ms: p50=8.42 p95=10.15 max=10.15
initial_render_500_ms: 1.47
initial_ops: count=534 json_bytes=50925
scroll_frame_ms: p50=1.09 p95=1.65 max=2.79 ops_p50=76 json_bytes_p50=8690
empty_frame_ms: p50=0.51 p95=0.60 max=0.87
like_frame_ms: p50=2.74 p95=3.39 max=4.16 json_bytes_p50=166
```

These are samples, not a performance contract. CI results vary by runner.

The harness excludes native text measurement and host rendering. Unchanged frames now skip Taffy layout entirely by comparing the flattened tree and viewport. The remaining cost is rebuilding/flattening the view tree and JSON serialization; scroll frames are only a few kilobytes, so layout, not JSON, was the first measured bottleneck.

CI runs this benchmark on Ubuntu and publishes the output in the job summary. It is informational, not a hard timing gate, until budgets are established on stable hardware.
