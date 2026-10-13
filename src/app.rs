//! 应用状态与主逻辑。

use std::sync::mpsc::{channel, Receiver};

use eframe::egui;

use crate::api::{self, ModelMode};
use crate::map::{MapSource, MapState};
use crate::model::{EventKind, ForecastBundle, Location, Screen};
use crate::theme::{self, Palette};

const DEFAULT_LAT: f64 = 30.2741;
const DEFAULT_LON: f64 = 120.1551;

/// 更新源（应返回 JSON：`{"version":"x.y.z"}`）；留空表示未配置。
pub const UPDATE_URL: &str = "";

pub struct DawnGlowApp {
    pub p: Palette,
    pub screen: Screen,
    /// 当前查看的事件（朝霞/晚霞）。
    pub kind: EventKind,
    /// 当前查看的天序号。
    pub day: usize,
    pub location: Location,
    pub bundle: ForecastBundle,
    pub loading: bool,
    pub status: String,
    pub fetch_rx: Option<Receiver<Result<ForecastBundle, String>>>,
    pub loc_rx: Option<Receiver<Result<(f64, f64, String), String>>>,
    pub search_query: String,
    pub search_results: Vec<Location>,
    pub search_rx: Option<Receiver<Result<Vec<Location>, String>>>,
    pub name_rx: Option<Receiver<(f64, f64, Option<String>)>>,
    pub map: MapState,
    pub show_factors: bool,
    /// 地点名只显示第一段（否则完整显示并换行）。
    pub compact_name: bool,
    /// 默认展示的事件（true=朝霞）。
    pub default_sunrise: bool,
    /// 是否显示数据来源署名。
    pub show_attribution: bool,
    /// 预报模式（融合 / 单模式）。
    pub model_mode: ModelMode,
    /// 自定义字体文件（系统字体名或绝对路径）；None=系统默认。
    pub font_file: Option<String>,
    /// 字体搜索框。
    pub font_query: String,
    /// 自定义下载链接输入框。
    pub font_url_input: String,
    /// 下载结果提示。
    pub font_dl_msg: String,
    /// 进行中的下载（可同时多个）：显示名 → 接收端。
    font_dl: Vec<(String, Receiver<(String, Result<String, String>)>)>,
    /// 各下载进度（已接收 / 总大小）。
    font_dl_prog: std::collections::HashMap<String, std::sync::Arc<DlProgress>>,
    /// 自定义字体文件路径输入。
    pub font_path_input: String,
    /// 系统字体文件名列表（缓存）。
    pub font_names: Vec<String>,
    /// 最近一次数据更新时间（地点本地 HH:MM）。
    pub last_updated: Option<String>,
    /// 首屏数据到达时刻（用于骨架→内容的错峰浮现）。
    pub reveal_at: Option<f64>,
    reveal_done: bool,
    /// 本次拉取的开始时刻（用于超时兑底）。
    fetch_started_at: Option<f64>,
    /// 更新检查状态与通道。
    pub update_status: String,
    update_rx: Option<Receiver<Result<String, String>>>,
    /// 安全区内边距（左、上、右、下），单位 egui point。
    pub pad: (f32, f32, f32, f32),
    /// 诊断用：原始安全区像素与显示密度。
    pub insets_px: (f32, f32, f32, f32),
    pub ppp: f32,
    insets_done: bool,
    insets_tries: u32,
    last_size: egui::Vec2,
    started: bool,
}

impl DawnGlowApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cfg = load_config();
        install_fonts(&cc.egui_ctx, &cfg.font_file);
        let location = Location {
            name: cfg.name.clone(),
            lat: cfg.lat,
            lon: cfg.lon,
        };
        let map = MapState::new(location.lat, location.lon, cfg.map_source);
        DawnGlowApp {
            p: theme::palette(),
            screen: Screen::Forecast,
            kind: if cfg.default_sunrise {
                EventKind::Sunrise
            } else {
                EventKind::Sunset
            },
            day: 0,
            location,
            bundle: ForecastBundle::default(),
            loading: false,
            status: String::new(),
            fetch_rx: None,
            loc_rx: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_rx: None,
            name_rx: None,
            map,
            show_factors: cfg.expand_factors,
            compact_name: cfg.compact_name,
            default_sunrise: cfg.default_sunrise,
            show_attribution: cfg.show_attribution,
            model_mode: cfg.model_mode,
            font_file: cfg.font_file.clone(),
            font_query: String::new(),
            font_url_input: String::new(),
            font_dl_msg: String::new(),
            font_dl: Vec::new(),
            font_dl_prog: std::collections::HashMap::new(),
            font_path_input: String::new(),
            font_names: system_font_files(),
            last_updated: None,
            reveal_at: None,
            reveal_done: true,
            fetch_started_at: None,
            update_status: String::new(),
            update_rx: None,
            pad: (20.0, 26.0, 20.0, 6.0),
            insets_done: false,
            insets_tries: 0,
            insets_px: (0.0, 0.0, 0.0, 0.0),
            ppp: 1.0,
            last_size: egui::Vec2::ZERO,
            started: false,
        }
    }

    /// 拉取当前地点的预报（后台线程）。
    pub fn start_fetch(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.status = "正在拉取 GFS / ECMWF / CAMS 数据…".into();
        self.fetch_started_at = Some(ctx.input(|i| i.time));
        let loc = self.location.clone();
        let mode = self.model_mode;
        let (tx, rx) = channel();
        self.fetch_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = api::fetch_bundle(&loc, mode);
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    }

    fn poll_fetch(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.fetch_rx {
            match rx.try_recv() {
                Ok(Ok(b)) => {
                    crate::widget::cache_bundle(&crate::platform::data_dir(), &b);
                    self.last_updated = Some(now_hhmm(b.utc_offset_seconds));
                    if self.bundle.days.is_empty() {
                        // 首次拿到数据：启动错峰浮现
                        self.reveal_at = Some(ctx.input(|i| i.time));
                        self.reveal_done = false;
                    }
                    self.bundle = b;
                    self.day = self.day.min(self.bundle.days.len().saturating_sub(1));
                    self.status.clear();
                    self.loading = false;
                    self.fetch_rx = None;
                    self.fetch_started_at = None;
                    self.save_config();
                }
                Ok(Err(e)) => {
                    self.status = e;
                    self.loading = false;
                    self.fetch_rx = None;
                    self.fetch_started_at = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.loading = false;
                    self.fetch_rx = None;
                    self.fetch_started_at = None;
                }
            }
        }
    }

    /// 使用系统最近定位。
    pub fn use_gps(&mut self, ctx: &egui::Context) {
        if self.loc_rx.is_some() {
            return;
        }
        self.status = "正在获取定位…".into();
        let (tx, rx) = channel();
        self.loc_rx = Some(rx);
        let ctx = ctx.clone();
        #[cfg(target_os = "android")]
        {
            let app = crate::ANDROID_APP.get().cloned();
            std::thread::spawn(move || {
                let res = match app {
                    Some(app) => match crate::android_glue::last_location(&app) {
                        Ok((lat, lon)) => Ok((lat, lon, String::new())),
                        Err(e) => Err(e),
                    },
                    None => Err("定位不可用".into()),
                };
                let _ = tx.send(res);
                ctx.request_repaint();
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = tx;
            self.status = "当前平台不使用设备定位，请用地图选点".into();
            self.loc_rx = None;
        }
    }

    fn poll_loc(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.loc_rx else { return };
        match rx.try_recv() {
            Ok(Ok((lat, lon, _))) => {
                self.set_location(Location {
                    name: "当前位置".into(),
                    lat,
                    lon,
                }, ctx);
                self.loc_rx = None;
            }
            Ok(Err(e)) => {
                self.status = e;
                self.loc_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.loc_rx = None;
            }
        }
    }

    pub fn set_location(&mut self, loc: Location, ctx: &egui::Context) {
        self.location = loc;
        self.map.center_lat = self.location.lat;
        self.map.center_lon = self.location.lon;
        self.map.pin_lat = self.location.lat;
        self.map.pin_lon = self.location.lon;
        self.day = 0;
        // 清空旧数据：新地点先显示骨架屏
        self.bundle = ForecastBundle::default();
        self.reveal_at = None;
        self.reveal_done = true;
        self.start_fetch(ctx);
    }

    /// 采用地图选点：先以「地图选点」占位并拉取预报，同时后台反查地名。
    pub fn use_map_point(&mut self, lat: f64, lon: f64, ctx: &egui::Context) {
        self.set_location(
            Location {
                name: "地图选点".into(),
                lat,
                lon,
            },
            ctx,
        );
        let (tx, rx) = channel();
        self.name_rx = Some(rx);
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let name = api::place_name(lat, lon);
            let _ = tx.send((lat, lon, name));
            ctx2.request_repaint();
        });
    }

    fn poll_name(&mut self) {
        let Some(rx) = &self.name_rx else { return };
        match rx.try_recv() {
            Ok((lat, lon, name)) => {
                let same = (self.location.lat - lat).abs() < 1e-6
                    && (self.location.lon - lon).abs() < 1e-6;
                if same {
                    if let Some(n) = name {
                        self.location.name = n;
                        self.bundle.location.name = self.location.name.clone();
                    }
                }
                self.name_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.name_rx = None;
            }
        }
    }

    /// 惰性读取窗口安全区（系统栏 + 挖孔），换算为内容内边距。
    /// 屏幕尺寸变化（旋转 / 密度变化）时重新读取。
    fn ensure_insets(&mut self, ctx: &egui::Context) {
        let size = ctx.screen_rect().size();
        if (size - self.last_size).length_sq() > 1.0 {
            self.last_size = size;
            self.insets_done = false;
            self.insets_tries = 0;
        }
        if self.insets_done {
            return;
        }
        self.insets_tries += 1;
        #[cfg(target_os = "android")]
        {
            let Some(app) = crate::ANDROID_APP.get() else {
                self.insets_done = true;
                return;
            };
            match crate::android_glue::window_insets(app) {
                Ok((l, t, r, b)) if l + t + r + b > 0.0 => {
                    let ppp = ctx.pixels_per_point().max(0.5);
                    self.insets_px = (l, t, r, b);
                    self.ppp = ppp;
                    let (lp, tp, rp, bp) = (l / ppp, t / ppp, r / ppp, b / ppp);
                    self.pad = (
                        lp.max(18.0) + 2.0,
                        tp.max(0.0) + 4.0,
                        rp.max(18.0) + 2.0,
                        bp.max(0.0) + 2.0,
                    );
                    log::info!(
                        "安全区(px): L{l:.0} T{t:.0} R{r:.0} B{b:.0} ppp={ppp:.2} pad={:?}",
                        self.pad
                    );
                    self.insets_done = true;
                }
                Ok(_) => {
                    // 全零：可能尚未布局，重试若干帧后再退回默认值。
                    if self.insets_tries > 180 {
                        self.insets_done = true;
                    }
                }
                Err(e) => {
                    if self.insets_tries > 180 {
                        log::warn!("读取安全区失败，使用默认留白: {e}");
                        self.insets_done = true;
                    }
                }
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            self.pad = (10.0, 10.0, 10.0, 10.0);
            self.insets_done = true;
        }
    }

    pub fn start_search(&mut self, ctx: &egui::Context) {
        let q = self.search_query.trim().to_string();
        if q.is_empty() || self.search_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.search_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = api::search_place(&q);
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    }

    fn poll_search(&mut self) {
        let Some(rx) = &self.search_rx else { return };
        match rx.try_recv() {
            Ok(Ok(v)) => {
                self.search_results = v;
                self.search_rx = None;
            }
            Ok(Err(e)) => {
                self.status = e;
                self.search_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.search_rx = None;
            }
        }
    }
}

impl eframe::App for DawnGlowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.started {
            self.started = true;
            self.start_fetch(ctx);
        }
        self.ensure_insets(ctx);

        // 系统返回键 / 返回手势
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            if !self.search_results.is_empty() {
                self.search_results.clear();
            } else if self.screen == Screen::Forecast {
                #[cfg(target_os = "android")]
                {
                    if let Some(app) = crate::ANDROID_APP.get() {
                        crate::android_glue::finish_activity(app);
                    }
                }
            } else if self.screen == Screen::Font {
                self.screen = Screen::Settings;
            } else {
                self.screen = Screen::Forecast;
            }
        }

        self.poll_fetch(ctx);
        self.poll_loc(ctx);
        self.poll_search();
        self.poll_name();
        self.poll_font_dl(ctx);
        self.poll_update();
        self.map.poll(ctx);

        let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05);
        if self.map.step(dt) {
            ctx.request_repaint();
        }

        // 拉取超时兑底：避免网络卡住时一直停在骨架屏
        if self.loading {
            let now = ctx.input(|i| i.time);
            if let Some(t0) = self.fetch_started_at {
                if now - t0 > 20.0 {
                    self.loading = false;
                    self.fetch_rx = None;
                    self.fetch_started_at = None;
                    self.status = "加载超时，请检查网络后重试".into();
                }
            }
        }

        // 错峰浮现收尾
        if !self.reveal_done {
            let now = ctx.input(|i| i.time);
            if let Some(t0) = self.reveal_at {
                if now - t0 > 1.2 {
                    self.reveal_done = true;
                } else {
                    ctx.request_repaint();
                }
            } else {
                self.reveal_done = true;
            }
        }

        if self.loading || self.loc_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        crate::ui::draw(self, ctx);
    }
}

/// 字体下载进度（已接收 / 总大小，字节）。
#[derive(Default)]
pub struct DlProgress {
    pub done: std::sync::atomic::AtomicU64,
    pub total: std::sync::atomic::AtomicU64,
}

/// 下载字体的本地缓存路径（`<data>/fonts/<名字>.ttf`）。
pub fn font_cache_path(name: &str) -> std::path::PathBuf {
    let mut safe: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if safe.is_empty() {
        safe = "font".into();
    }
    crate::platform::data_dir()
        .join("fonts")
        .join(format!("{safe}.ttf"))
}

/// 系统字体文件名列表（用于字体选择窗口）。
pub fn system_font_files() -> Vec<String> {
    let mut names = system_fonts_dir_files();
    if names.is_empty() {
        names = fonts_xml_files();
    }
    names
}

/// 默认字体链：模拟系统 sans-serif 家族（拉丁在前、CJK 在后）。
fn default_font_files() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new("/system/fonts");
    let names = system_font_files();
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    let pick = |kw: &str, out: &mut Vec<std::path::PathBuf>| {
        let mut hits: Vec<&String> = names.iter().filter(|n| n.contains(kw)).collect();
        hits.sort_by_key(|n| if n.contains("Regular") { 0 } else { 1 });
        if let Some(n) = hits.first() {
            push_font(out, dir.join(n.as_str()));
        }
    };
    for kw in ["Roboto-Regular.ttf", "NotoSans-Regular.ttf"] {
        if names.iter().any(|n| n == kw) {
            push_font(&mut out, dir.join(kw));
            break;
        }
    }
    if out.len() < 2 {
        for kw in [
            "NotoSansCJK",
            "NotoSansSC",
            "NotoSansCJKsc",
            "SourceHanSansSC",
            "HarmonyOS_Sans_SC",
            "OPPOSans",
            "DroidSansFallback",
        ] {
            pick(kw, &mut out);
            if out.len() >= 2 {
                break;
            }
        }
    }
    out
}

/// 安装字体：优先用户选择的字体文件（系统字体名或绝对路径），否则用系统默认链；
/// `.ttc` 自动选简体面。保留 egui 自带字体作 emoji 等兜底。
fn install_fonts(ctx: &egui::Context, chosen: &Option<String>) {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    match chosen {
        Some(c) => {
            let p = std::path::PathBuf::from(c);
            let p = if p.is_absolute() {
                p
            } else {
                std::path::Path::new("/system/fonts").join(c)
            };
            if p.exists() {
                files.push(p);
            }
        }
        None => files = default_font_files(),
    }
    if let Ok(p) = std::env::var("DAWNGLOW_FONT") {
        let pb = std::path::PathBuf::from(&p);
        if pb.exists() {
            files.insert(0, pb);
        }
    }

    let mut extra: Vec<(String, u32, Vec<u8>)> = Vec::new();
    for (i, path) in files.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let index = ttc_face_index(&bytes);
            extra.push((format!("sys{i}"), index, bytes));
        }
    }
    if extra.is_empty() {
        return;
    }

    let mut fonts = egui::FontDefinitions::default();
    let names: Vec<String> = extra.iter().map(|(n, _, _)| n.clone()).collect();
    for (name, index, bytes) in extra {
        fonts.font_data.insert(
            name,
            std::sync::Arc::new(egui::FontData {
                font: std::borrow::Cow::Owned(bytes),
                index,
                tweak: egui::FontTweak::default(),
            }),
        );
    }
    let prop = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mono = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let mut prop_list = names.clone();
    prop_list.extend(prop);
    let mut mono_list = names;
    mono_list.extend(mono);
    fonts.families.insert(egui::FontFamily::Proportional, prop_list);
    fonts.families.insert(egui::FontFamily::Monospace, mono_list);
    ctx.set_fonts(fonts);
}

/// `.ttc` 里挑简体中文面（SC/Hans/简体），找不到用 0。
fn ttc_face_index(bytes: &[u8]) -> u32 {
    let n = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
    for i in 0..n {
        if let Ok(face) = ttf_parser::Face::parse(bytes, i) {
            for name in face.names() {
                if let Some(s) = name.to_string() {
                    if is_sc_name(&s) {
                        return i;
                    }
                }
            }
        }
    }
    0
}

fn is_sc_name(s: &str) -> bool {
    let l = s.to_lowercase();
    if l.contains("simplified") || l.contains("简体") {
        return true;
    }
    l.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == "sc" || w == "hans")
}

/// `/system/fonts` 下的字体文件名（排序）。
fn system_fonts_dir_files() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/system/fonts") {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str() {
                let l = n.to_ascii_lowercase();
                if l.ends_with(".ttf") || l.ends_with(".ttc") || l.ends_with(".otf") {
                    names.push(n.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

/// 目录扫不到时，退回解析 fonts.xml 的 sans-serif 家族。
fn fonts_xml_files() -> Vec<String> {
    for xml_path in ["/system/etc/fonts.xml", "/system/etc/system_fonts.xml"] {
        if let Ok(xml) = std::fs::read_to_string(xml_path) {
            let block = xml_family_block(&xml, "sans-serif")
                .or_else(|| xml_family_block_legacy(&xml, "sans-serif"));
            if let Some(block) = block {
                let files = xml_font_entries(&block);
                if !files.is_empty() {
                    return files;
                }
            }
        }
    }
    Vec::new()
}

/// 加入字体文件（去重 / 存在 / 文件头像字体），最多 3 个。
fn push_font(out: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    if out.len() >= 3 || out.contains(&path) || !path.exists() {
        return;
    }
    if looks_like_font(&path) {
        out.push(path);
    }
}

/// 用文件头判断是否像字体（TTF/OTF/TTC），避免加载不可解析的文件。
fn looks_like_font(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut b = [0u8; 4];
    if f.read_exact(&mut b).is_err() {
        return false;
    }
    matches!(&b, b"\x00\x01\x00\x00" | b"OTTO" | b"ttcf" | b"true" | b"typ1")
}

/// 取 `name="family"` 所在 `<family>...</family>` 的内容（新格式 fonts.xml）。
fn xml_family_block(xml: &str, family: &str) -> Option<String> {
    let needle = format!("name=\"{family}\"");
    let pos = xml.find(&needle)?;
    let end = xml[pos..].find("</family>")? + pos;
    Some(xml[pos..end].to_string())
}

/// 旧格式 system_fonts.xml：`<family><name>sans-serif</name>...</family>`。
fn xml_family_block_legacy(xml: &str, family: &str) -> Option<String> {
    let needle = format!("<name>{family}</name>");
    let pos = xml.find(&needle)?;
    let end = xml[pos..].find("</family>")? + pos;
    Some(xml[pos..end].to_string())
}

/// 提取块内 `<font ...>文件名</font>` 的文件名。
fn xml_font_entries(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = block;
    while let Some(p) = rest.find("<font") {
        let after = &rest[p + 5..];
        let Some(gt) = after.find('>') else { break };
        let body = &after[gt + 1..];
        let Some(lt) = body.find('<') else { break };
        let name = body[..lt].trim();
        if name.ends_with(".ttf") || name.ends_with(".ttc") || name.ends_with(".otf") {
            out.push(name.to_string());
        }
        rest = &body[lt..];
    }
    out
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredConfig {
    name: String,
    lat: f64,
    lon: f64,
    #[serde(default = "default_true")]
    compact_name: bool,
    #[serde(default)]
    default_sunrise: bool,
    #[serde(default = "default_true")]
    show_attribution: bool,
    #[serde(default = "default_true")]
    expand_factors: bool,
    #[serde(default)]
    map_source: MapSource,
    #[serde(default)]
    model_mode: ModelMode,
    #[serde(default)]
    font_file: Option<String>,
}

fn default_true() -> bool {
    true
}

fn now_hhmm(offset_secs: i64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let local = secs + offset_secs;
    let h = (local % 86400) / 3600;
    let m = (local % 3600) / 60;
    format!("{h:02}:{m:02}")
}

fn settings_path() -> std::path::PathBuf {
    crate::platform::data_dir().join("settings.json")
}

fn load_config() -> StoredConfig {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str::<StoredConfig>(&s).ok())
        .unwrap_or_else(|| StoredConfig {
            name: "杭州".into(),
            lat: DEFAULT_LAT,
            lon: DEFAULT_LON,
            compact_name: true,
            default_sunrise: false,
            show_attribution: true,
            expand_factors: true,
            map_source: MapSource::default(),
            model_mode: ModelMode::default(),
            font_file: None,
        })
}

impl DawnGlowApp {
    /// 持久化配置（地点 + 设置）。
    /// 选择自定义字体（`None`=系统默认），立即生效。
    pub fn set_font_file(&mut self, f: Option<String>, ctx: &egui::Context) {
        self.font_file = f;
        self.save_config();
        let cur = self.font_file.clone();
        install_fonts(ctx, &cur);
    }

    /// 某字体是否正在下载。
    pub fn font_downloading(&self, name: &str) -> bool {
        self.font_dl.iter().any(|(n, _)| n == name)
    }

    /// 某字体已接收的 MB 数。
    pub fn font_dl_mb_of(&self, name: &str) -> f32 {
        self.font_dl_prog.get(name).map_or(0.0, |p| {
            p.done.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1_048_576.0
        })
    }

    /// 下载进度 0..1；服务端未给总大小时为 `None`（不确定进度）。
    pub fn font_dl_frac_of(&self, name: &str) -> Option<f32> {
        let p = self.font_dl_prog.get(name)?;
        let total = p.total.load(std::sync::atomic::Ordering::Relaxed);
        if total == 0 {
            return None;
        }
        Some((p.done.load(std::sync::atomic::Ordering::Relaxed) as f32 / total as f32).clamp(0.0, 1.0))
    }

    /// 下载字体（后台，可同时多个），完成后自动选用。
    ///
    /// 厂商字体包多为整包 zip（部分 CDN 带防盗链，需转递 `referer`），
    /// 先落临时文件再解出单个字体，避免整包读进内存。
    pub fn download_font(
        &mut self,
        name: &str,
        url: &str,
        referer: Option<&str>,
        ctx: &egui::Context,
    ) {
        let url = url.trim().to_string();
        if url.is_empty() || self.font_downloading(name) {
            return;
        }
        let referer = referer.map(|r| r.to_string());
        let out = font_cache_path(name);
        let dir = out.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
        crate::platform::ensure_dir(&dir);
        let name = name.to_string();
        let (tx, rx) = channel();
        let progress = std::sync::Arc::new(DlProgress::default());
        self.font_dl_prog.insert(name.clone(), progress.clone());
        self.font_dl.push((name.clone(), rx));
        self.font_dl_msg.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = (|| -> Result<String, String> {
                use std::io::{Read, Write};
                let mut req = ureq::get(&url)
                    .set("User-Agent", "DawnGlow/0.1")
                    .timeout(std::time::Duration::from_secs(900));
                if let Some(r) = referer.as_deref() {
                    req = req.set("Referer", r);
                }
                let resp = req.call().map_err(|e| e.to_string())?;
                if let Some(len) = resp
                    .header("Content-Length")
                    .and_then(|v| v.trim().parse::<u64>().ok())
                {
                    progress.total.store(len, std::sync::atomic::Ordering::Relaxed);
                }
                let tmp = dir.join(format!(".dl-{}.part", font_cache_path(&name).file_stem().map_or_else(String::new, |s| s.to_string_lossy().to_string())));
                let mut src = resp.into_reader();
                let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
                let mut buf = [0u8; 64 * 1024];
                let mut total = 0u64;
                loop {
                    let k = src.read(&mut buf).map_err(|e| e.to_string())?;
                    if k == 0 {
                        break;
                    }
                    f.write_all(&buf[..k]).map_err(|e| e.to_string())?;
                    total += k as u64;
                    progress.done.store(total, std::sync::atomic::Ordering::Relaxed);
                }
                drop(f);
                if total < 4096 {
                    let _ = std::fs::remove_file(&tmp);
                    return Err("文件过小，可能下载失败".into());
                }
                if crate::zipfont::is_zip_file(&tmp) {
                    let entry = crate::zipfont::extract_font(&tmp, &out)?;
                    let _ = std::fs::remove_file(&tmp);
                    log::info!("字体包解出 {entry} -> {}", out.display());
                } else {
                    std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
                }
                Ok(out.to_string_lossy().to_string())
            })();
            let _ = tx.send((name, res));
            ctx.request_repaint();
        });
    }

    fn poll_font_dl(&mut self, ctx: &egui::Context) {
        let mut done: Vec<usize> = Vec::new();
        let mut msgs: Vec<String> = Vec::new();
        let mut ready: Option<String> = None;
        for (i, (name, rx)) in self.font_dl.iter().enumerate() {
            match rx.try_recv() {
                Ok((_, Ok(path))) => {
                    done.push(i);
                    msgs.push(format!("{name} 已下载并启用"));
                    ready = Some(path);
                }
                Ok((_, Err(e))) => {
                    done.push(i);
                    msgs.push(format!("{name} 下载失败：{e}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => done.push(i),
            }
        }
        for i in done.into_iter().rev() {
            let (n, _) = self.font_dl.remove(i);
            self.font_dl_prog.remove(&n);
        }
        if let Some(p) = ready {
            self.set_font_file(Some(p), ctx);
        }
        for m in msgs {
            self.font_dl_msg = m;
        }
    }

    /// 切换预报模式（并重新拉取）。
    pub fn set_model_mode(&mut self, m: ModelMode, ctx: &egui::Context) {
        self.model_mode = m;
        self.save_config();
        self.start_fetch(ctx);
    }

    /// 切换地图源。
    pub fn set_map_source(&mut self, s: MapSource) {
        self.map.source = s;
        self.save_config();
    }

    /// 检查更新（未配置更新源时直接提示）。
    pub fn check_update(&mut self, ctx: &egui::Context) {
        if UPDATE_URL.is_empty() {
            self.update_status =
                format!("当前版本 {}（尚未配置更新源）", env!("CARGO_PKG_VERSION"));
            return;
        }
        if self.update_rx.is_some() {
            return;
        }
        self.update_status = "正在检查更新…".into();
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = (|| -> Result<String, String> {
                let resp = ureq::get(UPDATE_URL)
                    .set("User-Agent", "DawnGlow/0.1")
                    .timeout(std::time::Duration::from_secs(12))
                    .call()
                    .map_err(|e| e.to_string())?;
                let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
                v.get("version")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| "更新源返回格式不正确".to_string())
            })();
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    }

    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        match rx.try_recv() {
            Ok(Ok(remote)) => {
                let cur = env!("CARGO_PKG_VERSION");
                self.update_status = if remote == cur {
                    format!("已是最新版本 {cur}")
                } else {
                    format!("发现新版本 {remote}（当前 {cur}）")
                };
                self.update_rx = None;
            }
            Ok(Err(e)) => {
                self.update_status = format!("检查更新失败：{e}");
                self.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.update_rx = None;
            }
        }
    }

    pub fn save_config(&self) {
        let dir = crate::platform::data_dir();
        crate::platform::ensure_dir(&dir);
        let cfg = StoredConfig {
            name: self.location.name.clone(),
            lat: self.location.lat,
            lon: self.location.lon,
            compact_name: self.compact_name,
            default_sunrise: self.default_sunrise,
            show_attribution: self.show_attribution,
            expand_factors: self.show_factors,
            map_source: self.map.source,
            model_mode: self.model_mode,
            font_file: self.font_file.clone(),
        };
        if let Ok(s) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(settings_path(), s);
        }
    }
}
