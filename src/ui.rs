use crate::Shared;
use crate::model::{Config, ScratchSide, Snapshot};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, StrokeKind, Vec2,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const ACCENT: Color32 = Color32::from_rgb(182, 170, 255);
pub struct App {
    shared: Shared,
    config: Config,
    path: PathBuf,
    port: u16,
    tab: u8,
    selected: usize,
    capture_version: u64,
    dirty: Option<Instant>,
    message: String,
    presets: Vec<u32>,
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Shared,
        path: PathBuf,
        port: u16,
        error: Option<String>,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        let mut fonts = egui::FontDefinitions::default();
        for path in ["C:/Windows/Fonts/msyh.ttc", "C:/Windows/Fonts/meiryo.ttc"] {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .push("cjk".into());
                break;
            }
        }
        ctx.set_fonts(fonts);
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(12.0, 8.0);
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(16, 20, 32);
        style.visuals.window_fill = Color32::from_rgb(23, 28, 43);
        style.visuals.selection.bg_fill = Color32::from_rgb(76, 66, 120);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
        for w in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.noninteractive,
        ] {
            w.corner_radius = CornerRadius::same(10);
            w.weak_bg_fill = Color32::from_rgb(34, 40, 58);
        }
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 40, 58);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 53, 80);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        ctx.set_style(style);
        let config = shared.lock().unwrap().config.clone();
        let (presets, preset_error) = match crate::presets::discover(&path) {
            Ok(ids) => (ids, None),
            Err(e) => (vec![1, 2], Some(format!("预设读取失败：{e}"))),
        };
        Self {
            presets,
            shared,
            config,
            path,
            port,
            tab: 0,
            selected: 1,
            capture_version: 0,
            dirty: None,
            message: error
                .or(preset_error)
                .unwrap_or_else(|| "设置会自动保存 · 关闭此窗口会停止 OBS 输出".into()),
        }
    }
    fn save(&mut self) {
        match self.config.save(&self.path) {
            Ok(()) => {
                self.message = "已保存设置".into();
                self.dirty = None;
            }
            Err(e) => {
                self.message = format!("保存失败：{e}");
                self.dirty = None;
            }
        }
    }
    fn profile(&mut self, id: u32, save: bool) {
        let path = crate::presets::path(&self.path, id);
        let name = format!("预设{id}");
        if save {
            self.message = match self.config.save(&path) {
                Ok(()) => format!("已保存 {name} 配置"),
                Err(e) => format!("保存失败：{e}"),
            };
        } else if !path.exists() {
            self.message = format!("尚未保存 {name} 配置");
        } else {
            match Config::load(&path) {
                Ok(c) => {
                    self.config = c;
                    let mut m = self.shared.lock().unwrap();
                    let now = m.now();
                    m.clear(now);
                    m.capture = None;
                    self.dirty = Some(Instant::now());
                    self.message = format!("已载入 {name}");
                }
                Err(e) => self.message = format!("读取失败：{e}"),
            }
        }
    }
    fn bindings(
        &mut self,
        ui: &mut egui::Ui,
        capture: Option<(usize, usize)>,
        devices: &[(u32, String)],
    ) {
        ui.heading("输入绑定");
        ui.label(
            RichText::new("点击绑定后，按手台按钮、键盘键或转动皿。Esc 取消。")
                .color(Color32::from_gray(170)),
        );
        ui.add_space(6.0);
        self.selected = self.selected.min(self.config.lanes.len() - 1);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for i in 0..self.config.lanes.len() {
                let scratch = self.config.is_scratch(i);
                let text = if scratch {
                    format!(
                        "{}皿{}",
                        if self.config.scratch_sides[i] == ScratchSide::Left {
                            "左"
                        } else {
                            "右"
                        },
                        i + 1
                    )
                } else {
                    (i - self.config.scratch_sides.len() + 1).to_string()
                };
                if ui
                    .add_sized(
                        [if scratch { 58.0 } else { 28.0 }, 32.0],
                        egui::Button::new(text).selected(self.selected == i),
                    )
                    .clicked()
                {
                    self.selected = i;
                }
            }
        });
        ui.add_space(8.0);
        let selected = self.selected;
        let lane = &mut self.config.lanes[selected];
        ui.horizontal(|ui| {
            ui.label("显示文字");
            ui.add(egui::TextEdit::singleline(&mut lane.label).desired_width(150.0));
        });
        ui.horizontal(|ui| {
            ui.label("颜色");
            ui.color_edit_button_srgba_unmultiplied(&mut lane.color);
        });
        let mut remove = None;
        for slot in 0..lane.bindings.len() {
            ui.horizontal(|ui| {
                let waiting = capture == Some((selected, slot));
                let label = if waiting {
                    "等待输入…".into()
                } else {
                    lane.bindings[slot].label()
                };
                if ui
                    .add_sized([230.0, 38.0], egui::Button::new(label).selected(waiting))
                    .clicked()
                {
                    self.shared.lock().unwrap().capture = Some((selected, slot));
                }
                if ui.small_button("×").clicked() {
                    remove = Some(slot);
                }
            });
        }
        if let Some(slot) = remove {
            lane.bindings.remove(slot);
            let mut m = self.shared.lock().unwrap();
            let now = m.now();
            m.capture = None;
            m.clear(now);
        }
        if lane.bindings.len() < 4 && ui.button("＋ 添加另一种输入").clicked() {
            self.shared.lock().unwrap().capture = Some((selected, lane.bindings.len()));
        }
        if capture.is_some() && ui.button("取消绑定").clicked() {
            self.shared.lock().unwrap().capture = None;
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("每个皿可绑定两个方向；一个按键可同时绑定手台和键盘。")
                .size(13.0)
                .color(Color32::from_gray(165)),
        );
        ui.separator();
        ui.label("预设");
        for id in self.presets.clone() {
            ui.push_id(id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("预设{id}"));
                    if ui.button("保存当前设置").clicked() {
                        self.profile(id, true);
                    }
                    if ui.button("载入").clicked() {
                        self.profile(id, false);
                    }
                });
            });
        }
        if ui.button("＋ 添加预设").clicked() {
            self.message = match crate::presets::add(&self.path, &mut self.presets, &self.config) {
                Ok(id) => format!("已添加预设{id}，并保存当前设置"),
                Err(e) => format!("添加失败：{e}"),
            };
        }
        ui.separator();
        ui.label("检测到的手台");
        for (id, name) in devices {
            ui.label(RichText::new(format!("{name} · {:04X}", id & 0xffff)).size(13.0));
            if ui
                .small_button(format!("此手台 B1–B{} → 按键", self.config.key_count))
                .clicked()
            {
                for key in 0..self.config.key_count {
                    let i = self.config.scratch_sides.len() + key;
                    let bindings = &mut self.config.lanes[i].bindings;
                    bindings.retain(|b| matches!(b, crate::model::Binding::Keyboard { .. }));
                    bindings.insert(
                        0,
                        crate::model::Binding::ControllerButton {
                            device: *id,
                            button: key as u8,
                        },
                    );
                }
                let mut m = self.shared.lock().unwrap();
                let now = m.now();
                m.clear(now);
                self.message = "已设置按键；每个皿请分别绑定两个旋转方向".into();
            }
        }
        if devices.is_empty() {
            ui.label("未连接 HID 手台");
        }
        ui.add_space(8.0);
        ui.label("转盘感应");
        ui.add(
            egui::Slider::new(&mut self.config.scratch_threshold, 0.00001..=0.05)
                .logarithmic(true)
                .text("阈值"),
        );
        ui.add(
            egui::Slider::new(&mut self.config.scratch_release_ms, 16.0..=200.0).text("松开 / ms"),
        );
    }
    fn appearance(&mut self, ui: &mut egui::Ui) {
        ui.heading("布局与轨迹");
        let mut keys = self.config.key_count;
        let mut scratches = self.config.scratch_sides.len();
        let mut sides = self.config.scratch_sides.clone();
        ui.horizontal(|ui| {
            ui.label("按键数");
            ui.add(egui::DragValue::new(&mut keys).range(1..=32));
            ui.label("皿数");
            ui.add(egui::DragValue::new(&mut scratches).range(0..=4));
        });
        sides.resize(scratches, ScratchSide::Right);
        for (i, side) in sides.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("皿 {}", i + 1));
                ui.selectable_value(side, ScratchSide::Left, "左侧");
                ui.selectable_value(side, ScratchSide::Right, "右侧");
            });
        }
        ui.horizontal_wrapped(|ui| {
            for (label, k, sides_preset) in [
                ("7K 左皿", 7, vec![ScratchSide::Left]),
                ("7K 右皿", 7, vec![ScratchSide::Right]),
                ("14K 双皿", 14, vec![ScratchSide::Left, ScratchSide::Right]),
                ("无皿", keys, vec![]),
            ] {
                if ui.button(label).clicked() {
                    keys = k;
                    sides = sides_preset;
                }
            }
        });
        if keys != self.config.key_count || sides != self.config.scratch_sides {
            self.config.set_layout(keys, sides);
            self.selected = self.selected.min(self.config.lanes.len() - 1);
        }
        ui.label(RichText::new("每个皿可独立绑定输入；减少数量会移除对应绑定。").size(12.0));
        ui.separator();
        let [width, height] = self.config.dimensions();
        let fixed = self.config.control_height() + 52.0;
        let mut ratio = width / height;
        ui.horizontal(|ui| {
            ui.label("长宽比（宽 ÷ 高）");
            if ui
                .add(
                    egui::DragValue::new(&mut ratio)
                        .range((width / (fixed + 2000.0))..=(width / (fixed + 120.0)))
                        .speed(0.01)
                        .max_decimals(3),
                )
                .changed()
            {
                self.config.set_aspect_ratio(ratio);
            }
        });
        ui.horizontal(|ui| {
            for (label, ratio) in [
                ("16:9", 16.0 / 9.0),
                ("4:3", 4.0 / 3.0),
                ("1:1", 1.0),
                ("9:16", 9.0 / 16.0),
            ] {
                if ui.button(label).clicked() {
                    self.config.set_aspect_ratio(ratio);
                }
            }
        });
        ui.label(egui::RichText::new("改变轨迹区域比例；按键和皿不会拉伸。").size(12.0));
        for (value, min, max, label) in [
            (&mut self.config.key_width, 32.0, 120.0, "键宽"),
            (&mut self.config.key_height, 32.0, 120.0, "键高"),
            (&mut self.config.spacing, 2.0, 48.0, "键间距"),
            (&mut self.config.scratch_size, 48.0, 150.0, "皿大小"),
            (&mut self.config.scratch_gap, 4.0, 90.0, "皿间距"),
            (&mut self.config.radius, 4.0, 48.0, "圆角"),
            (&mut self.config.trail_height, 120.0, 2000.0, "轨迹高度"),
            (&mut self.config.speed, 60.0, 900.0, "流动速度"),
        ] {
            ui.add(egui::Slider::new(value, min..=max).text(label));
        }
        ui.add(egui::Slider::new(&mut self.config.trail_opacity, 0.1..=1.0).text("轨迹透明度"));
        ui.add(egui::Slider::new(&mut self.config.glow, 0.0..=1.0).text("发光强度"));
        ui.checkbox(&mut self.config.stagger, "黑白键错层排列");
        ui.horizontal(|ui| {
            ui.label("音符流起点");
            ui.selectable_value(&mut self.config.trail_stagger, true, "错落");
            ui.selectable_value(&mut self.config.trail_stagger, false, "相同起点");
        });
        ui.checkbox(&mut self.config.show_labels, "显示按键文字");
        ui.checkbox(&mut self.config.show_guides, "显示轨道底色");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("背景颜色");
            ui.color_edit_button_srgba_unmultiplied(&mut self.config.background);
        });
        let mut alpha = self.config.background[3] as u32;
        if ui
            .add(egui::Slider::new(&mut alpha, 0..=255).text("背景不透明度"))
            .changed()
        {
            self.config.background[3] = alpha as u8;
        }
        ui.label(
            RichText::new("不透明度为 0 时，OBS 输出透明背景。")
                .size(13.0)
                .color(Color32::from_gray(165)),
        );
        ui.separator();
        ui.label("配色预设");
        ui.horizontal(|ui| {
            for (name, palette) in [
                (
                    "雾紫",
                    [
                        [255, 163, 192, 255],
                        [160, 224, 228, 255],
                        [170, 158, 255, 255],
                    ],
                ),
                (
                    "冰蓝",
                    [
                        [246, 189, 132, 255],
                        [158, 216, 255, 255],
                        [112, 165, 239, 255],
                    ],
                ),
                (
                    "樱桃",
                    [
                        [255, 184, 203, 255],
                        [248, 217, 223, 255],
                        [224, 128, 164, 255],
                    ],
                ),
            ] {
                if ui.button(name).clicked() {
                    for i in 0..self.config.lanes.len() {
                        self.config.lanes[i].color = palette[if self.config.is_scratch(i) {
                            0
                        } else if (i - self.config.scratch_sides.len()) % 2 == 1 {
                            2
                        } else {
                            1
                        }];
                    }
                }
            }
        });
    }
    fn obs(&mut self, ui: &mut egui::Ui) {
        ui.heading("连接 OBS");
        ui.add_space(8.0);
        ui.label("1. 添加「浏览器」来源");
        ui.label("2. 复制下方地址到 URL");
        let url = format!("http://127.0.0.1:{}/overlay", self.port);
        ui.add(
            egui::TextEdit::singleline(&mut url.clone())
                .desired_width(280.0)
                .interactive(false),
        );
        if ui.button("复制浏览器源地址").clicked() {
            ui.ctx().copy_text(url);
            self.message = "已复制 OBS 地址".into();
        }
        let dim = self.config.dimensions();
        ui.label(format!(
            "3. 宽 {} × 高 {}，帧率 60",
            dim[0].ceil() as u32,
            dim[1].ceil() as u32
        ));
        ui.label("4. 保持 RoundKeys 运行");
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "尺寸随布局变化。修改布局后，将这里的尺寸同步到 OBS，或保持来源尺寸并缩放显示。",
            )
            .size(13.0)
            .color(Color32::from_gray(170)),
        );
        ui.separator();
        ui.label("与游戏画面同步");
        ui.add(
            egui::Slider::new(&mut self.config.output_delay_ms, 0.0..=3000.0).text("显示延迟 / ms"),
        );
        ui.label(
            RichText::new("只延迟按键显示，不改变游戏输入、声音或判定。")
                .size(13.0)
                .color(Color32::from_gray(165)),
        );
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let (
            snapshot,
            capture,
            devices,
            input_status,
            last_input,
            server_status,
            clients,
            demo,
            paused,
            version,
        ) = {
            let mut m = self.shared.lock().unwrap();
            (
                m.snapshot(),
                m.capture,
                m.devices.clone(),
                m.input_status.clone(),
                m.last_input.clone(),
                m.server_status.clone(),
                m.clients,
                m.demo,
                m.paused,
                m.capture_version,
            )
        };
        if version != self.capture_version {
            self.config = snapshot.config.clone();
            self.capture_version = version;
            self.dirty = Some(Instant::now());
            self.message = "输入绑定已更新".into();
        }
        let before = serde_json::to_string(&self.config).unwrap();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(16, 20, 32))
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("RoundKeys")
                            .size(28.0)
                            .strong()
                            .color(Color32::from_rgb(235, 229, 255)),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{}K + {} SCRATCH",
                            self.config.key_count,
                            self.config.scratch_sides.len()
                        ))
                        .size(12.0)
                        .color(ACCENT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("保存设置").clicked() {
                            self.save();
                        }
                        ui.label(
                            RichText::new(if clients > 0 {
                                format!("● OBS 已连接 · {clients}")
                            } else {
                                "○ 等待 OBS".into()
                            })
                            .color(if clients > 0 {
                                Color32::from_rgb(160, 224, 190)
                            } else {
                                Color32::from_gray(155)
                            }),
                        );
                    });
                });
            });
        egui::TopBottomPanel::bottom("footer")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 10)))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(&self.message)
                        .size(12.0)
                        .color(Color32::from_gray(170)),
                );
            });
        egui::SidePanel::right("settings")
            .exact_width(335.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(22, 27, 42))
                    .inner_margin(18.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, name) in ["绑定", "外观", "OBS"].iter().enumerate() {
                        if ui.selectable_label(self.tab == i as u8, *name).clicked() {
                            self.tab = i as u8;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    0 => self.bindings(ui, capture, &devices),
                    1 => self.appearance(ui),
                    _ => self.obs(ui),
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(16, 20, 32))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("实时预览");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.selectable_label(demo, "演示动画").clicked() {
                            let mut m = self.shared.lock().unwrap();
                            m.demo = !m.demo;
                        }
                        if ui
                            .button(if paused {
                                "继续输入"
                            } else {
                                "暂停输入"
                            })
                            .clicked()
                        {
                            let mut m = self.shared.lock().unwrap();
                            let now = m.now();
                            m.clear(now);
                            m.paused = !m.paused;
                        }
                        if ui.button("清空轨迹").clicked() {
                            let mut m = self.shared.lock().unwrap();
                            let now = m.now();
                            m.clear(now);
                            m.counts.fill(0);
                        }
                    });
                });
                ui.label(
                    RichText::new(if demo {
                        "演示中 · 此动画也会输出到 OBS"
                    } else {
                        &input_status
                    })
                    .size(12.0)
                    .color(if demo {
                        Color32::from_rgb(255, 191, 145)
                    } else {
                        Color32::from_gray(160)
                    }),
                );
                ui.add_space(12.0);
                let available = Vec2::new(
                    ui.available_width(),
                    (ui.available_height() - 75.0).max(150.0),
                );
                let mut current = snapshot.clone();
                current.config = self.config.clone();
                if let Some(selected) = paint_overlay(ui, &current, available, self.selected) {
                    self.selected = selected;
                    self.tab = 0;
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("最近输入")
                            .size(12.0)
                            .color(Color32::from_gray(140)),
                    );
                    ui.label(RichText::new(&last_input).color(ACCENT));
                });
                ui.label(
                    RichText::new(&server_status)
                        .size(12.0)
                        .color(Color32::from_gray(145)),
                );
            });
        self.config.normalize();
        if serde_json::to_string(&self.config).unwrap() != before {
            self.shared
                .lock()
                .unwrap()
                .apply_config(self.config.clone());
            self.dirty = Some(Instant::now());
        }
        if self
            .dirty
            .is_some_and(|t| t.elapsed() > Duration::from_millis(650))
        {
            self.save();
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        if self.dirty.is_some() {
            self.save();
        }
    }
}

fn color(c: [u8; 4], alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c[0],
        c[1],
        c[2],
        (c[3] as f32 * alpha.clamp(0.0, 1.0)) as u8,
    )
}
fn paint_overlay(ui: &mut egui::Ui, s: &Snapshot, size: Vec2, selected: usize) -> Option<usize> {
    let (panel, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(panel);
    p.rect_filled(panel, 22.0, Color32::from_rgb(12, 16, 26));
    let c = &s.config;
    let [w, h] = c.dimensions();
    let scale = (size.x / w).min(size.y / h) * 0.96;
    let origin = panel.center() - Vec2::new(w, h) * scale / 2.0;
    let rect = |x: f32, y: f32, w: f32, h: f32| {
        Rect::from_min_size(origin + Vec2::new(x, y) * scale, Vec2::new(w, h) * scale)
    };
    p.rect_filled(rect(0.0, 0.0, w, h), 24.0 * scale, color(c.background, 1.0));
    let lanes: Vec<Rect> = (0..c.lanes.len())
        .map(|i| {
            let [x, y, w, h] = c.lane_geometry(i);
            rect(x, y, w, h)
        })
        .collect();
    let flow_origins: Vec<f32> = (0..c.lanes.len())
        .map(|i| origin.y + c.trail_origin(i) * scale)
        .collect();
    let now = s.now - c.output_delay_ms;
    for (i, lane) in lanes.iter().enumerate() {
        if c.show_guides {
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(lane.min.x, flow_origins[i] - c.trail_height * scale),
                    Pos2::new(lane.max.x, flow_origins[i]),
                ),
                10.0,
                color(c.lanes[i].color, 0.045),
            );
        }
    }
    for stroke in &s.strokes {
        if now < stroke.start {
            continue;
        }
        let Some(&lane) = lanes.get(stroke.lane) else {
            continue;
        };
        let flow_y = flow_origins[stroke.lane];
        let boundary = flow_y - c.trail_height * scale;
        let end = stroke.end.unwrap_or(now).min(now);
        let top = flow_y - ((now - stroke.start) / 1000.0) as f32 * c.speed * scale;
        let bottom = flow_y - ((now - end) / 1000.0) as f32 * c.speed * scale;
        if bottom < boundary {
            continue;
        }
        let top = (top
            + if c.is_scratch(stroke.lane) {
                2.0 * scale
            } else {
                0.0
            })
        .max(boundary);
        let bottom = (bottom
            - if c.is_scratch(stroke.lane) && stroke.end.is_some_and(|end| end <= now) {
                2.0 * scale
            } else {
                0.0
            })
        .min(flow_y);
        let fade = ((bottom - boundary) / (70.0 * scale)).clamp(0.0, 1.0);
        p.rect_filled(
            Rect::from_min_max(
                Pos2::new(lane.min.x + 5.0 * scale, top),
                Pos2::new(lane.max.x - 5.0 * scale, bottom.max(top + 4.0 * scale)),
            ),
            c.radius * scale * 0.65,
            color(c.lanes[stroke.lane].color, c.trail_opacity * fade),
        );
    }
    let mut clicked = None;
    for (i, lane) in lanes.iter().enumerate() {
        let active = s
            .strokes
            .iter()
            .any(|st| st.lane == i && now >= st.start && st.end.is_none_or(|end| now < end));
        let col = c.lanes[i].color;
        let fill = if active {
            color(col, 0.93)
        } else {
            Color32::from_rgb(23, 29, 44)
        };
        let border = Stroke::new(
            if active { 2.0 } else { 1.4 } * scale,
            color(col, if active { 1.0 } else { 0.48 }),
        );
        if active && c.glow > 0.0 {
            for ring in (1..=4).rev() {
                let rr = lane.expand(ring as f32 * 2.0 * scale);
                if c.is_scratch(i) {
                    p.circle_filled(rr.center(), rr.width() / 2.0, color(col, 0.04 * c.glow));
                } else {
                    p.rect_filled(
                        rr,
                        c.radius * scale + ring as f32 * 2.0,
                        color(col, 0.04 * c.glow),
                    );
                }
            }
        }
        if c.is_scratch(i) {
            p.circle(
                lane.center(),
                lane.width() / 2.0 - 2.0 * scale,
                fill,
                border,
            );
            p.circle_stroke(
                lane.center(),
                lane.width() / 2.0 - 11.0 * scale,
                Stroke::new(scale, color(col, 0.28)),
            );
            let angle = if active { now as f32 / 180.0 } else { 0.0 };
            let v = Vec2::new(angle.sin(), -angle.cos());
            let r = lane.width() / 2.0;
            p.line_segment(
                [
                    lane.center() + v * (r - 17.0 * scale),
                    lane.center() + v * (r - 28.0 * scale),
                ],
                Stroke::new(4.0 * scale, color(col, 0.75)),
            );
        } else {
            let key = Rect::from_min_max(
                lane.min + Vec2::new(0.0, if active { 3.0 * scale } else { 0.0 }),
                lane.max,
            );
            p.rect(key, c.radius * scale, fill, border, StrokeKind::Inside);
        }
        if c.show_labels {
            p.text(
                lane.center(),
                egui::Align2::CENTER_CENTER,
                &c.lanes[i].label,
                FontId::proportional(if c.is_scratch(i) { 16.0 } else { 21.0 } * scale),
                if active {
                    Color32::from_rgb(23, 28, 43)
                } else {
                    color(col, 0.95)
                },
            );
        }
        if i == selected {
            p.rect_stroke(
                lane.expand(5.0 * scale),
                c.radius * scale + 5.0 * scale,
                Stroke::new(scale, Color32::from_rgba_unmultiplied(210, 205, 255, 85)),
                StrokeKind::Outside,
            );
        }
        if ui
            .interact(*lane, ui.id().with(i), egui::Sense::click())
            .clicked()
        {
            clicked = Some(i);
        }
    }
    clicked
}
