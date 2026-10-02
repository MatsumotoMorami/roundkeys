use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Binding {
    Keyboard {
        code: u16,
    },
    ControllerButton {
        device: u32,
        button: u8,
    },
    ControllerAxis {
        device: u32,
        axis: u8,
        direction: i8,
    },
}

impl Binding {
    pub fn label(self) -> String {
        match self {
            Self::Keyboard { code } => key_name(code),
            Self::ControllerButton { device, button } => {
                format!("HID {:04X} · B{}", device & 0xffff, button + 1)
            }
            Self::ControllerAxis {
                device,
                axis,
                direction,
            } => format!(
                "HID {:04X} · {} {}",
                device & 0xffff,
                ["X", "Y", "Z", "Rx", "Ry", "Rz", "Slider", "Dial", "Wheel"][axis.min(8) as usize],
                if direction > 0 { "+" } else { "−" }
            ),
        }
    }
}

pub fn key_name(code: u16) -> String {
    match code {
        48..=90 => char::from_u32(code as u32).unwrap().to_string(),
        112..=135 => format!("F{}", code - 111),
        96..=105 => format!("Num {}", code - 96),
        32 => "Space".into(),
        160 => "L Shift".into(),
        161 => "R Shift".into(),
        162 => "L Ctrl".into(),
        163 => "R Ctrl".into(),
        164 => "L Alt".into(),
        165 => "R Alt".into(),
        13 => "Enter".into(),
        9 => "Tab".into(),
        8 => "Backspace".into(),
        27 => "Esc".into(),
        37 => "Left".into(),
        38 => "Up".into(),
        39 => "Right".into(),
        40 => "Down".into(),
        186 => ";".into(),
        187 => "=".into(),
        188 => ",".into(),
        189 => "−".into(),
        190 => ".".into(),
        191 => "/".into(),
        192 => "`".into(),
        219 => "[".into(),
        220 => "\\".into(),
        221 => "]".into(),
        222 => "'".into(),
        _ => format!("VK {code}"),
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Lane {
    pub label: String,
    pub color: [u8; 4],
    pub bindings: Vec<Binding>,
}
impl Default for Lane {
    fn default() -> Self {
        Self {
            label: "KEY".into(),
            color: [168, 199, 255, 255],
            bindings: vec![],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lanes: [Lane; 8],
    pub key_width: f32,
    pub key_height: f32,
    pub spacing: f32,
    pub scratch_size: f32,
    pub scratch_gap: f32,
    pub radius: f32,
    pub trail_height: f32,
    pub speed: f32,
    pub trail_opacity: f32,
    pub glow: f32,
    pub background: [u8; 4],
    pub show_labels: bool,
    pub show_guides: bool,
    pub stagger: bool,
    pub output_delay_ms: f64,
    pub scratch_threshold: f64,
    pub scratch_release_ms: f64,
}
impl Default for Config {
    fn default() -> Self {
        let codes = [160, 83, 68, 70, 32, 74, 75, 76];
        Self {
            lanes: std::array::from_fn(|i| Lane {
                label: if i == 0 { "SCR".into() } else { i.to_string() },
                color: if i == 0 {
                    [255, 163, 192, 255]
                } else if i % 2 == 0 {
                    [170, 158, 255, 255]
                } else {
                    [160, 224, 228, 255]
                },
                bindings: if i == 0 {
                    vec![
                        Binding::Keyboard { code: 160 },
                        Binding::Keyboard { code: 162 },
                    ]
                } else {
                    vec![Binding::Keyboard { code: codes[i] }]
                },
            }),
            key_width: 62.0,
            key_height: 68.0,
            spacing: 10.0,
            scratch_size: 92.0,
            scratch_gap: 26.0,
            radius: 16.0,
            trail_height: 380.0,
            speed: 270.0,
            trail_opacity: 0.8,
            glow: 0.65,
            background: [15, 19, 30, 0],
            show_labels: true,
            show_guides: false,
            stagger: true,
            output_delay_ms: 0.0,
            scratch_threshold: 0.001,
            scratch_release_ms: 65.0,
        }
    }
}
impl Config {
    pub fn normalize(&mut self) {
        self.key_width = finite(self.key_width, 62.0).clamp(32.0, 120.0);
        self.key_height = finite(self.key_height, 68.0).clamp(32.0, 120.0);
        self.spacing = finite(self.spacing, 10.0).clamp(2.0, 48.0);
        self.scratch_size = finite(self.scratch_size, 92.0).clamp(48.0, 150.0);
        self.scratch_gap = finite(self.scratch_gap, 26.0).clamp(4.0, 90.0);
        self.radius = finite(self.radius, 16.0).clamp(4.0, 48.0);
        self.trail_height = finite(self.trail_height, 380.0).clamp(120.0, 900.0);
        self.speed = finite(self.speed, 270.0).clamp(60.0, 900.0);
        self.trail_opacity = finite(self.trail_opacity, 0.8).clamp(0.1, 1.0);
        self.glow = finite(self.glow, 0.65).clamp(0.0, 1.0);
        self.output_delay_ms = if self.output_delay_ms.is_finite() {
            self.output_delay_ms.clamp(0.0, 3000.0)
        } else {
            0.0
        };
        self.scratch_threshold = if self.scratch_threshold.is_finite() {
            self.scratch_threshold.clamp(0.00001, 0.05)
        } else {
            0.001
        };
        self.scratch_release_ms = if self.scratch_release_ms.is_finite() {
            self.scratch_release_ms.clamp(16.0, 200.0)
        } else {
            65.0
        };
        for lane in &mut self.lanes {
            lane.label = lane.label.chars().take(24).collect();
            lane.bindings.retain(|b| match b {
                Binding::Keyboard { code } => *code <= 255,
                Binding::ControllerButton { button, .. } => *button < 128,
                Binding::ControllerAxis {
                    axis, direction, ..
                } => *axis < 9 && [-1, 1].contains(direction),
            });
            lane.bindings.truncate(4);
        }
    }
    pub fn dimensions(&self) -> [f32; 2] {
        [
            40.0 + self.scratch_size + self.scratch_gap + self.key_width * 7.0 + self.spacing * 6.0,
            self.trail_height + self.key_height.max(self.scratch_size) + 52.0,
        ]
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut config: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        config.normalize();
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let temp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        // Windows rename does not replace existing files; retain a recoverable backup.
        if path.exists() {
            std::fs::copy(path, path.with_extension("json.bak")).map_err(|e| e.to_string())?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            unsafe extern "system" {
                fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
            }
            let a: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        std::fs::rename(temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn finite(x: f32, fallback: f32) -> f32 {
    if x.is_finite() { x } else { fallback }
}

#[derive(Clone, Debug, Serialize)]
pub struct Stroke {
    pub lane: usize,
    pub start: f64,
    pub end: Option<f64>,
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub now: f64,
    pub config: Config,
    pub strokes: Vec<Stroke>,
    pub pressed: [bool; 8],
    pub counts: [u64; 8],
}
pub struct Model {
    pub config: Config,
    pub epoch: Instant,
    pub strokes: Vec<Stroke>,
    pub counts: [u64; 8],
    pub down: HashSet<Binding>,
    pub capture: Option<(usize, usize)>,
    pub capture_version: u64,
    pub input_status: String,
    pub controller_status: String,
    pub server_status: String,
    pub clients: usize,
    pub paused: bool,
    pub devices: Vec<(u32, String)>,
    pub last_input: String,
    pub demo: bool,
}
impl Model {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            epoch: Instant::now(),
            strokes: vec![],
            counts: [0; 8],
            down: HashSet::new(),
            capture: None,
            capture_version: 0,
            input_status: "正在启动输入监听…".into(),
            controller_status: "正在检测控制器…".into(),
            server_status: "正在启动 OBS 服务…".into(),
            clients: 0,
            paused: false,
            devices: vec![],
            last_input: "等待输入".into(),
            demo: false,
        }
    }
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64() * 1000.0
    }
    pub fn pressed(&self) -> [bool; 8] {
        std::array::from_fn(|i| {
            !self.paused
                && self.config.lanes[i]
                    .bindings
                    .iter()
                    .any(|b| self.down.contains(b))
        })
    }
    pub fn transition(&mut self, binding: Binding, down: bool, at: f64) {
        if down && !self.down.contains(&binding) {
            self.last_input = binding.label();
        }
        if down && let Some((lane, slot)) = self.capture {
            if binding == (Binding::Keyboard { code: 27 }) {
                self.capture = None;
                return;
            }
            if !self.down.contains(&binding) {
                let bindings = &mut self.config.lanes[lane].bindings;
                if slot < bindings.len() {
                    bindings[slot] = binding;
                } else if bindings.len() < 4 {
                    bindings.push(binding);
                }
                self.capture = None;
                self.capture_version += 1;
                self.clear(at);
                self.down.insert(binding);
                return;
            }
        }
        let before = self.pressed();
        if down {
            self.down.insert(binding);
        } else {
            self.down.remove(&binding);
        }
        let after = self.pressed();
        for i in 0..8 {
            if !before[i] && after[i] {
                self.counts[i] += 1;
                self.strokes.push(Stroke {
                    lane: i,
                    start: at,
                    end: None,
                });
            }
            if before[i] && !after[i] {
                self.release(i, at);
            }
        }
        self.prune(at);
    }
    fn release(&mut self, lane: usize, at: f64) {
        if let Some(s) = self
            .strokes
            .iter_mut()
            .rev()
            .find(|s| s.lane == lane && s.end.is_none())
        {
            s.end = Some(at);
        }
    }
    pub fn clear(&mut self, _at: f64) {
        self.strokes.clear();
        self.down.clear();
    }
    pub fn prune(&mut self, now: f64) {
        let lifetime = (self.config.trail_height as f64 / self.config.speed as f64) * 1000.0
            + self.config.output_delay_ms
            + 1000.0;
        self.strokes
            .retain(|s| s.end.is_none_or(|end| now - end < lifetime));
    }
    pub fn snapshot(&mut self) -> Snapshot {
        let now = self.now();
        self.prune(now);
        let mut strokes = self.strokes.clone();
        let mut pressed = self.pressed();
        if self.demo {
            pressed = [false; 8];
            strokes.clear();
            for step in ((now / 190.0) as i64 - 30).max(0)..=(now / 190.0) as i64 {
                let lane = if step % 9 == 0 {
                    0
                } else {
                    (step % 7 + 1) as usize
                };
                let start = step as f64 * 190.0;
                let end = start
                    + if lane == 0 {
                        220.0
                    } else if step % 5 == 0 {
                        480.0
                    } else {
                        85.0
                    };
                strokes.push(Stroke {
                    lane,
                    start,
                    end: Some(end),
                });
                if now >= start && now < end {
                    pressed[lane] = true;
                }
            }
        }
        Snapshot {
            now,
            config: self.config.clone(),
            strokes,
            pressed,
            counts: self.counts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_key_repeats_do_not_make_extra_strokes() {
        let mut m = Model::new(Config::default());
        let k = Binding::Keyboard { code: 83 };
        m.transition(k, true, 10.0);
        m.transition(k, true, 20.0);
        m.transition(k, false, 110.0);
        assert_eq!(m.strokes.len(), 1);
        assert_eq!(m.strokes[0].end, Some(110.0));
        assert_eq!(m.counts[1], 1);
    }
    #[test]
    fn scratch_two_directions_share_one_continuous_hold() {
        let mut m = Model::new(Config::default());
        let a = Binding::Keyboard { code: 160 };
        let b = Binding::Keyboard { code: 162 };
        m.transition(a, true, 0.0);
        m.transition(b, true, 10.0);
        m.transition(a, false, 20.0);
        assert!(m.pressed()[0]);
        m.transition(b, false, 30.0);
        assert_eq!(m.strokes.len(), 1);
        assert_eq!(m.strokes[0].end, Some(30.0));
    }
    #[test]
    fn history_pruning_keeps_long_holds_and_discards_old_releases() {
        let mut m = Model::new(Config::default());
        m.strokes = vec![
            Stroke {
                lane: 0,
                start: 0.0,
                end: None,
            },
            Stroke {
                lane: 1,
                start: 0.0,
                end: Some(10.0),
            },
        ];
        m.prune(20_000.0);
        assert_eq!(m.strokes.len(), 1);
    }
    #[test]
    fn capture_changes_binding_without_recording_a_hit() {
        let mut m = Model::new(Config::default());
        m.capture = Some((3, 0));
        m.transition(Binding::Keyboard { code: 65 }, true, 0.0);
        assert_eq!(
            m.config.lanes[3].bindings,
            vec![Binding::Keyboard { code: 65 }]
        );
        assert!(m.strokes.is_empty());
    }
}
