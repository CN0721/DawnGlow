//! 数据层：Open-Meteo。
//!
//! - 预报：`models=gfs_seamless,ecmwf_ifs025`（NCEP GFS + ECMWF IFS）
//! - 空气质量：`domains=cams_global`（Copernicus CAMS 气溶胶光学厚度）
//! - 地理编码：Open-Meteo Geocoding
//!
//! 同一变量取两模式的算术平均作为融合输入；某模式缺测则用另一模式。

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use crate::model::{DayForecast, EventScore, ForecastBundle, Location};
use crate::score::{self, Inputs};

const UA: &str = "DawnGlow/0.1 (Android; sunrise/sunset glow forecast)";
const FORECAST_DAYS: u32 = 4;

/// 预报模式（决定使用 GFS / ECMWF / 两者融合）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum ModelMode {
    /// 两模式算术平均（默认）
    #[default]
    Blend,
    /// 仅 NCEP GFS
    Gfs,
    /// 仅 ECMWF IFS
    Ecmwf,
}

/// 融合模式使用的多模式集合（实测在中国区域均有云量数据）。
const MODELS_BLEND: [&str; 8] = [
    "gfs_seamless",
    "ecmwf_ifs025",
    "icon_seamless",
    "gem_seamless",
    "jma_seamless",
    "cma_grapes_global",
    "ukmo_seamless",
    "metno_seamless",
];

impl ModelMode {
    pub const ALL: [ModelMode; 3] = [ModelMode::Blend, ModelMode::Gfs, ModelMode::Ecmwf];

    pub fn name(self) -> &'static str {
        match self {
            ModelMode::Blend => "多模式集合",
            ModelMode::Gfs => "仅 NCEP GFS",
            ModelMode::Ecmwf => "仅 ECMWF IFS",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            ModelMode::Blend => "8 个模式取中位数，并给出集合概率（推荐）",
            ModelMode::Gfs => "只看 GFS，刷新时更一致",
            ModelMode::Ecmwf => "只看 ECMWF（无 visibility，不扣霾分）",
        }
    }

    pub fn models(self) -> &'static [&'static str] {
        match self {
            ModelMode::Blend => &MODELS_BLEND,
            ModelMode::Gfs => &["gfs_seamless"],
            ModelMode::Ecmwf => &["ecmwf_ifs025"],
        }
    }
}

/// 带有限重试的 GET（不采用长超时，避免“一直加载”）。
fn get_json(url: &str, attempts: u32, timeout_secs: u64) -> Result<Value, String> {
    let mut last = String::from("网络请求失败");
    for attempt in 0..attempts.max(1) {
        match ureq::get(url)
            .set("User-Agent", UA)
            .set("Accept", "application/json")
            .timeout(Duration::from_secs(timeout_secs))
            .call()
        {
            Ok(resp) => match resp.into_json::<Value>() {
                Ok(v) => return Ok(v),
                Err(e) => last = format!("响应解析失败: {e}"),
            },
            Err(e) => last = format!("网络请求失败: {e}"),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    Err(last)
}

/// 拉取指定地点的完整预报并完成评分。
///
/// 预报与空气质量**并行**请求；空气质量失败不致命（AOD 缺测则评分自动降级）。
pub fn fetch_bundle(loc: &Location, mode: ModelMode) -> Result<ForecastBundle, String> {
    let models = mode.models();
    let aq_url = aq_url(loc.lat, loc.lon);
    let aq_handle = std::thread::spawn(move || get_json(&aq_url, 2, 10).ok());
    // 集合预报只在多模式集合下才需要；体积较大（~110 KB），失败不致命
    let ens_handle = (models.len() > 1).then(|| {
        let u = ens_url(loc.lat, loc.lon);
        std::thread::spawn(move || get_json(&u, 1, 25).ok())
    });
    let fc = get_json(&forecast_url(loc.lat, loc.lon, models), 2, 8)?;
    let aq = aq_handle.join().ok().flatten();
    let ens = ens_handle.and_then(|h| h.join().ok()).flatten();
    build_bundle(loc, &fc, aq.as_ref(), ens.as_ref(), models)
}

fn forecast_url(lat: f64, lon: f64, models: &[&str]) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
&hourly=cloud_cover_low,cloud_cover_mid,cloud_cover_high,cloud_cover,visibility,relative_humidity_2m,precipitation_probability,direct_radiation,sunshine_duration\
&daily=sunrise,sunset,precipitation_probability_mean\
&models={}\
&timezone=auto&forecast_days={FORECAST_DAYS}",
        models.join(",")
    )
}

/// 集合预报（ECMWF ENS 51 + NCEP GEFS 31 = 82 个成员）的云量，用于算概率。
fn ens_url(lat: f64, lon: f64) -> String {
    format!(
        "https://ensemble-api.open-meteo.com/v1/ensemble?latitude={lat:.4}&longitude={lon:.4}\
&hourly=cloud_cover_low,cloud_cover_mid,cloud_cover_high,cloud_cover\
&models=ecmwf_ifs025_ensemble,ncep_gefs025&timezone=auto&forecast_days={FORECAST_DAYS}"
    )
}

fn aq_url(lat: f64, lon: f64) -> String {
    format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat:.4}&longitude={lon:.4}\
&hourly=aerosol_optical_depth&domains=cams_global&timezone=auto&forecast_days={FORECAST_DAYS}"
    )
}

// ---------- JSON 小工具 ----------

fn as_f32(v: &Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}

fn series<'a>(root: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    root.get(key).and_then(|v| v.as_array())
}

fn time_at(root: &Value, i: usize) -> Option<String> {
    series(root, "time")?
        .get(i)
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// 取变量序列：多模型时带模型后缀，**单模型时 Open-Meteo 不带后缀**。
fn series_for<'a>(root: &'a Value, var: &str, model: &str) -> Option<&'a Vec<Value>> {
    series(root, &format!("{var}_{model}")).or_else(|| series(root, var))
}

/// 取某变量在多个模式下的**中位数**序列。
///
/// 取中位数而非算术平均：各模式常常一个报晴、一个报阴，平均会落到两者都不预测的
/// 「中间态」；模式越多中位数越稳。
fn merged(root: &Value, var: &str, n: usize, models: &[&str]) -> Vec<Option<f32>> {
    let mut out = vec![None; n];
    for i in 0..n {
        let mut vals: Vec<f32> = models
            .iter()
            .filter_map(|&m| series_for(root, var, m).and_then(|a| a.get(i)).and_then(as_f32))
            .collect();
        if vals.is_empty() {
            continue;
        }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let k = vals.len();
        out[i] = Some(if k % 2 == 1 {
            vals[k / 2]
        } else {
            (vals[k / 2 - 1] + vals[k / 2]) / 2.0
        });
    }
    out
}

/// 日尺度时间（如 sunrise）：优先列表靠后的模式，其次前面的。
fn daily_time(root: &Value, base: &str, i: usize, models: &[&str]) -> Option<String> {
    for &m in models.iter().rev() {
        if let Some(arr) = series_for(root, base, m) {
            if let Some(s) = arr.get(i).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

/// ISO 本地时间 "YYYY-MM-DDTHH:MM" → (date, hour, minute)。
fn split_iso(s: &str) -> Option<(&str, u32, u32)> {
    let (date, rest) = s.split_once('T')?;
    let hour: u32 = rest.get(0..2)?.parse().ok()?;
    let min: u32 = rest.get(3..5).and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((date, hour, min))
}

/// 模式短名（展示用）。
fn short_name(m: &str) -> &'static str {
    match m {
        "gfs_seamless" => "GFS",
        "ecmwf_ifs025" => "ECMWF",
        _ => "模型",
    }
}

/// "YYYY-MM-DDTHH:MM" → "HH:MM"。
fn clock(s: &str) -> String {
    s.split_once('T')
        .map(|(_, r)| r.get(0..5).unwrap_or(r).to_string())
        .unwrap_or_else(|| s.to_string())
}

// ---------- 组装 ----------

// ---------- 逐时序列集合与事件锚点 ----------

/// 一组逐时变量（已按所选模式融合）。
#[derive(Clone, Copy)]
struct Series<'a> {
    low: &'a [Option<f32>],
    mid: &'a [Option<f32>],
    high: &'a [Option<f32>],
    total: &'a [Option<f32>],
    visibility: &'a [Option<f32>],
    /// 日照时长（秒/小时）
    sunshine: &'a [Option<f32>],
    humidity: &'a [Option<f32>],
    rain: &'a [Option<f32>],
}

/// 事件时刻的锚点：所在小时索引 + 小时内比例（分钟/60）。
fn event_anchor(times: &[String], date: &str, event_iso: Option<&str>) -> Option<(usize, f32)> {
    let (target_hour, frac) = match event_iso.and_then(split_iso) {
        Some((_, h, m)) => (h, m as f32 / 60.0),
        None => (18, 0.0),
    };
    let mut best: Option<(u32, usize)> = None;
    for (i, t) in times.iter().enumerate() {
        if let Some((d, h, _)) = split_iso(t) {
            if d == date {
                let dist = h.abs_diff(target_hour);
                if best.map_or(true, |(bd, _)| dist < bd) {
                    best = Some((dist, i));
                }
            }
        }
    }
    best.map(|(_, i)| (i, frac))
}

/// 取事件**时刻**的值：在事件所在小时与下一小时之间按分钟线性插值。
///
/// 早期版本取 `[h-1, h+2]` 四小时均值，会把日落之后两小时（天已黑）的数据等权重算进来。
fn at_event(arr: &[Option<f32>], idx: usize, frac: f32) -> Option<f32> {
    let a = arr.get(idx).copied().flatten();
    let b = arr.get(idx + 1).copied().flatten();
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * frac),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// 在给定锚点上组装一份评分输入。
fn inputs_at(s: &Series, idx: usize, frac: f32, aod: Option<f32>) -> Inputs {
    Inputs {
        cloud_low: at_event(s.low, idx, frac),
        cloud_mid: at_event(s.mid, idx, frac),
        cloud_high: at_event(s.high, idx, frac),
        cloud_total: at_event(s.total, idx, frac),
        humidity: at_event(s.humidity, idx, frac),
        visibility_km: at_event(s.visibility, idx, frac).map(|m| m / 1000.0),
        aod,
        rain_prob: at_event(s.rain, idx, frac),
        sunshine_frac: at_event(s.sunshine, idx, frac).map(|s| s / 3600.0),
    }
}

/// 在给定锚点上算一个事件的评分。
fn score_at(s: &Series, idx: usize, frac: f32, aod: Option<f32>) -> EventScore {
    score::compute(&inputs_at(s, idx, frac, aod))
}

/// 只用其中**一个**模式重算某事件的评分（用于展示模式分歧）。
///
/// 能见度只有 GFS 提供（ECMWF 全为 null），因此比较模式分歧时统一用融合后的能见度，
/// 否则分歧里会混进「ECMWF 少一个变量」这类数据缺失的噪声。
#[allow(clippy::too_many_arguments)]
fn model_score(
    hourly: &Value,
    n: usize,
    model: &str,
    anchor: Option<(usize, f32)>,
    times: &[String],
    aod_map: &HashMap<String, f32>,
    visibility: &[Option<f32>],
) -> Option<f32> {
    let anchor = anchor?;
    let low = merged(hourly, "cloud_cover_low", n, &[model]);
    let mid = merged(hourly, "cloud_cover_mid", n, &[model]);
    let high = merged(hourly, "cloud_cover_high", n, &[model]);
    let total = merged(hourly, "cloud_cover", n, &[model]);
    let humidity = merged(hourly, "relative_humidity_2m", n, &[model]);
    let rain = merged(hourly, "precipitation_probability", n, &[model]);
    let sunshine = merged(hourly, "sunshine_duration", n, &[model]);
    let s = Series {
        low: &low,
        mid: &mid,
        high: &high,
        total: &total,
        visibility,
        sunshine: &sunshine,
        humidity: &humidity,
        rain: &rain,
    };
    let (idx, frac) = anchor;
    let aod = times.get(idx).and_then(|t| aod_map.get(t).copied());
    Some(score_at(&s, idx, frac, aod).score)
}

/// 把各模式评分写进结果：记录分歧（用于展示，以及无集合数据时的置信度折扣）。
/// 只把前两个模式留给展示，避免 8 个模式把界面撑爆。
fn apply_models(sc: &mut EventScore, scores: Vec<(String, f32)>) {
    if scores.len() < 2 {
        return;
    }
    let lo = scores.iter().map(|(_, v)| *v).fold(f32::INFINITY, f32::min);
    let hi = scores
        .iter()
        .map(|(_, v)| *v)
        .fold(f32::NEG_INFINITY, f32::max);
    sc.spread = (hi - lo).clamp(0.0, 1.0);
    sc.model_scores = scores.into_iter().take(2).collect();
}

/// 集合成员逐个评分：云量用成员自己的，其余变量沿用多模式中位状态。
/// 返回 `(分数, 完整评分)`，以便取中位成员当代表。
fn ens_members(ens: &Value, anchor: Option<(usize, f32)>, base: &Inputs) -> Vec<(f32, EventScore)> {
    let (Some(hourly), Some((idx, frac))) = (ens.get("hourly"), anchor) else {
        return Vec::new();
    };
    let Some(obj) = hourly.as_object() else {
        return Vec::new();
    };
    let mut suffixes: Vec<&str> = obj
        .keys()
        .filter_map(|k| k.strip_prefix("cloud_cover_low"))
        .collect();
    suffixes.sort_unstable();
    let mut out = Vec::with_capacity(suffixes.len());
    for suf in suffixes {
        let arr = |var: &str| -> Vec<Option<f32>> {
            series(hourly, &format!("{var}{suf}"))
                .map(|a| a.iter().map(as_f32).collect())
                .unwrap_or_default()
        };
        let inp = Inputs {
            cloud_low: at_event(&arr("cloud_cover_low"), idx, frac),
            cloud_mid: at_event(&arr("cloud_cover_mid"), idx, frac),
            cloud_high: at_event(&arr("cloud_cover_high"), idx, frac),
            cloud_total: at_event(&arr("cloud_cover"), idx, frac),
            humidity: base.humidity,
            visibility_km: base.visibility_km,
            aod: base.aod,
            rain_prob: base.rain_prob,
            sunshine_frac: base.sunshine_frac,
        };
        if inp.cloud_total.is_none() && inp.cloud_low.is_none() {
            continue;
        }
        let sc = score::compute(&inp);
        out.push((sc.score, sc));
    }
    out
}

/// 「值得出工」阈值，与 `EventScore::grade()` 的「很好 · 值得出工」一致。
const WORTH: f32 = 0.60;

/// 集合成员 → 以**中位成员**为代表（这样因子明细与总分自洽，不是「平均输入再打分」），
/// 并填入概率 / 中位数 / 区间，置信度按「与主结论相左的成员比例」打折。
fn ens_representative(
    ens: &Value,
    anchor: Option<(usize, f32)>,
    base: &Inputs,
    confidence_base: f32,
) -> Option<EventScore> {
    let mut members = ens_members(ens, anchor, base);
    if members.len() < 5 {
        return None;
    }
    members.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = members.len();
    let median = if n % 2 == 1 {
        members[n / 2].0
    } else {
        (members[n / 2 - 1].0 + members[n / 2].0) / 2.0
    };
    let prob = members.iter().filter(|m| m.0 >= WORTH).count() as f32 / n as f32;
    let (rep_score, mut rep) = members[n / 2].clone();
    let agree = if rep_score >= WORTH { prob } else { 1.0 - prob };
    rep.ens_prob = Some(prob);
    rep.ens_median = Some(median);
    rep.ens_range = Some((members[0].0, members[n - 1].0));
    rep.ens_count = n;
    rep.confidence = (confidence_base * (1.0 - (1.0 - agree) * 0.5)).clamp(0.05, 1.0);
    Some(rep)
}

fn build_bundle(
    loc: &Location,
    fc: &Value,
    aq: Option<&Value>,
    ens: Option<&Value>,
    models: &[&str],
) -> Result<ForecastBundle, String> {
    let hourly = fc.get("hourly").ok_or("预报缺少 hourly 字段")?;
    let daily = fc.get("daily").ok_or("预报缺少 daily 字段")?;

    let n = series(hourly, "time").map(|a| a.len()).unwrap_or(0);
    if n == 0 {
        return Err("预报小时序列为空".into());
    }

    let cloud_low = merged(hourly, "cloud_cover_low", n, models);
    let cloud_mid = merged(hourly, "cloud_cover_mid", n, models);
    let cloud_high = merged(hourly, "cloud_cover_high", n, models);
    let cloud_total = merged(hourly, "cloud_cover", n, models);
    let visibility = merged(hourly, "visibility", n, models);
    let humidity = merged(hourly, "relative_humidity_2m", n, models);
    let rain = merged(hourly, "precipitation_probability", n, models);
    let sunshine = merged(hourly, "sunshine_duration", n, models);

    let times: Vec<String> = (0..n)
        .filter_map(|i| time_at(hourly, i))
        .collect();

    // CAMS 气溶胶：按时间字符串对齐。
    let mut aod_map: HashMap<String, f32> = HashMap::new();
    if let Some(aq) = aq {
        if let Some(h) = aq.get("hourly") {
            let aq_times = series(h, "time");
            let aod = series(h, "aerosol_optical_depth");
            if let (Some(ts), Some(vals)) = (aq_times, aod) {
                for (i, t) in ts.iter().enumerate() {
                    if let (Some(t), Some(v)) = (t.as_str(), vals.get(i).and_then(as_f32)) {
                        aod_map.insert(t.to_string(), v);
                    }
                }
            }
        }
    }

    let daily_times = series(daily, "time")
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut days = Vec::new();
    for (di, date) in daily_times.iter().enumerate() {
        let sunrise_iso = daily_time(daily, "sunrise", di, models);
        let sunset_iso = daily_time(daily, "sunset", di, models);
        let sunrise_anchor = event_anchor(&times, date, sunrise_iso.as_deref());
        let sunset_anchor = event_anchor(&times, date, sunset_iso.as_deref());

        let s = Series {
            low: &cloud_low,
            mid: &cloud_mid,
            high: &cloud_high,
            total: &cloud_total,
            visibility: &visibility,
            sunshine: &sunshine,
            humidity: &humidity,
            rain: &rain,
        };
        let mk = |anchor: Option<(usize, f32)>| -> (EventScore, Option<Inputs>) {
            match anchor {
                None => (EventScore::default(), None),
                Some((i, f)) => {
                    let inp = inputs_at(
                        &s,
                        i,
                        f,
                        times.get(i).and_then(|t| aod_map.get(t).copied()),
                    );
                    let sc = score::compute(&inp);
                    (sc, Some(inp))
                }
            }
        };
        let (mut sunrise_score, sunrise_inp) = mk(sunrise_anchor);
        let (mut sunset_score, sunset_inp) = mk(sunset_anchor);

        // 多模式集合：各模式独立评分 + 集合概率
        if models.len() > 1 {
            for (anchor, sc, base) in [
                (sunrise_anchor, &mut sunrise_score, sunrise_inp.as_ref()),
                (sunset_anchor, &mut sunset_score, sunset_inp.as_ref()),
            ] {
                let scores = models
                    .iter()
                    .filter_map(|&m| {
                        model_score(hourly, n, m, anchor, &times, &aod_map, &visibility)
                            .map(|v| (short_name(m).to_string(), v))
                    })
                    .collect::<Vec<_>>();
                apply_models(sc, scores);
                let spread = sc.spread;
                let shown = sc.model_scores.clone();
                let availability = sc.confidence;
                // 集合可用时以中位成员为主口径；否则回退到中位输入的状态
                if let (Some(ens), Some(base)) = (ens, base) {
                    if let Some(rep) = ens_representative(ens, anchor, base, availability) {
                        *sc = rep;
                    }
                }
                if sc.ens_prob.is_none() {
                    sc.confidence = (availability * (1.0 - spread * 0.6)).clamp(0.05, 1.0);
                }
                sc.spread = spread;
                sc.model_scores = shown;
            }
        }

        days.push(DayForecast {
            date: date.clone(),
            sunrise: sunrise_iso.as_deref().map(clock),
            sunset: sunset_iso.as_deref().map(clock),
            sunrise_score,
            sunset_score,
        });
    }

    Ok(ForecastBundle {
        location: loc.clone(),
        days,
        utc_offset_seconds: fc
            .get("utc_offset_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    })
}

/// 反向地理编码，用于给地图选点起个名字。失败返回 None。
pub fn place_name(lat: f64, lon: f64) -> Option<String> {
    // Open-Meteo 地理编码不提供反向查询；改用 Nominatim。
    nominatim_reverse(lat, lon)
}

fn nominatim_reverse(lat: f64, lon: f64) -> Option<String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat={lat:.4}&lon={lon:.4}&zoom=10&accept-language=zh-CN"
    );
    let resp = ureq::get(&url)
        .set("User-Agent", UA)
        .timeout(Duration::from_secs(15))
        .call()
        .ok()?;
    let v: Value = resp.into_json().ok()?;
    let addr = v.get("address")?;
    for key in ["city", "town", "county", "state_district", "state", "country"] {
        if let Some(s) = addr.get(key).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    v.get("display_name")
        .and_then(|x| x.as_str())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
}

/// 名称检索（正向地理编码）。
pub fn search_place(query: &str) -> Result<Vec<Location>, String> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=8&language=zh&format=json",
        urlencode(query)
    );
    let v = get_json(&url, 2, 10)?;
    let mut out = Vec::new();
    if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
        for r in results {
            let name = r.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let admin = r
                .get("admin1")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let country = r.get("country").and_then(|x| x.as_str()).unwrap_or("");
            let label = [name, admin, country]
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ");
            if let (Some(lat), Some(lon)) = (
                r.get("latitude").and_then(|x| x.as_f64()),
                r.get("longitude").and_then(|x| x.as_f64()),
            ) {
                out.push(Location {
                    name: label,
                    lat,
                    lon,
                });
            }
        }
    }
    Ok(out)
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
