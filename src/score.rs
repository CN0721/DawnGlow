//! 火烧云（朝霞/晚霞）鲜艳度评分。
//!
//! 移植并扩展 r-ayin/sunset-prediction (MIT) 的研究驱动型多因子模型：
//! 云型配置、能见度、湿度、降水、总云量五个因子，另把 CAMS 的气溶胶光学厚度
//! (AOD) 作为独立因子纳入「大气通透度」评估（Henriksson 2019 / Chen 2022 指出
//! AOD 是影响晚霞观感的最重要单一特征之一）。

use crate::model::{CloudType, EventScore, Factor};

/// 评分输入（已在事件时段做过多源融合）。
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

fn v(x: Option<f32>, fallback: f32) -> f32 {
    x.unwrap_or(fallback)
}

/// 计算单个事件的鲜艳度评分（0..1）。
pub fn compute(inp: &Inputs) -> EventScore {
    let high = v(inp.cloud_high, f32::NAN);
    let total = v(inp.cloud_total, 50.0);

    let mut score = 0.0_f32;
    let mut factors: Vec<Factor> = Vec::new();

    // ── 1. 云型配置 (~35%) ─────────────────────────────────────
    let high_dominant = inp.cloud_high.map_or(false, |h| h >= 30.0)
        && inp.cloud_low.map_or(true, |l| l < 40.0);
    let low_dominant = inp.cloud_low.map_or(false, |l| (20.0..=55.0).contains(&l))
        && inp.cloud_high.map_or(true, |h| h < 40.0);
    let multi_layer = inp.cloud_high.map_or(false, |h| h > 15.0)
        && inp.cloud_low.map_or(false, |l| l > 10.0);
    let overcast = inp.cloud_total.map_or(false, |t| t > 80.0);
    let clear_sky = inp.cloud_total.map_or(false, |t| t < 10.0);

    let (cloud_type, ct_score) = if high_dominant && !overcast {
        (CloudType::HighDominant, 0.40)
    } else if low_dominant && !overcast {
        (CloudType::LowDominant, 0.28)
    } else if (10.0..=75.0).contains(&total) {
        (CloudType::Mixed, 0.22)
    } else if clear_sky {
        (CloudType::Clear, 0.05)
    } else if overcast {
        (CloudType::Overcast, -0.10)
    } else {
        (CloudType::Mixed, 0.12)
    };
    score += ct_score;
    factors.push(Factor {
        label: "云型配置".into(),
        value: cloud_type.label().into(),
        delta: ct_score,
    });

    if multi_layer {
        score += 0.08;
        factors.push(Factor {
            label: "多层云加分".into(),
            value: "高/低云并存，纹理丰富".into(),
            delta: 0.08,
        });
    } else if cloud_type == CloudType::HighDominant && high > 40.0 {
        score += 0.04;
        factors.push(Factor {
            label: "高云纹理".into(),
            value: "卷云纹理细腻".into(),
            delta: 0.04,
        });
    }

    // ── 2. 能见度 (~25%，AOD 代理) ─────────────────────────────
    let vis_delta = match inp.visibility_km {
        None => 0.0,
        Some(km) if km >= 20.0 => 0.18,
        Some(km) if km >= 12.0 => 0.12,
        Some(km) if km >= 6.0 => 0.05,
        Some(_) => -0.08,
    };
    score += vis_delta;
    factors.push(Factor {
        label: "能见度".into(),
        value: inp
            .visibility_km
            .map_or("无数据".into(), |km| format!("{km:.1} km")),
        delta: vis_delta,
    });

    // ── 2b. 气溶胶 (CAMS AOD) ─────────────────────────────────
    // 适量气溶胶增强暖色散射；过浓则灰蒙。
    let aod_delta = match inp.aod {
        None => 0.0,
        Some(a) if a <= 0.10 => 0.02,
        Some(a) if a <= 0.40 => 0.06,
        Some(a) if a <= 0.60 => 0.03,
        Some(a) if a <= 0.80 => 0.0,
        Some(_) => -0.08,
    };
    score += aod_delta;
    factors.push(Factor {
        label: "气溶胶 AOD".into(),
        value: inp.aod.map_or("无数据".into(), |a| format!("{a:.2}")),
        delta: aod_delta,
    });

    // ── 3. 湿度 (~15%) ────────────────────────────────────────
    let hum_delta = match inp.humidity {
        None => 0.0,
        Some(h) if (40.0..=60.0).contains(&h) => 0.15,
        Some(h) if (30.0..40.0).contains(&h) || (60.0..=75.0).contains(&h) => 0.08,
        Some(h) if h > 85.0 => -0.10,
        Some(_) => 0.04,
    };
    score += hum_delta;
    factors.push(Factor {
        label: "相对湿度".into(),
        value: inp.humidity.map_or("无数据".into(), |h| format!("{h:.0}%")),
        delta: hum_delta,
    });

    // ── 4. 降水惩罚 (~10%) ────────────────────────────────────
    let rain_delta = match inp.rain_prob {
        None => 0.0,
        Some(r) if r > 50.0 => -0.15,
        Some(r) if r > 25.0 => -0.08,
        Some(_) => 0.0,
    };
    score += rain_delta;
    factors.push(Factor {
        label: "降水概率".into(),
        value: inp.rain_prob.map_or("无数据".into(), |r| format!("{r:.0}%")),
        delta: rain_delta,
    });

    // ── 5. 总云量修正 (~15%) ──────────────────────────────────
    let tc = inp.cloud_total;
    if let Some(tc) = tc {
        if tc > 75.0 {
            let penalty = -0.08 * ((tc - 75.0) / 25.0);
            score += penalty;
            factors.push(Factor {
                label: "总云量修正".into(),
                value: format!("{tc:.0}% 过厚"),
                delta: penalty,
            });
        } else if (15.0..=60.0).contains(&tc) {
            score += 0.05;
            factors.push(Factor {
                label: "总云量修正".into(),
                value: format!("{tc:.0}% 最优区间"),
                delta: 0.05,
            });
        }
    }

    let score = score.clamp(0.0, 1.0);

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
    let confidence = (data_points as f32 / 7.0 + 0.1).min(1.0);

    EventScore {
        score,
        confidence,
        cloud_low: inp.cloud_low,
        cloud_mid: inp.cloud_mid,
        cloud_high: inp.cloud_high,
        cloud_total: inp.cloud_total,
        humidity: inp.humidity,
        visibility_km: inp.visibility_km,
        aod: inp.aod,
        rain_prob: inp.rain_prob,
        cloud_type,
        factors,
    }
}
