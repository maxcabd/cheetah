//! Frame metrics from the connected device's session: what each frame cost,
//! by what caused it. `cheetah metrics` reads the file this writes.
//!
//! `work` is the runtime's own time (update, view, diff, layout, encode). It
//! excludes `measure`, the wait for the device to measure new text: that is
//! a network round trip here but an in-process call in an installed app.
//! Neither includes drawing on the device. Times are from this machine and
//! build profile, so compare them with each other, not with a phone.

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
        }
    }

    pub fn frame(&mut self, report: FrameReport) {
        let report = without_waits(report, std::mem::take(&mut self.waited_us));
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
                (name.clone(), value)
            })
            .collect();
        let mut total = summarize(all);
        total["frames"] = self.triggers.values().map(|t| t.count).sum::<u64>().into();
        total["over_budget"] = self.triggers.values().map(|t| t.over_budget).sum::<u64>().into();
        total["unchanged"] = self.triggers.values().map(|t| t.unchanged).sum::<u64>().into();
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
