//! 配色与质感令牌（单一「杂志感」主题）。
//!
//! 卡片质感（填充 / 描边 / 圆角 / 内边距 / 阴影 / 间距）与配色一并放在 [`Palette`]。

use egui::epaint::Shadow;
use egui::Color32;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub bg_top: Color32,
    pub fg: Color32,
    pub dim: Color32,
    /// 主强调色（朝霞橙金）
    pub accent: Color32,
    pub accent_soft: Color32,
    pub gold: Color32,
    /// 朝霞色（日出）
    pub dawn: Color32,
    /// 晚霞色（日落）
    pub dusk: Color32,
    /// 警示色（雾霾 / 差）
    pub warn: Color32,

    pub card_fill: Color32,
    pub card_stroke: Color32,
    /// 未选中控件 / 次级按钮的浅填充
    pub chip_fill: Color32,
    /// 次级按钮描边（比 card_stroke 明显）
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

/// 杂志感：左对齐、信息密度高；卡片保留清晰边界与足够留白。
pub fn palette() -> Palette {
    Palette {
        bg: rgb([0x0B, 0x10, 0x20]),
        bg_top: rgb([0x1A, 0x16, 0x33]),
        fg: rgb([0xF5, 0xF6, 0xFA]),
        dim: rgb([0x93, 0x98, 0xAD]),
        accent: rgb([0xFF, 0xB4, 0x54]),
        accent_soft: rgba([0xFF, 0xB4, 0x54], 52),
        gold: rgb([0xFF, 0xD4, 0x79]),
        dawn: rgb([0xFF, 0xB4, 0x54]),
        dusk: rgb([0xFF, 0x7A, 0x59]),
        warn: rgb([0xE5, 0x6B, 0x6B]),
        card_fill: rgba([255, 255, 255], 16),
        card_stroke: rgba([255, 255, 255], 14),
        chip_fill: rgba([255, 255, 255], 20),
        chip_stroke: rgba([255, 255, 255], 40),
        surface: rgb([0x14, 0x18, 0x24]),
        track: rgba([255, 255, 255], 30),
        skel: rgba([255, 255, 255], 16),
        skel_hi: rgba([255, 255, 255], 44),
        card_radius: 16.0,
        card_pad: 16.0,
        card_shadow: Shadow::NONE,
        gap: 11.0,
    }
}

/// 鲜艳度 → 颜色（低=灰蓝，高=火红）。
pub fn score_color(p: &Palette, score: f32) -> Color32 {
    let s = score.clamp(0.0, 1.0);
    if s >= 0.75 {
        p.dusk
    } else if s >= 0.5 {
        p.accent
    } else if s >= 0.3 {
        p.gold
    } else if s >= 0.15 {
        p.dim
    } else {
        Color32::from_rgb(0x6C, 0x71, 0x86)
    }
}

/// 竖向渐变（顶部氛围 → 底色）。
pub fn vgrad(p: &Palette) -> Vec<(f32, Color32)> {
    vec![(0.0, p.bg_top), (0.5, p.bg), (1.0, p.bg)]
}
