//! 配色与渐变。对齐设计稿：近黑底 + 蓝紫品牌色，暖色只留给天空卡片。

use egui::{Color32, Shadow};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub bg_top: Color32,
    pub fg: Color32,
    /// 次级文字（正文说明）
    pub dim: Color32,
    /// 三级文字（标签 / 数值 / 页脚）
    pub fg3: Color32,
    /// 品牌色（选中、开关、滑块、链接）
    pub accent: Color32,
    pub accent_soft: Color32,
    /// 强调文字（与品牌色同调，用于数字与高亮）
    pub gold: Color32,
    /// 朝霞色（日出）
    pub dawn: Color32,
    /// 晚霞色（日落）
    pub dusk: Color32,
    /// 警示色（雾霾 / 差）
    pub warn: Color32,

    pub card_fill: Color32,
    /// 行按下 / 悬停底色
    pub card_press: Color32,
    pub card_stroke: Color32,
    /// 分段控件底槽
    pub chip_fill: Color32,
    /// 分段控件滑块 / 次级按钮描边
    pub chip_stroke: Color32,
    /// 与卡片同级的实心表面（地图画布 / 面板底）
    pub surface: Color32,
    /// 轨道 / 底槽（进度条、评分环背景、地图按钮）
    pub track: Color32,
    /// 骨架屏底色 / 扫光高光
    pub skel: Color32,
    pub skel_hi: Color32,
    pub card_radius: f32,
    pub card_pad: f32,
    pub card_shadow: Shadow,
    /// 卡片之间的垂直间距
    pub gap: f32,
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn rgba(c: [u8; 3], a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], a)
}

pub fn palette() -> Palette {
    Palette {
        bg: rgb([0x08, 0x09, 0x0F]),
        bg_top: rgb([0x0A, 0x0C, 0x13]),
        fg: rgb([0xF2, 0xF4, 0xFA]),
        dim: rgb([0x9A, 0xA3, 0xB6]),
        fg3: rgb([0x64, 0x6D, 0x80]),
        accent: rgb([0x8E, 0xA2, 0xFF]),
        accent_soft: rgba([0x8E, 0xA2, 0xFF], 40),
        gold: rgb([0x8E, 0xA2, 0xFF]),
        dawn: rgb([0xFF, 0x9D, 0x6E]),
        dusk: rgb([0xFF, 0x7A, 0x59]),
        warn: rgb([0xFB, 0x71, 0x85]),

        card_fill: rgba([255, 255, 255], 12),
        card_press: rgba([255, 255, 255], 19),
        card_stroke: rgba([255, 255, 255], 20),
        chip_fill: rgba([255, 255, 255], 14),
        chip_stroke: rgba([255, 255, 255], 33),
        surface: rgb([0x15, 0x18, 0x21]),
        track: rgba([255, 255, 255], 41),
        skel: rgba([255, 255, 255], 20),
        skel_hi: rgba([255, 255, 255], 44),
        card_radius: 16.0,
        card_pad: 16.0,
        card_shadow: Shadow::NONE,
        gap: 12.0,
    }
}

/// 天空卡片的渐变（150°：深蓝 → 品红 → 暖橙）。
pub fn sky_gradient() -> [(f32, Color32); 3] {
    [
        (0.0, rgb([0x1A, 0x2B, 0x63])),
        (0.56, rgb([0xA4, 0x4F, 0x9C])),
        (1.0, rgb([0xFF, 0x9D, 0x6E])),
    ]
}

/// 鲜艳度 → 颜色（低=灰蓝，高=火红）。
pub fn score_color(p: &Palette, score: f32) -> Color32 {
    let s = score.clamp(0.0, 1.0);
    if s >= 0.75 {
        p.dusk
    } else if s >= 0.5 {
        p.dawn
    } else if s >= 0.3 {
        rgb([0xE8, 0xA8, 0x7C])
    } else if s >= 0.15 {
        p.dim
    } else {
        p.fg3
    }
}

/// 竖向渐变（顶部氛围 → 底色）。设计稿为近纯色深底，只保留极轻的顶部提亮。
pub fn vgrad(p: &Palette) -> Vec<(f32, Color32)> {
    vec![(0.0, p.bg_top), (0.45, p.bg), (1.0, p.bg)]
}
