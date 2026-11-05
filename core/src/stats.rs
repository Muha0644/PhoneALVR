//! Lightweight statistics used by the HUD and diagnostics export. Timestamps
//! are supplied by the caller (nanoseconds, monotonic) so the logic is
//! deterministic in tests.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub latency_ms: f32,
    pub dropped_frames: u64,
    pub frames: u64,
    pub bitrate_mbps: f32,
    pub view_width: u32,
    pub view_height: u32,
    pub codec: String,
    pub tracking_hz: f32,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_time_ms: 0.0,
            latency_ms: 0.0,
            dropped_frames: 0,
            frames: 0,
            bitrate_mbps: 0.0,
            view_width: 0,
            view_height: 0,
            codec: "unknown".to_owned(),
            tracking_hz: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatsTracker {
    stats: Stats,
    last_frame_ns: Option<u64>,
    last_tracking_ns: Option<u64>,
    fps_ema: f32,
    frame_time_ema: f32,
    latency_ema: f32,
    tracking_ema: f32,
    alpha: f32,
}

impl Default for StatsTracker {
    fn default() -> Self {
        Self::new(0.1)
    }
}

impl StatsTracker {
    pub fn new(alpha: f32) -> Self {
        Self {
            stats: Stats::default(),
            last_frame_ns: None,
            last_tracking_ns: None,
            fps_ema: 0.0,
            frame_time_ema: 0.0,
            latency_ema: 0.0,
            tracking_ema: 0.0,
            alpha: alpha.clamp(0.001, 1.0),
        }
    }

    fn ema(previous: f32, sample: f32, alpha: f32) -> f32 {
        if previous == 0.0 {
            sample
        } else {
            previous + alpha * (sample - previous)
        }
    }

    /// Called once per presented frame.
    pub fn on_frame(&mut self, now_ns: u64) {
        self.stats.frames = self.stats.frames.saturating_add(1);
        if let Some(previous) = self.last_frame_ns {
            let delta_ns = now_ns.saturating_sub(previous);
            if delta_ns > 0 {
                let delta_ms = delta_ns as f32 / 1_000_000.0;
                self.frame_time_ema = Self::ema(self.frame_time_ema, delta_ms, self.alpha);
                let fps = 1000.0 / delta_ms;
                self.fps_ema = Self::ema(self.fps_ema, fps, self.alpha);
            }
        }
        self.last_frame_ns = Some(now_ns);
        self.stats.fps = self.fps_ema;
        self.stats.frame_time_ms = self.frame_time_ema;
    }

    pub fn on_dropped_frame(&mut self) {
        self.stats.dropped_frames = self.stats.dropped_frames.saturating_add(1);
    }

    pub fn set_dropped_frames(&mut self, count: u64) {
        self.stats.dropped_frames = count;
    }

    /// ALVR's end-to-end pipeline estimate, in milliseconds.
    pub fn on_latency(&mut self, latency_ms: f32) {
        if latency_ms.is_finite() && latency_ms >= 0.0 {
            self.latency_ema = Self::ema(self.latency_ema, latency_ms, self.alpha);
            self.stats.latency_ms = self.latency_ema;
        }
    }

    /// Called once per tracking sample sent.
    pub fn on_tracking(&mut self, now_ns: u64) {
        if let Some(previous) = self.last_tracking_ns {
            let delta_ns = now_ns.saturating_sub(previous);
            if delta_ns > 0 {
                let hz = 1_000_000_000.0 / delta_ns as f32;
                self.tracking_ema = Self::ema(self.tracking_ema, hz, self.alpha);
                self.stats.tracking_hz = self.tracking_ema;
            }
        }
        self.last_tracking_ns = Some(now_ns);
    }

    pub fn set_stream(&mut self, width: u32, height: u32, codec: &str) {
        self.stats.view_width = width;
        self.stats.view_height = height;
        self.stats.codec = codec.to_owned();
    }

    pub fn set_codec(&mut self, codec: &str) {
        self.stats.codec = codec.to_owned();
    }

    pub fn set_bitrate_mbps(&mut self, bitrate: f32) {
        if bitrate.is_finite() && bitrate >= 0.0 {
            self.stats.bitrate_mbps = bitrate;
        }
    }

    pub fn snapshot(&self) -> Stats {
        self.stats.clone()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.stats).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn reset_session_counters(&mut self) {
        self.stats.dropped_frames = 0;
        self.stats.frames = 0;
        self.last_frame_ns = None;
        self.last_tracking_ns = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ninety_hertz_matches_expected_fps_and_frame_time() {
        let mut tracker = StatsTracker::new(1.0);
        let period = 1_000_000_000u64 / 90;
        for i in 0..100 {
            tracker.on_frame(i * period);
        }
        let stats = tracker.snapshot();
        assert!((stats.fps - 90.0).abs() < 0.5, "fps={}", stats.fps);
        assert!(
            (stats.frame_time_ms - 11.111).abs() < 0.1,
            "frame_time={}",
            stats.frame_time_ms
        );
    }

    #[test]
    fn dropped_frames_and_frames_are_counted() {
        let mut tracker = StatsTracker::new(1.0);
        tracker.on_frame(0);
        tracker.on_frame(1_000_000);
        tracker.on_dropped_frame();
        tracker.on_dropped_frame();
        let stats = tracker.snapshot();
        assert_eq!(stats.frames, 2);
        assert_eq!(stats.dropped_frames, 2);
    }

    #[test]
    fn latency_rejects_non_finite_values() {
        let mut tracker = StatsTracker::new(1.0);
        tracker.on_latency(42.0);
        tracker.on_latency(f32::NAN);
        tracker.on_latency(-1.0);
        assert_eq!(tracker.snapshot().latency_ms, 42.0);
    }

    #[test]
    fn tracking_rate_is_measured() {
        let mut tracker = StatsTracker::new(1.0);
        let period = 1_000_000_000u64 / 500;
        for i in 0..10 {
            tracker.on_tracking(i * period);
        }
        assert!((tracker.snapshot().tracking_hz - 500.0).abs() < 5.0);
    }

    #[test]
    fn codec_update_preserves_stream_dimensions() {
        let mut tracker = StatsTracker::new(0.5);
        tracker.set_stream(1184, 1056, "unknown");
        tracker.set_codec("hevc");
        let stats = tracker.snapshot();
        assert_eq!(stats.view_width, 1184);
        assert_eq!(stats.view_height, 1056);
        assert_eq!(stats.codec, "hevc");
    }

    #[test]
    fn json_contains_all_fields() {
        let mut tracker = StatsTracker::new(0.5);
        tracker.set_stream(1184, 1056, "h264");
        tracker.set_bitrate_mbps(80.0);
        let json = tracker.to_json();
        assert!(json.contains("view_width"));
        assert!(json.contains("1184"));
        assert!(json.contains("h264"));
    }
}
