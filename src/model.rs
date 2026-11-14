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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchSide {
    Left,
    Right,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lanes: Vec<Lane>,
    pub key_count: usize,
    pub scratch_sides: Vec<ScratchSide>,
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
    pub trail_stagger: bool,
    pub output_delay_ms: f64,
    pub scratch_threshold: f64,
    pub scratch_release_ms: f64,
}
impl Default for Config {
    fn default() -> Self {
        let codes = [160, 83, 68, 70, 32, 74, 75, 76];
        Self {
            key_count: 7,
            scratch_sides: vec![ScratchSide::Left],
            lanes: (0..8)
                .map(|i| Lane {
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
                })
                .collect(),
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
            trail_stagger: true,
            output_delay_ms: 0.0,
            scratch_threshold: 0.001,
            scratch_release_ms: 65.0,
        }
    }
}
impl Config {
    pub fn normalize(&mut self) {
        self.key_count = self.key_count.clamp(1, 32);
        self.scratch_sides.truncate(4);
        let total = self.key_count + self.scratch_sides.len();
        self.lanes.truncate(total);
        while self.lanes.len() < total {
            self.lanes.push(self.default_lane(self.lanes.len()));
        }
        self.key_width = finite(self.key_width, 62.0).clamp(32.0, 120.0);
        self.key_height = finite(self.key_height, 68.0).clamp(32.0, 120.0);
        self.spacing = finite(self.spacing, 10.0).clamp(2.0, 48.0);
        self.scratch_size = finite(self.scratch_size, 92.0).clamp(48.0, 150.0);
        self.scratch_gap = finite(self.scratch_gap, 26.0).clamp(4.0, 90.0);
        self.radius = finite(self.radius, 16.0).clamp(4.0, 48.0);
        self.trail_height = finite(self.trail_height, 380.0).clamp(120.0, 2000.0);
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
    fn default_lane(&self, index: usize) -> Lane {
        if self.is_scratch(index) {
            Lane {
                label: format!("SCR{}", index + 1),
                color: [255, 163, 192, 255],
                bindings: vec![],
            }
        } else {
            let key = index - self.scratch_sides.len();
            let codes = [83, 68, 70, 32, 74, 75, 76];
            Lane {
                label: (key + 1).to_string(),
                color: if key.is_multiple_of(2) {
                    [160, 224, 228, 255]
                } else {
                    [170, 158, 255, 255]
                },
                bindings: codes
                    .get(key)
                    .map(|&code| vec![Binding::Keyboard { code }])
                    .unwrap_or_default(),
            }
        }
    }
    pub fn is_scratch(&self, lane: usize) -> bool {
        lane < self.scratch_sides.len()
    }
    pub fn set_layout(&mut self, keys: usize, sides: Vec<ScratchSide>) {
        let old_scratch = self.scratch_sides.len();
        let old = self.lanes.clone();
        self.key_count = keys.clamp(1, 32);
        self.scratch_sides = sides.into_iter().take(4).collect();
        self.lanes = (0..self.key_count + self.scratch_sides.len())
            .map(|i| {
                let previous = if self.is_scratch(i) {
                    (i < old_scratch).then_some(i)
                } else {
                    Some(old_scratch + i - self.scratch_sides.len())
                };
                previous
                    .and_then(|j| old.get(j))
                    .cloned()
                    .unwrap_or_else(|| self.default_lane(i))
            })
            .collect();
    }
    pub fn control_height(&self) -> f32 {
        if self.scratch_sides.is_empty() {
            self.key_height
        } else {
            self.key_height.max(self.scratch_size)
        }
    }
    pub fn dimensions(&self) -> [f32; 2] {
        [
            40.0 + self.key_width * self.key_count as f32
                + self.spacing * self.key_count.saturating_sub(1) as f32
                + (self.scratch_size + self.scratch_gap) * self.scratch_sides.len() as f32,
            self.trail_height + self.control_height() + 52.0,
        ]
    }
    pub fn lane_geometry(&self, lane: usize) -> [f32; 4] {
        let left = self
            .scratch_sides
            .iter()
            .filter(|&&s| s == ScratchSide::Left)
            .count();
        let key_start = 20.0 + left as f32 * (self.scratch_size + self.scratch_gap);
        let (x, w, h, raised) = if self.is_scratch(lane) {
            let side = self.scratch_sides[lane];
            let rank = self.scratch_sides[..lane]
                .iter()
                .filter(|&&s| s == side)
                .count();
            let x = if side == ScratchSide::Left {
                20.0 + rank as f32 * (self.scratch_size + self.scratch_gap)
            } else {
                key_start
                    + self.key_count as f32 * self.key_width
                    + self.key_count.saturating_sub(1) as f32 * self.spacing
                    + self.scratch_gap
                    + rank as f32 * (self.scratch_size + self.scratch_gap)
            };
            (x, self.scratch_size, self.scratch_size, 0.0)
        } else {
            let key = lane - self.scratch_sides.len();
            (
                key_start + key as f32 * (self.key_width + self.spacing),
                self.key_width,
                self.key_height,
                if self.stagger && key % 2 == 1 {
                    14.0
                } else {
                    0.0
                },
            )
        };
        [x, self.dimensions()[1] - 24.0 - h - raised, w, h]
    }
    pub fn trail_origin(&self, lane: usize) -> f32 {
        if self.trail_stagger {
            self.lane_geometry(lane)[1]
        } else {
            self.dimensions()[1]
                - 24.0
                - self
                    .control_height()
                    .max(self.key_height + if self.stagger { 14.0 } else { 0.0 })
        }
    }
    pub fn set_aspect_ratio(&mut self, ratio: f32) {
        let [width, height] = self.dimensions();
        let ratio = finite(ratio, width / height).max(0.01);
        self.set_output_height(width / ratio);
    }
    pub fn set_output_height(&mut self, height: f32) {
        let fixed = self.control_height() + 52.0;
        self.trail_height = (finite(height, self.dimensions()[1]) - fixed).clamp(120.0, 2000.0);
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
    pub pressed: Vec<bool>,
    pub counts: Vec<u64>,
}
pub struct Model {
    pub config: Config,
    pub epoch: Instant,
    pub strokes: Vec<Stroke>,
    pub counts: Vec<u64>,
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
    pub fn new(mut config: Config) -> Self {
        config.normalize();
        let count = config.lanes.len();
        Self {
            config,
            epoch: Instant::now(),
            strokes: vec![],
            counts: vec![0; count],
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
    pub fn apply_config(&mut self, mut config: Config) {
        config.normalize();
        if config.key_count != self.config.key_count
            || config.scratch_sides != self.config.scratch_sides
        {
            self.clear(self.now());
            self.capture = None;
            self.counts = vec![0; config.lanes.len()];
        }
        self.config = config;
    }
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64() * 1000.0
    }
    pub fn pressed(&self) -> Vec<bool> {
        (0..self.config.lanes.len())
            .map(|i| {
                !self.paused
                    && self.config.lanes[i]
                        .bindings
                        .iter()
                        .any(|b| self.down.contains(b))
            })
            .collect()
    }
    pub fn transition(&mut self, binding: Binding, down: bool, at: f64) {
        if down && !self.down.contains(&binding) {
            self.last_input = binding.label();
        }
        if down
            && let Some((lane, slot)) = self
                .capture
                .filter(|(lane, _)| *lane < self.config.lanes.len())
        {
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
        // SCR bindings represent turn directions: a new direction starts a new note.
        if down && !self.down.contains(&binding) {
            for (i, &pressed) in before
                .iter()
                .enumerate()
                .take(self.config.scratch_sides.len())
            {
                if pressed && self.config.lanes[i].bindings.contains(&binding) {
                    self.release(i, at);
                    self.counts[i] += 1;
                    self.strokes.push(Stroke {
                        lane: i,
                        start: at,
                        end: None,
                    });
                }
            }
        }
        if down {
            self.down.insert(binding);
        } else {
            self.down.remove(&binding);
        }
        let after = self.pressed();
        for i in 0..self.config.lanes.len() {
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
            pressed = vec![false; self.config.lanes.len()];
            strokes.clear();
            for step in ((now / 190.0) as i64 - 30).max(0)..=(now / 190.0) as i64 {
                let lane = step as usize % self.config.lanes.len();
                let start = step as f64 * 190.0;
                let end = start
                    + if self.config.is_scratch(lane) {
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
            counts: self.counts.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_config_keeps_bindings_and_right_scratch_keeps_identity() {
        let old = Config::default();
        let mut value = serde_json::to_value(&old).unwrap();
        value.as_object_mut().unwrap().remove("key_count");
        value.as_object_mut().unwrap().remove("scratch_sides");
        let mut c: Config = serde_json::from_value(value).unwrap();
        c.normalize();
        assert_eq!(c.lanes[1].bindings, old.lanes[1].bindings);
        c.set_layout(7, vec![ScratchSide::Right]);
        assert_eq!(c.lanes[0].bindings, old.lanes[0].bindings);
        assert!(c.lane_geometry(0)[0] > c.lane_geometry(7)[0]);
        assert_eq!(c.dimensions(), old.dimensions());
    }
    #[test]
    fn changing_lane_counts_preserves_keys_and_has_no_overlaps() {
        let mut c = Config::default();
        let first = c.lanes[1].bindings.clone();
        c.set_layout(14, vec![ScratchSide::Left, ScratchSide::Right]);
        assert_eq!(c.lanes[2].bindings, first);
        assert_eq!(c.lanes.len(), 16);
        let mut geometry: Vec<_> = (0..c.lanes.len()).map(|i| c.lane_geometry(i)).collect();
        geometry.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        for pair in geometry.windows(2) {
            assert!(pair[0][0] + pair[0][2] < pair[1][0]);
        }
        assert!(
            geometry.last().unwrap()[0] + geometry.last().unwrap()[2] <= c.dimensions()[0] - 20.0
        );
        c.set_layout(5, vec![]);
        assert_eq!(c.lanes[0].bindings, first);
        assert_eq!(c.lanes.len(), 5);
        assert_eq!(c.dimensions()[0], 390.0);
    }
    #[test]
    fn multiple_scratches_split_independently_and_layout_change_clears_holds() {
        let mut c = Config::default();
        c.set_layout(4, vec![ScratchSide::Left, ScratchSide::Right]);
        c.lanes[1].bindings = vec![
            Binding::Keyboard { code: 161 },
            Binding::Keyboard { code: 163 },
        ];
        let mut m = Model::new(c);
        m.transition(Binding::Keyboard { code: 160 }, true, 0.0);
        m.transition(Binding::Keyboard { code: 161 }, true, 10.0);
        m.transition(Binding::Keyboard { code: 163 }, true, 20.0);
        assert_eq!(m.counts[..2], [1, 2]);
        assert!(
            m.strokes
                .iter()
                .find(|s| s.lane == 0)
                .unwrap()
                .end
                .is_none()
        );
        let mut c = m.config.clone();
        c.set_layout(32, vec![]);
        m.apply_config(c);
        assert!(m.strokes.is_empty());
        assert!(m.down.is_empty());
        assert_eq!(m.snapshot().pressed.len(), 32);
        assert_eq!(m.counts.len(), 32);
    }
    #[test]
    fn custom_height_preserves_key_geometry_and_survives_save_format() {
        let mut c = Config::default();
        let width = c.dimensions()[0];
        c.set_output_height(1080.0);
        assert_eq!(c.dimensions(), [width, 1080.0]);
        assert_eq!(c.key_height, 68.0);
        assert_eq!(c.speed, 270.0);
        let mut restored: Config =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        restored.normalize();
        assert_eq!(restored.dimensions()[1], 1080.0);
        restored.set_output_height(0.0);
        assert_eq!(restored.trail_height, 120.0);
    }
    #[test]
    fn aspect_ratio_changes_layout_without_stretching_controls() {
        let mut c = Config::default();
        c.set_aspect_ratio(16.0 / 9.0);
        let [w, h] = c.dimensions();
        assert!((w / h - 16.0 / 9.0).abs() < 0.0001);
        assert_eq!(c.scratch_size, 92.0);
        assert_eq!(c.key_width, 62.0);
        c.set_aspect_ratio(1.0);
        assert_eq!(c.dimensions(), [652.0, 652.0]);
    }
    #[test]
    fn trail_origin_can_align_without_changing_key_stagger() {
        let mut c = Config::default();
        assert_ne!(c.trail_origin(1), c.trail_origin(2));
        c.trail_stagger = false;
        for i in 1..8 {
            assert_eq!(c.trail_origin(0), c.trail_origin(i));
        }
        assert!(c.stagger);
        let restored: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert!(!restored.trail_stagger);
        let old: Config = serde_json::from_str("{}").unwrap();
        assert!(old.trail_stagger);
    }
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
    fn scratch_reversal_splits_even_when_direction_inputs_overlap() {
        let mut m = Model::new(Config::default());
        let a = Binding::Keyboard { code: 160 };
        let b = Binding::Keyboard { code: 162 };
        m.transition(a, true, 0.0);
        m.transition(b, true, 10.0);
        m.transition(a, false, 20.0);
        assert!(m.pressed()[0]);
        m.transition(b, false, 30.0);
        assert_eq!(m.strokes.len(), 2);
        assert_eq!(m.strokes[0].end, Some(10.0));
        assert_eq!(m.strokes[1].start, 10.0);
        assert_eq!(m.strokes[1].end, Some(30.0));
        assert_eq!(m.counts[0], 2);
    }
    #[test]
    fn scratch_axis_reversal_splits_but_same_direction_reports_do_not() {
        let mut m = Model::new(Config::default());
        let a = Binding::ControllerAxis {
            device: 1,
            axis: 0,
            direction: 1,
        };
        let b = Binding::ControllerAxis {
            device: 1,
            axis: 0,
            direction: -1,
        };
        m.config.lanes[0].bindings = vec![a, b];
        m.transition(a, true, 0.0);
        m.transition(a, true, 50.0);
        m.transition(b, true, 100.0);
        m.transition(a, false, 100.0);
        m.transition(b, true, 110.0);
        m.transition(b, false, 150.0);
        assert_eq!(m.strokes.len(), 2);
        assert_eq!(m.strokes[0].end, Some(100.0));
        assert_eq!(m.strokes[1].start, 100.0);
        assert_eq!(m.strokes[1].end, Some(150.0));
        assert_eq!(m.counts[0], 2);
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
