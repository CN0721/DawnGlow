//! 滑动地图：栅格瓦片的多提供商支持、磁盘缓存与分数缩放。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use egui::ColorImage;

use crate::geo;

/// 瓦片源。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum MapSource {
    #[default]
    CartoDark,
    CartoLight,
    Osm,
    Amap,
}

impl MapSource {
    pub const ALL: [MapSource; 4] = [
        MapSource::CartoDark,
        MapSource::CartoLight,
        MapSource::Amap,
        MapSource::Osm,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MapSource::CartoDark => "Carto 深色",
            MapSource::CartoLight => "Carto 浅色",
            MapSource::Osm => "OpenStreetMap",
            MapSource::Amap => "高德地图",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            MapSource::CartoDark => "深色底图，与深色界面统一（推荐）",
            MapSource::CartoLight => "浅色简洁底图",
            MapSource::Osm => "标准 OpenStreetMap 底图",
            MapSource::Amap => "国内路网与中文标注（非官方瓦片，自动坐标纠偏）",
        }
    }

    fn idx(self) -> u32 {
        match self {
            MapSource::CartoDark => 0,
            MapSource::CartoLight => 1,
            MapSource::Osm => 2,
            MapSource::Amap => 3,
        }
    }

    fn from_index(i: u32) -> MapSource {
        match i {
            1 => MapSource::CartoLight,
            2 => MapSource::Osm,
            3 => MapSource::Amap,
            _ => MapSource::CartoDark,
        }
    }

    pub fn index(self) -> u32 {
        self.idx()
    }

    fn url(self, z: u32, x: u32, y: u32) -> String {
        match self {
            MapSource::CartoDark | MapSource::CartoLight => {
                let style = if self == MapSource::CartoDark {
                    "dark_all"
                } else {
                    "light_all"
                };
                let shard = ["a", "b", "c", "d"][((x + y) % 4) as usize];
                format!("https://{shard}.basemaps.cartocdn.com/{style}/{z}/{x}/{y}.png")
            }
            MapSource::Osm => format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
            MapSource::Amap => {
                // 高德瓦片为 GCJ-02，需将 WGS-84 瓦片索引纠偏后请求。
                let (gx, gy) = gcj_tile(z, x, y);
                let shard = (gx + gy) % 4 + 1;
                format!(
                    "https://wprd0{shard}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&style=7&x={gx}&y={gy}&z={z}"
                )
            }
        }
    }

    fn url_fallback(self, z: u32, x: u32, y: u32) -> Option<String> {
        match self {
            MapSource::Osm => Some(format!("https://tile.openstreetmap.de/{z}/{x}/{y}.png")),
            _ => None,
        }
    }

    pub fn attribution(self) -> &'static str {
        match self {
            MapSource::Osm => "© OpenStreetMap contributors",
            MapSource::Amap => "© 高德地图 AutoNavi",
            _ => "© OpenStreetMap contributors · © CARTO",
        }
    }
}

/// 把 WGS-84 瓦片索引换算为高德 GCJ-02 瓦片索引（同层级）。
fn gcj_tile(z: u32, x: u32, y: u32) -> (u32, u32) {
    let (lon, lat) = geo::tile_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, z as f64);
    let (glon, glat) = geo::wgs84_to_gcj02(lon, lat);
    let (fx, fy) = geo::lonlat_to_tile(glon, glat, z as f64);
    (fx.floor().max(0.0) as u32, fy.floor().max(0.0) as u32)
}

/// (source, z, x, y)
pub type Key = (u32, u32, u32, u32);

enum Tile {
    Ready(egui::TextureHandle),
    Failed,
}

pub struct MapState {
    pub center_lat: f64,
    pub center_lon: f64,
    /// 当前缩放（可见值，带动画）
    pub zoom: f32,
    /// 目标缩放
    pub zoom_target: f32,
    pub source: MapSource,
    pub pin_lat: f64,
    pub pin_lon: f64,
    /// 惯性速度（屏幕像素 / 秒）
    pub vel: egui::Vec2,
    /// 是否正在拖动（拖动时不施加惯性）
    pub dragging: bool,
    tiles: HashMap<Key, Tile>,
    requested: HashSet<Key>,
    tx: Sender<(Key, Result<ColorImage, String>)>,
    rx: Receiver<(Key, Result<ColorImage, String>)>,
    disk: std::path::PathBuf,
}

impl MapState {
    pub fn new(lat: f64, lon: f64, source: MapSource) -> Self {
        let (tx, rx) = channel();
        MapState {
            center_lat: lat,
            center_lon: lon,
            zoom: 9.0,
            zoom_target: 9.0,
            source,
            pin_lat: lat,
            pin_lon: lon,
            vel: egui::Vec2::ZERO,
            dragging: false,
            tiles: HashMap::new(),
            requested: HashSet::new(),
            tx,
            rx,
            disk: crate::platform::data_dir().join("tiles"),
        }
    }

    /// 接收后台线程解码好的瓦片并上传为纹理（必须在主线程调用）。
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((k, res)) = self.rx.try_recv() {
            match res {
                Ok(ci) => {
                    let tex = ctx.load_texture(
                        format!("tile:{}-{}-{}-{}", k.0, k.1, k.2, k.3),
                        ci,
                        egui::TextureOptions::LINEAR,
                    );
                    self.tiles.insert(k, Tile::Ready(tex));
                }
                Err(e) => {
                    log::warn!("瓦片 {:?} 加载失败: {e}", k);
                    self.tiles.insert(k, Tile::Failed);
                }
            }
        }
    }

    pub fn texture(&self, k: &Key) -> Option<&egui::TextureHandle> {
        match self.tiles.get(k) {
            Some(Tile::Ready(t)) => Some(t),
            _ => None,
        }
    }

    pub fn request(&mut self, k: Key, ctx: &egui::Context) {
        if self.requested.contains(&k) {
            return;
        }
        self.requested.insert(k);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let disk = self.disk.clone();
        std::thread::spawn(move || {
            let res = fetch_tile(k, &disk);
            let _ = tx.send((k, res));
            ctx.request_repaint();
        });
    }

    pub fn zoom_by(&mut self, delta: f32) {
        self.zoom_target = (self.zoom_target + delta).clamp(3.0, 19.0);
    }

    /// 以屏幕像素为单位平移。
    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        let z = self.zoom as f64;
        let (fx, fy) = geo::lonlat_to_tile(self.center_lon, self.center_lat, z);
        let (lon, lat) = geo::tile_to_lonlat(fx - dx / 256.0, fy - dy / 256.0, z);
        self.center_lon = lon.clamp(-180.0, 180.0);
        self.center_lat = lat.clamp(-85.0, 85.0);
    }

    /// 以视口内某点（相对中心的屏幕偏移 dx,dy）为锚点缩放 dz 个级别。
    pub fn zoom_at(&mut self, dz: f32, dx: f64, dy: f64) {
        let z1 = self.zoom as f64;
        let (fx, fy) = geo::lonlat_to_tile(self.center_lon, self.center_lat, z1);
        // 锚点地理坐标（当前缩放）
        let (alon, alat) = geo::tile_to_lonlat(
            (fx * 256.0 + dx) / 256.0,
            (fy * 256.0 + dy) / 256.0,
            z1,
        );
        let z2 = (self.zoom + dz).clamp(3.0, 19.0);
        self.zoom = z2;
        self.zoom_target = z2;
        // 重算中心，使锚点回到同一屏幕位置
        let (gx, gy) = geo::lonlat_to_tile(alon, alat, z2 as f64);
        let (lon, lat) = geo::tile_to_lonlat(
            (gx * 256.0 - dx) / 256.0,
            (gy * 256.0 - dy) / 256.0,
            z2 as f64,
        );
        self.center_lon = lon.clamp(-180.0, 180.0);
        self.center_lat = lat.clamp(-85.0, 85.0);
    }

    /// 推进缩放动画与惯性平移；返回是否仍在动（用于请求重绘）。
    pub fn step(&mut self, dt: f32) -> bool {
        let mut busy = false;

        let dz = self.zoom_target - self.zoom;
        if dz.abs() > 0.002 {
            self.zoom += dz * (dt * 14.0).min(1.0);
            busy = true;
        } else {
            self.zoom = self.zoom_target;
        }

        if !self.dragging && self.vel.length_sq() > 1.0 {
            let d = self.vel * dt;
            self.pan_by(d.x as f64, d.y as f64);
            self.vel *= (1.0 - 5.0 * dt).max(0.0);
            busy = true;
        } else if !self.dragging {
            self.vel = egui::Vec2::ZERO;
        }

        busy
    }
}

fn fetch_tile(k: Key, disk: &std::path::Path) -> Result<ColorImage, String> {
    let (src, z, x, y) = k;
    let file = disk.join(format!("{src}_{z}_{x}_{y}.png"));
    if let Ok(bytes) = std::fs::read(&file) {
        if let Ok(img) = image::load_from_memory(&bytes) {
            return Ok(to_color_image(img));
        }
    }

    let source = MapSource::from_index(src);
    let mut urls = vec![source.url(z, x, y)];
    if let Some(fb) = source.url_fallback(z, x, y) {
        urls.push(fb);
    }

    let mut last_err = String::from("无可用瓦片源");
    for url in urls {
        match ureq::get(&url)
            .set("User-Agent", "DawnGlow/0.1 (Android; map tiles)")
            .timeout(Duration::from_secs(15))
            .call()
        {
            Ok(resp) => {
                let mut bytes = Vec::new();
                if resp.into_reader().read_to_end(&mut bytes).is_err() {
                    last_err = "读取瓦片失败".into();
                    continue;
                }
                match image::load_from_memory(&bytes) {
                    Ok(img) => {
                        let _ = std::fs::create_dir_all(disk);
                        let _ = std::fs::write(&file, &bytes);
                        return Ok(to_color_image(img));
                    }
                    Err(e) => last_err = format!("瓦片解码失败: {e}"),
                }
            }
            Err(e) => last_err = format!("{e}"),
        }
    }
    Err(last_err)
}

fn to_color_image(img: image::DynamicImage) -> ColorImage {
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw())
}
