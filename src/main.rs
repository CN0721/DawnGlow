//! 桌面开发入口（预览用）。

fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "栖霞 (QiXia)",
        options,
        Box::new(|cc| Ok(Box::new(dawnglow::app::DawnGlowApp::new(cc)))),
    )
}
