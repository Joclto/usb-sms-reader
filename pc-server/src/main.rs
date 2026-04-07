mod config;
mod error;
mod core;
mod forwarder;
mod classifier;
mod storage;
mod server;
mod app;

use eframe::egui;

fn main() -> eframe::Result<()> {
    dotenv::dotenv().ok();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("USB SMS Reader"),
        ..Default::default()
    };

    eframe::run_native(
        "USB SMS Reader",
        native_options,
        Box::new(|cc| Ok(Box::new(app::SmsReaderApp::new(cc)))),
    )
}