//! 桌面小组件所需的朝晚霞摘要文本与离线缓存。
//!
//! 小组件由 Java 侧（`GlowWidgetProvider`）调用 JNI 入口
//! `WidgetBridge.forecast`，最终落到这里复用同一条数据与评分链路。
//!
//! 输出为「每行一个字段」的 9 行文本，Java 侧按行填充到对应控件：
//! 0 日期 / 1 朝霞时间 / 2 朝霞鲜艳度(整数) / 3 晚霞时间 / 4 晚霞鲜艳度(整数) /
//! 5 分层云量 / 6 气溶胶+能见度 / 7 湿度+降水 / 8 主事件+云型。
//! 进度条由 Java 侧用系统 `ProgressBar` 绘制（RemoteViews 不支持自绘）。

use std::path::Path;

use crate::api;
use crate::model::{EventKind, ForecastBundle, Location};

const CACHE_FILE: &str = "widget.txt";

/// 拉取并生成小组件文本；失败时退回上次缓存。
pub fn summary(lat: f64, lon: f64, files_dir: &Path) -> String {
    let loc = Location {
        name: String::new(),
        lat,
        lon,
    };
    match api::fetch_bundle(&loc, crate::api::ModelMode::Blend) {
        Ok(b) => {
            let text = format_bundle(&b);
            write_cache(files_dir, &text);
            text
        }
        Err(e) => match std::fs::read_to_string(files_dir.join(CACHE_FILE)) {
            Ok(t) if !t.trim().is_empty() => match t.split_once('\n') {
                Some((first, rest)) => format!("{first} · 离线\n{rest}"),
                None => format!("{t} · 离线"),
            },
            _ => format!("获取失败\n{e}"),
        },
    }
}

/// 主 App 拉取成功后写入缓存，供小组件离线显示。
pub fn cache_bundle(files_dir: &Path, b: &ForecastBundle) {
    write_cache(files_dir, &format_bundle(b));
}

fn write_cache(files_dir: &Path, text: &str) {
    let _ = std::fs::create_dir_all(files_dir);
    let _ = std::fs::write(files_dir.join(CACHE_FILE), text);
}

fn pct(score: f32) -> i32 {
    (score * 100.0).round() as i32
}

fn pct_str(v: Option<f32>) -> String {
    v.map(|x| format!("{x:.0}%")).unwrap_or_else(|| "--".into())
}

/// 首日预报 → 9 行字段（供 RemoteViews 逐项展示）。
pub fn format_bundle(b: &ForecastBundle) -> String {
    let Some(day) = b.days.first() else {
        return "暂无数据".into();
    };
    let md = day.date.get(5..).unwrap_or(day.date.as_str());
    let sr = &day.sunrise_score;
    let ss = &day.sunset_score;

    let sunrise_time = day.sunrise.as_deref().unwrap_or("--:--");
    let sunset_time = day.sunset.as_deref().unwrap_or("--:--");
    let sunrise_pct = pct(sr.score).to_string();
    let sunset_pct = pct(ss.score).to_string();

    // 细节行取「更优事件」（分数高者），与末行主事件一致。
    let best = day.best_event();
    let detail = day.score_for(best);
    let best_label = match best {
        EventKind::Sunrise => "朝霞",
        EventKind::Sunset => "晚霞",
    };

    let clouds = format!(
        "云量 高{} · 中{} · 低{} · 总{}",
        pct_str(detail.cloud_high),
        pct_str(detail.cloud_mid),
        pct_str(detail.cloud_low),
        pct_str(detail.cloud_total),
    );

    let aod = detail.aod.map(|a| format!("{a:.2}")).unwrap_or_else(|| "--".into());
    let vis = detail
        .visibility_km
        .map(|v| format!("{v:.0}km"))
        .unwrap_or_else(|| "--".into());
    let env1 = format!("气溶胶 {aod} · 能见度 {vis}");

    let hum = pct_str(detail.humidity);
    let rain = pct_str(detail.rain_prob);
    let env2 = format!("湿度 {hum} · 降水 {rain}");

    let last = format!(
        "{best_label} · {} · 置信 {:.0}%",
        detail.cloud_type.label(),
        detail.confidence * 100.0
    );

    [
        md.to_string(),
        sunrise_time.to_string(),
        sunrise_pct,
        sunset_time.to_string(),
        sunset_pct,
        clouds,
        env1,
        env2,
        last,
    ]
    .join("\n")
}
