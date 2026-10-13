//! 火烧云（朝霞/晚霞）鲜艳度评分。
//!
//! 移植并扩展 r-ayin/sunset-prediction (MIT) 的研究驱动型多因子模型：云型配置、通透度、
//! 湿度、降水、总云量五个因子，另把 CAMS 的气溶胶光学厚度 (AOD) 作为独立因子纳入
//! 「大气通透度」评估（Henriksson 2019 / Chen 2022 指出 AOD 是影响晚霞观感的最重要
//! 单一特征之一）。
//!
//! 所有因子都是**连续**打分（分段线性），不设阈值分档：早期版本用 if/else 把云型分成
//! 五档，相邻档差 15–50 分，云量只要跨过阈值评分就跳一大截，GFS 与 ECMWF 平均后还会
//! 恰好落进惩罚档。现在数值连续变化，评分曲线没有悬崖。

use crate::model::{CloudType, EventScore, Factor};

/// 评分输入（已在事件时刻做过多源融合）。
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub cloud_low: Option<f32>,
    pub cloud_mid: Option<f32>,
    pub cloud_high: Option<f32>,
    pub cloud_total: Option<f32>,
    pub humidity: Option<f32>,
    pub visibility_km: Option<f32>,
    pub aod: Option<f32>,
    pub rain_prob: Option<f32>,
}

/// 分段线性插值，`pts` 需按 x 升序。
fn curve(x: f32, pts: &[(f32, f32)]) -> f32 {
    match pts.first() {
        None => 0.0,
        Some(&(x0, y0)) if x <= x0 => y0,
        _ => {
            for w in pts.windows(2) {
                let (x0, y0) = w[0];
                let (x1, y1) = w[1];
                if x <= x1 {
                    let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
                    return y0 + (y1 - y0) * t;
                }
            }
            pts[pts.len() - 1].1
        }
    }
}

/// 高云（卷云/卷积云）贡献：三到八成最理想，接近全阴时回落。
fn high_cloud_score(high: f32) -> f32 {
    curve(
        high,
        &[
            (0.0, 0.0),
            (10.0, 0.10),
            (30.0, 0.30),
            (55.0, 0.34),
            (80.0, 0.30),
            (95.0, 0.10),
            (100.0, 0.0),
        ],
    )
}

/// 低云（层积云）贡献：二到五成最好，过少无云可染、过厚遮光。
fn low_cloud_score(low: f32) -> f32 {
    curve(
        low,
        &[
            (0.0, 0.0),
            (15.0, 0.05),
            (25.0, 0.14),
            (40.0, 0.18),
            (55.0, 0.15),
            (70.0, 0.04),
            (85.0, -0.06),
            (100.0, -0.10),
        ],
    )
}

/// 总云量修正：过厚扣分，晴空略扣，一成半到六成最优。
fn total_cloud_adj(total: f32) -> f32 {
    curve(
        total,
        &[
            (0.0, -0.06),
            (8.0, -0.02),
            (15.0, 0.05),
            (45.0, 0.05),
            (60.0, 0.03),
            (75.0, 0.0),
            (90.0, -0.06),
            (100.0, -0.10),
        ],
    )
}

/// 多层云加分：高、低云同时存在时纹理最丰富，按两者中较弱的一方加权。
fn multi_layer_bonus(high: Option<f32>, low: Option<f32>) -> f32 {
    match (high, low) {
        (Some(h), Some(l)) => {
            let a = curve(
                h,
                &[(0.0, 0.0), (15.0, 0.0), (35.0, 1.0), (60.0, 1.0), (100.0, 0.6)],
            );
            let b = curve(
                l,
                &[(0.0, 0.0), (10.0, 0.0), (25.0, 1.0), (60.0, 1.0), (100.0, 0.5)],
            );
            a.min(b) * 0.07
        }
        _ => 0.0,
    }
}

/// 由连续量推出一个「最接近」的云型，仅用于展示文案。
fn classify(inp: &Inputs) -> CloudType {
    let (high, low, total) = (inp.cloud_high, inp.cloud_low, inp.cloud_total);
    if total.map_or(false, |t| t > 80.0)
        || (low.map_or(false, |l| l > 75.0) && high.map_or(true, |h| h < 20.0))
    {
        CloudType::Overcast
    } else if total.map_or(false, |t| t < 10.0) {
        CloudType::Clear
    } else if high.map_or(false, |h| h >= 30.0) && low.map_or(true, |l| l < 40.0) {
        CloudType::HighDominant
    } else if low.map_or(false, |l| (20.0..=55.0).contains(&l)) && high.map_or(true, |h| h < 40.0) {
        CloudType::LowDominant
    } else {
        CloudType::Mixed
    }
}

fn push(score: &mut f32, factors: &mut Vec<Factor>, label: &str, value: String, delta: f32) {
    *score += delta;
    factors.push(Factor {
        label: label.into(),
        value,
        delta,
    });
}

/// 计算单个事件的鲜艳度评分（0..1）。
///
/// `confidence` 在这里只反映「数据是否齐全」；模式分歧的折扣由数据层施加。
pub fn compute(inp: &Inputs) -> EventScore {
    let mut score = 0.0_f32;
    let mut factors: Vec<Factor> = Vec::new();

    // ── 1. 云型配置（高云 + 低云，连续）──
    let ct_score = inp.cloud_high.map_or(0.0, high_cloud_score)
        + inp.cloud_low.map_or(0.0, low_cloud_score);
    push(
        &mut score,
        &mut factors,
        "云型配置",
        classify(inp).label().into(),
        ct_score,
    );

    // 多层云加分
    let multi = multi_layer_bonus(inp.cloud_high, inp.cloud_low);
    if multi > 0.01 {
        push(
            &mut score,
            &mut factors,
            "多层云加分",
            "高/低云并存，纹理丰富".into(),
            multi,
        );
    }

    // ── 2. 通透度：低能见度（霾/雾/沙尘）扣分 ──
    // ⚠ 不用 `sunshine_duration`/`direct_radiation`：实测 GFS 的这两个场与云量自相矛盾
    // （2026-10-17 17:00 总云 100% 却报日照 3600s；阴天与晴天在日出后一小时的直射占比
    // 都是 0.50–0.57，没有区分度），而且 `sunshine_duration` 在标签 T 处统计 [T-1, T]，
    // 日出那一小时全是黑夜，会把晴天日出白扣满。
    // 也**不**拿能见度当常态加分：GFS 的 visibility 在多数地区常年顶在 24140 m 上限。
    let haze_delta = inp.visibility_km.map_or(0.0, |km| {
        if km >= 20.0 {
            0.0
        } else {
            curve(km, &[(0.0, -0.10), (3.0, -0.08), (8.0, -0.04), (20.0, 0.0)])
        }
    });
    let vis_text = match inp.visibility_km {
        None => "无数据".to_string(),
        Some(km) if km >= 20.0 => "空气质量良好".to_string(),
        Some(km) => format!("能见 {km:.0} km"),
    };
    push(&mut score, &mut factors, "通透度", vis_text, haze_delta);

    // ── 2b. 气溶胶（CAMS AOD）：适量增强暖色散射，过浓则灰蒙 ──
    let aod_delta = inp.aod.map_or(0.0, |a| {
        curve(
            a,
            &[
                (0.0, 0.02),
                (0.10, 0.02),
                (0.25, 0.05),
                (0.40, 0.06),
                (0.60, 0.03),
                (0.80, 0.0),
                (1.0, -0.08),
                (2.0, -0.08),
            ],
        )
    });
    push(
        &mut score,
        &mut factors,
        "气溶胶 AOD",
        inp.aod.map_or("无数据".into(), |a| format!("{a:.2}")),
        aod_delta,
    );

    // ── 3. 湿度 ──
    let hum_delta = inp.humidity.map_or(0.0, |h| {
        curve(
            h,
            &[
                (0.0, 0.04),
                (30.0, 0.04),
                (40.0, 0.15),
                (60.0, 0.15),
                (75.0, 0.08),
                (85.0, -0.10),
                (100.0, -0.10),
            ],
        )
    });
    push(
        &mut score,
        &mut factors,
        "相对湿度",
        inp.humidity.map_or("无数据".into(), |h| format!("{h:.0}%")),
        hum_delta,
    );

    // ── 4. 降水 ──
    let rain_delta = inp.rain_prob.map_or(0.0, |r| {
        curve(
            r,
            &[(0.0, 0.0), (25.0, 0.0), (35.0, -0.08), (50.0, -0.15), (100.0, -0.15)],
        )
    });
    push(
        &mut score,
        &mut factors,
        "降水概率",
        inp.rain_prob.map_or("无数据".into(), |r| format!("{r:.0}%")),
        rain_delta,
    );

    // ── 5. 总云量修正 ──
    if let Some(tc) = inp.cloud_total {
        push(
            &mut score,
            &mut factors,
            "总云量修正",
            format!("{tc:.0}%"),
            total_cloud_adj(tc),
        );
    }

    let data_points = [
        inp.cloud_low,
        inp.cloud_mid,
        inp.cloud_high,
        inp.cloud_total,
        inp.humidity,
        inp.visibility_km,
        inp.aod,
    ]
    .iter()
    .filter(|x| x.is_some())
    .count();

    EventScore {
        score: score.clamp(0.0, 1.0),
        confidence: (data_points as f32 / 7.0 + 0.1).min(1.0),
        cloud_low: inp.cloud_low,
        cloud_mid: inp.cloud_mid,
        cloud_high: inp.cloud_high,
        cloud_total: inp.cloud_total,
        humidity: inp.humidity,
        visibility_km: inp.visibility_km,
        aod: inp.aod,
        rain_prob: inp.rain_prob,
        cloud_type: classify(inp),
        factors,
        model_scores: Vec::new(),
        spread: 0.0,
        ens_prob: None,
        ens_median: None,
        ens_range: None,
        ens_count: 0,
    }
}
