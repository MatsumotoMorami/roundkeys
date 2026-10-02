use crate::Shared;
use crate::model::{Config, Snapshot};
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
        Self {
            shared,
            config,
            path,
            port,
            tab: 0,
            selected: 1,
            capture_version: 0,
            dirty: None,
            message: error.unwrap_or_else(|| "设置会自动保存 · 关闭此窗口会停止 OBS 输出".into()),
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
    fn profile(&mut self, name: &str, save: bool) {
        let path = self.path.with_file_name(format!("profile-{name}.json"));
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
            RichText::new("点击绑定后，按手台按钮、键盘键或转动左皿。Esc 取消。")
                .color(Color32::from_gray(170)),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for i in 0..8 {
                let text = if i == 0 {
                    "左皿".into()
                } else {
                    format!("{}", i)
                };
                if ui
                    .add_sized(
                        [if i == 0 { 40.0 } else { 28.0 }, 32.0],
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
            RichText::new(
                "左皿可绑定两个方向；一个按键可同时绑定手台和键盘。Mode 2、Mode 4 可分别保存。",
            )
            .size(13.0)
            .color(Color32::from_gray(165)),
        );
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("保存 Mode 2").clicked() {
                self.profile("mode2", true);
            }
            if ui.button("载入").clicked() {
                self.profile("mode2", false);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("保存 Mode 4").clicked() {
                self.profile("mode4", true);
            }
            if ui.button("载入").clicked() {
                self.profile("mode4", false);
            }
        });
        ui.separator();
        ui.label("检测到的手台");
        for (id, name) in devices {
            ui.label(RichText::new(format!("{name} · {:04X}", id & 0xffff)).size(13.0));
            if ui.small_button("此手台 B1–B7 → 1–7").clicked() {
                for i in 1..8 {
                    let bindings = &mut self.config.lanes[i].bindings;
                    bindings.retain(|b| matches!(b, crate::model::Binding::Keyboard { .. }));
                    bindings.insert(
                        0,
                        crate::model::Binding::ControllerButton {
                            device: *id,
                            button: (i - 1) as u8,
                        },
                    );
                }
                let mut m = self.shared.lock().unwrap();
                let now = m.now();
                m.clear(now);
                self.message = "已设置七键；左皿请分别绑定两个旋转方向".into();
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
        for (value, min, max, label) in [
            (&mut self.config.key_width, 32.0, 120.0, "键宽"),
            (&mut self.config.key_height, 32.0, 120.0, "键高"),
            (&mut self.config.spacing, 2.0, 48.0, "键间距"),
            (&mut self.config.scratch_size, 48.0, 150.0, "左皿大小"),
            (&mut self.config.scratch_gap, 4.0, 90.0, "左皿间距"),
            (&mut self.config.radius, 4.0, 48.0, "圆角"),
            (&mut self.config.trail_height, 120.0, 900.0, "轨迹高度"),
            (&mut self.config.speed, 60.0, 900.0, "流动速度"),
        ] {
            ui.add(egui::Slider::new(value, min..=max).text(label));
        }
        ui.add(egui::Slider::new(&mut self.config.trail_opacity, 0.1..=1.0).text("轨迹透明度"));
        ui.add(egui::Slider::new(&mut self.config.glow, 0.0..=1.0).text("发光强度"));
        ui.checkbox(&mut self.config.stagger, "黑白键错层排列");
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
                    for i in 0..8 {
                        self.config.lanes[i].color = palette[if i == 0 {
                            0
                        } else if i % 2 == 0 {
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
                    ui.label(RichText::new("7K + LEFT SCRATCH").size(12.0).color(ACCENT));
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
                            m.counts = [0; 8];
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
            self.shared.lock().unwrap().config = self.config.clone();
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
    let lanes: [Rect; 8] = std::array::from_fn(|i| {
        let kw = if i == 0 { c.scratch_size } else { c.key_width };
        let kh = if i == 0 { c.scratch_size } else { c.key_height };
        let x = if i == 0 {
            20.0
        } else {
            20.0 + c.scratch_size + c.scratch_gap + (i - 1) as f32 * (c.key_width + c.spacing)
        };
        let y = h
            - 24.0
            - kh
            - if c.stagger && i > 0 && i % 2 == 0 {
                14.0
            } else {
                0.0
            };
        rect(x, y, kw, kh)
    });
    let now = s.now - c.output_delay_ms;
    for (i, lane) in lanes.iter().enumerate() {
        if c.show_guides {
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(lane.min.x, lane.min.y - c.trail_height * scale),
                    lane.right_top(),
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
        let lane = lanes[stroke.lane];
        let boundary = lane.min.y - c.trail_height * scale;
        let end = stroke.end.unwrap_or(now).min(now);
        let top = lane.min.y - ((now - stroke.start) / 1000.0) as f32 * c.speed * scale;
        let bottom = lane.min.y - ((now - end) / 1000.0) as f32 * c.speed * scale;
        if bottom < boundary {
            continue;
        }
        let top = top.max(boundary);
        let bottom = bottom.min(lane.min.y);
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
                if i == 0 {
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
        if i == 0 {
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
                FontId::proportional(if i == 0 { 16.0 } else { 21.0 } * scale),
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
