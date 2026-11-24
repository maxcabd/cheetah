//! Frame metrics from the connected device's session: what each frame cost,
//! by what caused it. `cheetah metrics` reads the file this writes.
//!
//! `work` is the runtime's own time (update, view, diff, layout, encode). It
//! excludes `measure`, the wait for the device to measure new text: that is
//! a network round trip here but an in-process call in an installed app.
//! Times are from this machine and build profile, so compare them with each
//! other, not with a phone.
//!
//! `device` is what the same frame cost on the phone, as it reports it:
//! applying the ops, by kind of work, then UIKit's layout and commit.

use runtime::dev::FrameReport;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Kept per trigger; older frames drop out.
const SAMPLES: usize = 500;
/// One 60 Hz frame.
pub const FRAME_BUDGET_US: u64 = 16_667;
const WRITE_EVERY: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Trigger {
    count: u64,
    over_budget: u64,
    unchanged: u64,
    frames: VecDeque<FrameReport>,
    device: VecDeque<DeviceFrame>,
}

/// Times a device reports for a frame, in the order they're listed there.
const DEVICE_TIMES: [&str; 10] = [
    "queue_us", "apply_us", "commit_us", "create_us", "props_us", "children_us", "layout_us",
    "remove_us", "native_us", "map_us",
];

#[derive(Clone, Debug)]
struct DeviceFrame([u64; DEVICE_TIMES.len()]);

impl DeviceFrame {
    fn read(message: &Value) -> Self {
        Self(DEVICE_TIMES.map(|name| message[name].as_u64().unwrap_or(0)))
    }

    /// Main-thread time, from the first op to the commit.
    fn main_us(&self) -> u64 {
        self.0[1] + self.0[2]
    }
}

fn summarize_device(frames: &[&DeviceFrame]) -> Value {
    let parts: serde_json::Map<String, Value> = DEVICE_TIMES
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let name = name.trim_end_matches("_us").to_string();
            (name, mean(frames.iter().map(|frame| frame.0[index])).into())
        })
        .collect();
    json!({
        "frames": frames.len(),
        "main_us": spread(frames.iter().map(|frame| frame.main_us())),
        "mean_us": parts,
    })
}

pub struct Metrics {
    app: String,
    started: Instant,
    written: Option<Instant>,
    triggers: BTreeMap<String, Trigger>,
    measures: u64,
    measure_timeouts: u64,
    measured_texts: u64,
    /// Waited for measurements since the last frame.
    waited_us: u64,
    /// Frames sent and not yet reported applied: cause and size.
    sent: VecDeque<(String, usize)>,
}

/// `report` with all `waited_us` counted as measuring. Texts known up front
/// are measured in one batch (`prime_us`); the rest are asked for one by one
/// while building or laying out, and those waits aren't that stage's work.
fn without_waits(mut report: FrameReport, waited_us: u64) -> FrameReport {
    let m = &mut report.metrics;
    let extra = waited_us.saturating_sub(m.prime_us);
    let in_layout = extra.min(m.layout_us);
    m.layout_us -= in_layout;
    m.build_us = m.build_us.saturating_sub(extra - in_layout);
    m.prime_us = m.prime_us.max(waited_us);
    report
}

/// Where the running server for `app` keeps its metrics.
pub fn path(app: &str) -> PathBuf {
    cheetah_cli::metrics_path(app)
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn spread(values: impl Iterator<Item = u64>) -> Value {
    let mut sorted: Vec<u64> = values.collect();
    sorted.sort_unstable();
    json!({
        "p50": percentile(&sorted, 0.5),
        "p95": percentile(&sorted, 0.95),
        "max": sorted.last().copied().unwrap_or(0),
    })
}

fn mean(values: impl Iterator<Item = u64>) -> u64 {
    let (sum, count) = values.fold((0, 0), |(sum, count), value| (sum + value, count + 1));
    if count == 0 {
        0
    } else {
        sum / count
    }
}

fn summarize<'a>(frames: impl Iterator<Item = &'a FrameReport> + Clone) -> Value {
    json!({
        "work_us": spread(frames.clone().map(FrameReport::work_us)),
        "measure_us": spread(frames.clone().map(|f| f.metrics.prime_us)),
        "wire_bytes": spread(frames.clone().map(|f| f.wire_bytes as u64)),
        "ops": spread(frames.clone().map(|f| f.metrics.ops as u64)),
        "nodes": spread(frames.clone().map(|f| f.metrics.nodes as u64)),
        "mean_us": {
            "update": mean(frames.clone().map(|f| f.metrics.update_us)),
            "build": mean(frames.clone().map(|f| f.metrics.build_us)),
            "diff": mean(frames.clone().map(|f| f.metrics.diff_us)),
            "layout": mean(frames.clone().map(|f| f.metrics.layout_us)),
            "encode": mean(frames.map(|f| f.encode_us)),
        },
    })
}

impl Metrics {
    pub fn new(app: &str) -> Self {
        Self {
            app: app.to_string(),
            started: Instant::now(),
            written: None,
            triggers: BTreeMap::new(),
            measures: 0,
            measure_timeouts: 0,
            measured_texts: 0,
            waited_us: 0,
            sent: VecDeque::new(),
        }
    }

    pub fn frame(&mut self, report: FrameReport) {
        let report = without_waits(report, std::mem::take(&mut self.waited_us));
        if report.wire_bytes > 0 {
            if self.sent.len() == SAMPLES {
                self.sent.pop_front();
            }
            self.sent.push_back((report.trigger.clone(), report.wire_bytes));
        }
        let trigger = self.triggers.entry(report.trigger.clone()).or_default();
        trigger.count += 1;
        trigger.over_budget += u64::from(report.work_us() > FRAME_BUDGET_US);
        trigger.unchanged += u64::from(report.metrics.ops == 0);
        if trigger.frames.len() == SAMPLES {
            trigger.frames.pop_front();
        }
        trigger.frames.push_back(report);
        if self.written.is_none_or(|at| at.elapsed() > WRITE_EVERY) {
            self.write();
        }
    }

    /// The device applied a frame. Reports come in the order frames were
    /// sent; the size skips over any that weren't ours (an alert).
    pub fn applied(&mut self, message: &Value) {
        let bytes = message["bytes"].as_u64().unwrap_or(0) as usize;
        let Some(at) = self.sent.iter().position(|(_, size)| *size == bytes) else {
            return;
        };
        let (name, _) = self.sent.drain(..=at).next_back().expect("found above");
        let trigger = self.triggers.entry(name).or_default();
        if trigger.device.len() == SAMPLES {
            trigger.device.pop_front();
        }
        trigger.device.push_back(DeviceFrame::read(message));
    }

    pub fn measured(&mut self, texts: usize, waited: Duration, timed_out: bool) {
        self.waited_us += waited.as_micros() as u64;
        self.measures += 1;
        self.measured_texts += texts as u64;
        self.measure_timeouts += u64::from(timed_out);
    }

    pub fn summary(&self) -> Value {
        let all = self.triggers.values().flat_map(|t| t.frames.iter());
        let triggers: serde_json::Map<String, Value> = self
            .triggers
            .iter()
            .map(|(name, trigger)| {
                let mut value = summarize(trigger.frames.iter());
                value["frames"] = trigger.count.into();
                value["over_budget"] = trigger.over_budget.into();
                value["unchanged"] = trigger.unchanged.into();
                if !trigger.device.is_empty() {
                    value["device"] = summarize_device(&trigger.device.iter().collect::<Vec<_>>());
                }
                (name.clone(), value)
            })
            .collect();
        let mut total = summarize(all);
        total["frames"] = self.triggers.values().map(|t| t.count).sum::<u64>().into();
        total["over_budget"] = self.triggers.values().map(|t| t.over_budget).sum::<u64>().into();
        total["unchanged"] = self.triggers.values().map(|t| t.unchanged).sum::<u64>().into();
        let device: Vec<&DeviceFrame> = self.triggers.values().flat_map(|t| &t.device).collect();
        if !device.is_empty() {
            total["device"] = summarize_device(&device);
        }
        json!({
            "app": self.app,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "session_s": self.started.elapsed().as_secs(),
            "budget_us": FRAME_BUDGET_US,
            "total": total,
            "triggers": triggers,
            "text_measuring": {
                "round_trips": self.measures,
                "texts": self.measured_texts,
                "timeouts": self.measure_timeouts,
            },
        })
    }

    /// Replaces the file in one step, so a reader never sees half of it.
    pub fn write(&mut self) {
        self.written = Some(Instant::now());
        let path = path(&self.app);
        let partial = path.with_extension("json.part");
        if std::fs::write(&partial, self.summary().to_string()).is_ok() {
            let _ = std::fs::rename(&partial, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::RuntimeMetrics;

    fn frame(trigger: &str, layout_us: u64, prime_us: u64, ops: usize) -> FrameReport {
        FrameReport {
            trigger: trigger.into(),
            metrics: RuntimeMetrics {
                layout_us,
                prime_us,
                ops,
                ..Default::default()
            },
            encode_us: 10,
            wire_bytes: 100,
        }
    }

    #[test]
    fn groups_frames_by_what_caused_them() {
        let mut metrics = Metrics::new("metrics-test");
        for us in [1_000, 2_000, 30_000] {
            metrics.frame(frame("click place", us, 0, 5));
        }
        metrics.frame(frame("scroll", 0, 0, 0));
        let summary = metrics.summary();
        let click = &summary["triggers"]["click place"];
        assert_eq!(click["frames"], 3);
        assert_eq!(click["over_budget"], 1);
        assert_eq!(click["work_us"]["p50"], 2_010);
        assert_eq!(click["work_us"]["max"], 30_010);
        assert_eq!(summary["triggers"]["scroll"]["unchanged"], 1);
        assert_eq!(summary["total"]["frames"], 4);
        let _ = std::fs::remove_file(path("metrics-test"));
    }

    #[test]
    fn waiting_for_the_device_is_not_work() {
        let report = frame("resize", 2_000, 400_000, 5);
        assert_eq!(report.work_us(), 2_010);
    }

    #[test]
    fn waits_during_layout_are_not_work() {
        let mut metrics = Metrics::new("metrics-wait-test");
        metrics.measured(20, Duration::from_micros(50_000), false);
        metrics.measured(1, Duration::from_micros(30_000), false);
        // 50 ms in the batch, 30 of layout's 32 ms waiting for one more text.
        metrics.frame(frame("resize", 32_000, 50_000, 5));
        let resize = &metrics.summary()["triggers"]["resize"];
        assert_eq!(resize["work_us"]["max"], 2_010);
        assert_eq!(resize["measure_us"]["max"], 80_000);
        // The next frame starts clean.
        metrics.frame(frame("scroll", 3_000, 0, 5));
        assert_eq!(metrics.summary()["triggers"]["scroll"]["work_us"]["max"], 3_010);
        let _ = std::fs::remove_file(path("metrics-wait-test"));
    }

    #[test]
    fn device_reports_go_with_the_frames_they_are_for() {
        let mut metrics = Metrics::new("metrics-device-test");
        let sized = |trigger: &str, wire_bytes| FrameReport {
            wire_bytes,
            ..frame(trigger, 1_000, 0, 5)
        };
        metrics.frame(sized("click place", 300));
        // Nothing was sent for this one, so nothing comes back.
        metrics.frame(sized("scroll", 0));
        metrics.frame(sized("native", 900));
        metrics.applied(&json!({"bytes": 300, "apply_us": 2_000, "commit_us": 3_000, "map_us": 500}));
        // Not one of the app's frames.
        metrics.applied(&json!({"bytes": 77, "apply_us": 99_000}));
        metrics.applied(&json!({"bytes": 900, "apply_us": 40_000, "commit_us": 10_000}));
        let summary = metrics.summary();
        let click = &summary["triggers"]["click place"]["device"];
        assert_eq!(click["main_us"]["max"], 5_000);
        assert_eq!(click["mean_us"]["map"], 500);
        assert_eq!(summary["triggers"]["native"]["device"]["main_us"]["max"], 50_000);
        assert!(summary["triggers"]["scroll"]["device"].is_null());
        assert_eq!(summary["total"]["device"]["frames"], 2);
        let _ = std::fs::remove_file(path("metrics-device-test"));
    }

    #[test]
    fn keeps_a_bounded_number_of_samples() {
        let mut metrics = Metrics::new("metrics-bound-test");
        for _ in 0..SAMPLES + 20 {
            metrics.frame(frame("scroll", 1, 0, 1));
        }
        assert_eq!(metrics.triggers["scroll"].frames.len(), SAMPLES);
        assert_eq!(metrics.triggers["scroll"].count as usize, SAMPLES + 20);
        let _ = std::fs::remove_file(path("metrics-bound-test"));
    }
}
