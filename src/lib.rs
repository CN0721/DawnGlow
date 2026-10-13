//! 栖霞 (QiXia) —— 基于 GFS / ECMWF / CAMS 的日出日落火烧云预报。
//!
//! 本 crate 同时产出动态库（Android NativeActivity）与桌面可执行文件（开发预览）。

pub mod api;
pub mod app;
pub mod geo;
pub mod map;
pub mod model;
pub mod platform;
pub mod score;
pub mod theme;
pub mod ui;
pub mod widget;
pub mod zipfont;

#[cfg(target_os = "android")]
pub mod android_glue;

/// Android 私有数据目录（由 `android_main` 启动时写入）。
#[cfg(target_os = "android")]
pub static ANDROID_DATA: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// 启动用的 `AndroidApp`（供后台线程读取定位）。
#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<android_activity::AndroidApp> =
    std::sync::OnceLock::new();

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    use std::sync::OnceLock;
    static LOG_INIT: OnceLock<()> = OnceLock::new();
    LOG_INIT.get_or_init(|| {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(log::LevelFilter::Info),
        );
    });

    if let Some(p) = app.internal_data_path() {
        let _ = ANDROID_DATA.set(p);
    }
    let _ = ANDROID_APP.set(app.clone());

    android_glue::request_location_permission(&app);
    android_glue::enable_edge_to_edge(&app);

    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "栖霞",
        options,
        Box::new(|cc| Ok(Box::new(crate::app::DawnGlowApp::new(cc)))),
    ) {
        log::error!("eframe 运行失败: {e}");
    }
}
