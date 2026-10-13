//! 墨卡托地图坐标换算（OSM 瓦片编号）。缩放级别用 `f64` 以支持分数缩放。

/// 经纬度 → 分数瓦片坐标 (x, y)（`z` 可为小数）。
pub fn lonlat_to_tile(lon: f64, lat: f64, z: f64) -> (f64, f64) {
    let n = 2f64.powf(z);
    let lat = lat.clamp(-85.051_128_78, 85.051_128_78);
    let x = (lon + 180.0) / 360.0 * n;
    let lat_rad = lat.to_radians();
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

/// 分数瓦片坐标 → 经纬度。
pub fn tile_to_lonlat(x: f64, y: f64, z: f64) -> (f64, f64) {
    let n = 2f64.powf(z);
    let lon = x / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan();
    (lon, lat_rad.to_degrees())
}

// ---------- 高德（GCJ-02 火星坐标）纠偏 ----------

const GCJ_A: f64 = 6378245.0;
const GCJ_EE: f64 = 0.006_693_421_622_965_943;

fn out_of_china(lon: f64, lat: f64) -> bool {
    !(73.66..135.05).contains(&lon) || !(3.86..53.55).contains(&lat)
}

fn transform_lat(x: f64, y: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let mut ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.sqrt();
    ret += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * pi).sin() + 40.0 * (y / 3.0 * pi).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * pi).sin() + 320.0 * (y * pi / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn transform_lon(x: f64, y: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let mut ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.sqrt();
    ret += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * pi).sin() + 40.0 * (x / 3.0 * pi).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * pi).sin() + 300.0 * (x / 30.0 * pi).sin()) * 2.0 / 3.0;
    ret
}

/// WGS-84 → GCJ-02（中国境内偏移，境外原样返回）。
pub fn wgs84_to_gcj02(lon: f64, lat: f64) -> (f64, f64) {
    if out_of_china(lon, lat) {
        return (lon, lat);
    }
    let dlat = transform_lat(lon - 105.0, lat - 35.0);
    let dlon = transform_lon(lon - 105.0, lat - 35.0);
    let radlat = lat / 180.0 * std::f64::consts::PI;
    let magic = 1.0 - GCJ_EE * radlat.sin() * radlat.sin();
    let sqrtmagic = magic.sqrt();
    let dlat = (dlat * 180.0) / ((GCJ_A * (1.0 - GCJ_EE)) / (magic * sqrtmagic) * std::f64::consts::PI);
    let dlon = (dlon * 180.0) / (GCJ_A / sqrtmagic * radlat.cos() * std::f64::consts::PI);
    (lon + dlon, lat + dlat)
}
