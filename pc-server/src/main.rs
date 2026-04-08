#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            
            if let Ok(font_data) = load_cjk_font() {
                fonts.font_data.insert(
                    "CjkFont".to_owned(),
                    egui::FontData::from_owned(font_data),
                );
                
                fonts.families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "CjkFont".to_owned());
                
                fonts.families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "CjkFont".to_owned());
                
                cc.egui_ctx.set_fonts(fonts);
            }
            
            Ok(Box::new(app::SmsReaderApp::new(cc)))
        }),
    )
}

fn load_cjk_font() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let exe_dir = std::env::current_exe()?;
    let exe_dir = exe_dir.parent().ok_or("no parent")?;
    
    let candidates = [
        exe_dir.join("fonts").join("NotoSansSC-Regular.ttf"),
        exe_dir.join("fonts").join("NotoSansSC-Regular.otf"),
        exe_dir.join("NotoSansSC-Regular.ttf"),
    ];
    
    for path in &candidates {
        if path.exists() {
            return Ok(std::fs::read(path)?);
        }
    }
    
    if let Ok(system_font) = std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        return Ok(system_font);
    }
    if let Ok(system_font) = std::fs::read("C:/Windows/Fonts/simsun.ttc") {
        return Ok(system_font);
    }
    if let Ok(system_font) = std::fs::read("C:/Windows/Fonts/simhei.ttf") {
        return Ok(system_font);
    }
    
    Err("no CJK font found".into())
}