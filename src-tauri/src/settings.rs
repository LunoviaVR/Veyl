//! User settings: one JSON file in the data directory, validated and clamped on
//! load and on every save so out-of-range values from a hand-edited file or the
//! UI can never reach the samplers.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use veyl_core::alerts::{Comparator, ThresholdRule};
use veyl_core::events::Severity;
use veyl_storage::Retention;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EventAlerts {
    pub new_bsod: bool,
    pub unexpected_restart: bool,
    pub repeated_crashes: bool,
    /// Crashes of the same application within `crash_window_min` that trigger an alert.
    pub crash_count: u32,
    pub crash_window_min: u32,
}

impl Default for EventAlerts {
    fn default() -> Self {
        Self {
            new_bsod: true,
            unexpected_restart: true,
            repeated_crashes: true,
            crash_count: 3,
            crash_window_min: 30,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlaySettings {
    pub enabled: bool,
    /// One of `OVERLAY_POSITIONS`: where the overlay sits on the game's window.
    pub position: String,
    pub scale: f64,
    pub opacity: f64,
    pub refresh_ms: u32,
    /// One of `OVERLAY_DESIGNS` (a folder under public/overlay-themes).
    pub design: String,
    /// One of `OVERLAY_THEMES`.
    pub theme: String,
}

/// Bundled overlay designs; must match `OVERLAY_DESIGNS` in src/overlay/ovl.ts.
pub const OVERLAY_DESIGNS: [&str; 18] = [
    "essential/sleek-compact",
    "essential/sleek-rings",
    "essential/sleek-meters",
    "essential/sleek-meters-universal",
    "essential/sleek",
    "essential/simple-sleek",
    "essential/sleek-side",
    "essential/performance",
    "laptop/card",
    "laptop/island",
    "laptop/island-time",
    "laptop/minibar",
    "laptop/notch",
    "tv/box",
    "tv/line",
    "tv/pill",
    "tv/side",
    "cassette/console",
];
/// A 3×3 grid over the game's window; must match `OVERLAY_POSITIONS` in src/overlay/ovl.ts.
pub const OVERLAY_POSITIONS: [&str; 9] = [
    "topLeft",
    "topCenter",
    "topRight",
    "middleLeft",
    "center",
    "middleRight",
    "bottomLeft",
    "bottomCenter",
    "bottomRight",
];
/// Dark, Smoke ("clear", its folder name in the source repository), Float, White.
pub const OVERLAY_THEMES: [&str; 4] = ["dark", "clear", "float", "white"];

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            position: "topCenter".into(), // where the bundled designs are meant to sit
            scale: 1.0,
            opacity: 0.85,
            refresh_ms: 500,
            design: OVERLAY_DESIGNS[0].into(),
            theme: OVERLAY_THEMES[0].into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// system | dark | light | cassette (the Cassette Futurism console)
    pub theme: String,
    pub sample_interval_ms: u32,
    pub sensor_interval_ms: u32,
    pub storage_health_interval_min: u32,
    pub history_enabled: bool,
    pub retention: Retention,
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub auto_update_check: bool,
    pub start_minimized: bool,
    pub notifications_enabled: bool,
    pub event_alerts: EventAlerts,
    pub alert_rules: Vec<ThresholdRule>,
    pub presentmon_path: Option<String>,
    pub debugger_path: Option<String>,
    pub symbol_server_enabled: bool,
    pub stutter_factor: f64,
    pub overlay: OverlaySettings,
    pub export_redact_default: bool,
    /// Start the FPS capture by itself when a game Windows recognises is running.
    pub auto_capture_games: bool,
}

// A positional constructor keeps the default rule table readable as one line per rule.
#[allow(clippy::too_many_arguments)]
fn rule(
    id: &str,
    name: &str,
    metric: &str,
    cmp: Comparator,
    threshold: f64,
    hysteresis: f64,
    duration: u32,
    cooldown: u32,
    severity: Severity,
    notify: bool,
) -> ThresholdRule {
    ThresholdRule {
        id: id.into(),
        name: name.into(),
        enabled: true,
        metric: metric.into(),
        comparator: cmp,
        threshold,
        hysteresis,
        duration_secs: duration,
        cooldown_secs: cooldown,
        severity,
        notify,
    }
}

pub fn default_rules() -> Vec<ThresholdRule> {
    use Comparator::*;
    vec![
        rule(
            "gpu-temp",
            "GPU temperature high",
            "gpu.*.temp_c",
            Above,
            90.0,
            5.0,
            30,
            900,
            Severity::Warning,
            true,
        ),
        rule(
            "gpu-hotspot",
            "GPU hotspot temperature high",
            "gpu.*.hotspot_c",
            Above,
            105.0,
            5.0,
            30,
            900,
            Severity::Warning,
            true,
        ),
        rule(
            "cpu-temp",
            "CPU package temperature high",
            "cpu.package_temp_c",
            Above,
            95.0,
            5.0,
            30,
            900,
            Severity::Warning,
            true,
        ),
        rule(
            "cpu-sustained",
            "Sustained high CPU usage",
            "cpu.total_pct",
            Above,
            95.0,
            10.0,
            300,
            3600,
            Severity::Info,
            false,
        ),
        rule(
            "memory-pressure",
            "Memory almost full",
            "memory.load_pct",
            Above,
            92.0,
            4.0,
            120,
            1800,
            Severity::Warning,
            true,
        ),
        rule(
            "commit-pressure",
            "Committed memory near the limit",
            "memory.commit_pct",
            Above,
            90.0,
            3.0,
            60,
            1800,
            Severity::Warning,
            true,
        ),
        rule(
            "disk-space",
            "Low free disk space",
            "volume.*.free_pct",
            Below,
            10.0,
            1.0,
            0,
            86_400,
            Severity::Warning,
            true,
        ),
        rule(
            "thermal-throttle",
            "Windows thermal throttling (passive cooling)",
            "thermal.passive_limit_min_pct",
            Below,
            100.0,
            0.5,
            30,
            1800,
            Severity::Warning,
            true,
        ),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            sample_interval_ms: 1000,
            sensor_interval_ms: 2000,
            storage_health_interval_min: 30,
            history_enabled: true,
            retention: Retention::default(),
            close_to_tray: true,
            minimize_to_tray: true,
            auto_update_check: true,
            start_minimized: false,
            notifications_enabled: true,
            event_alerts: EventAlerts::default(),
            alert_rules: default_rules(),
            presentmon_path: None,
            debugger_path: None,
            symbol_server_enabled: false,
            stutter_factor: 2.5,
            overlay: OverlaySettings::default(),
            export_redact_default: true,
            auto_capture_games: true,
        }
    }
}

impl Settings {
    /// Clamps every numeric field to its safe range and drops invalid rules.
    pub fn validated(mut self) -> Self {
        self.sample_interval_ms = self.sample_interval_ms.clamp(500, 5000);
        self.sensor_interval_ms = self
            .sensor_interval_ms
            .clamp(1000, 10_000)
            .max(self.sample_interval_ms);
        self.storage_health_interval_min = self.storage_health_interval_min.clamp(5, 1440);
        self.retention.raw_hours = self.retention.raw_hours.clamp(1, 168);
        self.retention.minute_days = self.retention.minute_days.clamp(1, 365);
        self.retention.event_days = self.retention.event_days.clamp(7, 3650);
        self.retention.session_days = self.retention.session_days.clamp(1, 3650);
        if !["system", "dark", "light", "cassette"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        self.stutter_factor = if self.stutter_factor.is_finite() {
            self.stutter_factor.clamp(1.5, 10.0)
        } else {
            2.5
        };
        self.event_alerts.crash_count = self.event_alerts.crash_count.clamp(2, 50);
        self.event_alerts.crash_window_min = self.event_alerts.crash_window_min.clamp(1, 1440);
        let o = &mut self.overlay;
        o.scale = if o.scale.is_finite() {
            o.scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        o.opacity = if o.opacity.is_finite() {
            o.opacity.clamp(0.2, 1.0)
        } else {
            0.85
        };
        o.refresh_ms = o.refresh_ms.clamp(250, 5000);
        if !OVERLAY_POSITIONS.contains(&o.position.as_str()) {
            o.position = "topCenter".into();
        }
        if !OVERLAY_DESIGNS.contains(&o.design.as_str()) {
            o.design = OVERLAY_DESIGNS[0].into();
        }
        if !OVERLAY_THEMES.contains(&o.theme.as_str()) {
            o.theme = OVERLAY_THEMES[0].into();
        }
        for p in [&mut self.presentmon_path, &mut self.debugger_path] {
            if p.as_deref()
                .is_some_and(|s| s.trim().is_empty() || s.len() > 1024)
            {
                *p = None;
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.alert_rules.retain(|r| {
            r.threshold.is_finite()
                && r.hysteresis.is_finite()
                && !r.id.is_empty()
                && r.id.len() <= 64
                && r.metric.len() <= 128
                && r.metric.matches('*').count() <= 1
                && seen.insert(r.id.clone())
        });
        for r in &mut self.alert_rules {
            r.hysteresis = r.hysteresis.abs();
            r.duration_secs = r.duration_secs.min(86_400);
            r.cooldown_secs = r.cooldown_secs.min(7 * 86_400);
            // Cut at a character boundary: String::truncate panics inside a multi-byte letter.
            if let Some((i, _)) = r.name.char_indices().nth(120) {
                r.name.truncate(i);
            }
        }
        self
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s.validated(),
                Err(e) => {
                    tracing::warn!(
                        "settings file is invalid ({e}); keeping a backup and using defaults"
                    );
                    let _ = std::fs::copy(path, path.with_extension("json.invalid"));
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Atomic save: write a temp file, then rename over the old one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp: PathBuf = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_values() {
        let mut s = Settings {
            sample_interval_ms: 1,
            sensor_interval_ms: 999_999,
            theme: "neon".into(),
            stutter_factor: f64::NAN,
            ..Default::default()
        };
        s.overlay.opacity = 7.0;
        s.overlay.design = "../../secrets".into();
        s.overlay.theme = "neon".into();
        s.alert_rules.push(s.alert_rules[0].clone()); // duplicate id
        let v = s.validated();
        assert_eq!(v.sample_interval_ms, 500);
        assert_eq!(v.sensor_interval_ms, 10_000);
        assert_eq!(v.theme, "system");
        let cassette = Settings {
            theme: "cassette".into(),
            ..Default::default()
        };
        assert_eq!(cassette.validated().theme, "cassette");
        assert_eq!(v.stutter_factor, 2.5);
        assert_eq!(v.overlay.opacity, 1.0);
        assert_eq!(v.overlay.design, OVERLAY_DESIGNS[0]);
        assert_eq!(v.overlay.theme, OVERLAY_THEMES[0]);
        assert_eq!(v.alert_rules.len(), default_rules().len());
    }

    #[test]
    fn every_overlay_design_and_theme_is_bundled() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/overlay-themes");
        for d in OVERLAY_DESIGNS {
            for t in OVERLAY_THEMES {
                for f in ["layout.ovl", "atlas.png"] {
                    assert!(root.join(d).join(t).join(f).is_file(), "{d}/{t}/{f}");
                }
            }
        }
    }

    #[test]
    fn long_non_ascii_rule_name_is_cut_without_panicking() {
        let mut s = Settings::default();
        s.alert_rules[0].name = "é".repeat(200); // byte 120 falls inside a letter
        assert_eq!(s.validated().alert_rules[0].name.chars().count(), 120);
    }

    #[test]
    fn partial_json_fills_defaults_and_round_trips() {
        let s: Settings =
            serde_json::from_str(r#"{"theme":"dark","overlay":{"enabled":true}}"#).expect("parse");
        assert_eq!(s.theme, "dark");
        assert!(s.overlay.enabled);
        assert_eq!(s.overlay.refresh_ms, 500);
        assert_eq!(s.sample_interval_ms, 1000);
        let dir = std::env::temp_dir().join(format!("veyl-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let p = dir.join("settings.json");
        s.save(&p).expect("save");
        assert_eq!(Settings::load(&p), s.validated());
        std::fs::write(&p, "{ not json").expect("write");
        assert_eq!(Settings::load(&p), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
