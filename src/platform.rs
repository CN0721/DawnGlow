use std::path::PathBuf;

/// 本应用私有数据目录（设置、瓦片缓存）。
///
/// - Android：`AndroidApp::internal_data_path()`，即 `/data/data/<pkg>/files`
/// - 桌面：`$DAWNGLOW_HOME` 或 `~/.dawnglow`
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        if let Some(p) = crate::ANDROID_DATA.get() {
            return p.clone();
        }
    }
    if let Ok(home) = std::env::var("DAWNGLOW_HOME") {
        return PathBuf::from(home);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".dawnglow");
    }
    PathBuf::from(".")
}

pub fn ensure_dir(p: &std::path::Path) {
    let _ = std::fs::create_dir_all(p);
}
