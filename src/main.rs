#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod input;
mod model;
mod presets;
mod server;
mod ui;

use model::{Config, Model};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
pub type Shared = Arc<Mutex<Model>>;
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("roundkeys.exe"))
        .parent()
        .unwrap()
        .join("config.json")
}
fn main() -> eframe::Result {
    let config_path = config_path();
    let (config, error) = match Config::load(&config_path) {
        Ok(c) => (c, None),
        Err(e) => (
            Config::default(),
            Some(format!("配置读取失败，已使用默认值（原文件保留）：{e}")),
        ),
    };
    let shared = Arc::new(Mutex::new(Model::new(config)));
    shared.lock().unwrap().demo = std::env::args().any(|a| a == "--demo");
    let input_check = std::env::args().any(|a| a == "--input-check");
    let headless = std::env::args().any(|a| a == "--headless");
    if input_check || headless {
        input::start(shared.clone());
    }
    let port = std::env::var("ROUNDKEYS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(17842);
    server::start(shared.clone(), port);
    if input_check {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let mut m = shared.lock().unwrap();
        let snapshot = m.snapshot();
        println!(
            "{}",
            serde_json::json!({"input":m.input_status,"controllers":m.controller_status,"devices":m.devices,"last_input":m.last_input,"counts":snapshot.counts})
        );
        return Ok(());
    }
    if headless {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1110.0, 790.0])
        .with_min_inner_size([940.0, 650.0]);
    if std::env::args().any(|a| a == "--capture-ui") {
        viewport = viewport
            .with_position([-4000.0, -4000.0])
            .with_active(false)
            .with_visible(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "RoundKeys · Keys + Scratch",
        options,
        Box::new(move |cc| {
            // Windows routes a keyboard usage to the last window registered in the process.
            // Start our background sink after winit has created its event loop.
            input::start(shared.clone());
            Ok(Box::new(ui::App::new(cc, shared, config_path, port, error)))
        }),
    )
}
