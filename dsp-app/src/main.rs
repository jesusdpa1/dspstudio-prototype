mod app;

mod blueprint;
mod components;
mod core;
mod features;
mod icons;

use app::DspStudioApp;
use crate::core::bridge::BackendConfig;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut cli_path: Option<PathBuf> = None;
    let mut backend = BackendConfig::Local;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--remote" && i + 1 < args.len() {
            backend = BackendConfig::Remote(args[i + 1].clone());
            i += 2;
        } else if !args[i].starts_with("--") {
            cli_path = Some(PathBuf::from(&args[i]));
            i += 1;
        } else {
            i += 1;
        }
    }

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        vsync: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_visible(true),
        ..Default::default()
    };

    eframe::run_native(
        "DSP Studio",
        native_options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(DspStudioApp::new(cc, cli_path.clone(), backend, false)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Noto Sans Symbols 2 covers the geometric panel-toggle glyphs (◧ U+25E7,
    // ◨ U+25E8, ⬧ U+2B27) that are absent from egui's default font.
    fonts.font_data.insert(
        "noto_symbols2".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(
            include_bytes!("../assets/NotoSansSymbols2-Regular.ttf")
        )),
    );

    // Append as fallback so it only activates for glyphs not in the primary font.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("noto_symbols2".to_owned());
    }

    ctx.set_fonts(fonts);
}
