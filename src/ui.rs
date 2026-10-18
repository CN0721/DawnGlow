//! 界面绘制。
//!
//! 布局规范：所有区块都包在 `card()` 里（同一内边距/圆角），卡片间垂直间距一致。
//! 横向行按 `available_width` 计算列宽；**过长的文本不截断**，而是用 `wrapped()`
//! 在固定列宽内自动换行（先算出换行后高度再精确占位，绝不横向溢出）。

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, Pos2, Rect, Response, RichText, Sense, Stroke, Vec2};

use crate::app::DawnGlowApp;
use crate::api::ModelMode;
use crate::geo;
use crate::model::{DayForecast, EventKind, Screen};
use crate::theme::{self, Palette};

const TILE: f64 = 256.0;

// ---------- 视觉标尺（全站统一，勿再散落字面量）----------

/// 空态图标
const FS_EMOJI: f32 = 40.0;
/// 选项行 / 列表项名称
const FS_ITEM: f32 = 15.0;
/// 卡片小标题
const FS_SECTION: f32 = 14.0;
/// 正文 / 按钮
const FS_BODY: f32 = 13.0;
/// 说明文字
const FS_CAPTION: f32 = 12.0;
/// 极小标签 / 署名
const FS_MICRO: f32 = 10.0;

/// 胶囊按钮高度
const H_CHIP: f32 = 32.0;
/// 卡片内通栏主按钮高度
const H_BTN: f32 = 40.0;

/// 紧邻元素间距
const SP_XS: f32 = 4.0;
/// 卡片内默认间距
const SP_SM: f32 = 6.0;
/// 段落间距
const SP_MD: f32 = 8.0;
// ---------- 设计稿标尺（新界面统一用这一组，单位 dp）----------

/// 页面左右边距
const PAD_X: f32 = 20.0;
/// 页眉小字
const T_KICKER: f32 = 9.5;
/// 页面大标题
const T_H1: f32 = 28.0;
/// 标题下的说明
const T_SUB: f32 = 11.5;
/// 行标题
const T_ROW: f32 = 13.5;
/// 行内小字
const T_ROW_S: f32 = 10.5;
/// 行右侧数值
const T_VALUE: f32 = 11.5;
/// 分组标题
const T_GROUP: f32 = 10.5;
/// 分段控件文字
const T_SEG: f32 = 10.5;
/// 要素卡数值
const T_METRIC: f32 = 17.0;
/// 要素卡备注
const T_METRIC_S: f32 = 9.0;
/// 天数胶囊数值
const T_DAY: f32 = 15.0;
/// 天空卡分数
const T_SCORE: f32 = 52.0;
/// 天空卡等级
const T_QUALITY: f32 = 12.5;
/// 主按钮
const T_PRIMARY: f32 = 13.0;
/// 页脚小字
const T_FINE: f32 = 9.0;

pub fn draw(app: &mut DawnGlowApp, ctx: &egui::Context) {
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    gradient(&painter, rect, &app.p);
    apply_style(ctx, &app.p);

    let (l, t, r, b) = app.pad;
    let margin = egui::Margin {
        left: i8c(l + PAD_X),
        top: i8c(t),
        right: i8c(r + PAD_X),
        bottom: i8c(b),
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.inner_margin(margin))
        .show(ctx, |ui| {
            let limit = ui.max_rect();
            match app.screen {
                Screen::Forecast => forecast(app, ui, ctx),
                Screen::Map => map(app, ui, ctx),
                Screen::Settings => settings(app, ui),
                Screen::Font => font_screen(app, ui),
                Screen::Version => version(app, ui),
            }
            let used = ui.min_rect();
            if used.right() > limit.right() + 1.0 || used.left() < limit.left() - 1.0 {
                log::warn!(
                    "布局横向溢出: used=[{:.1},{:.1}] limit=[{:.1},{:.1}]",
                    used.left(),
                    used.right(),
                    limit.left(),
                    limit.right()
                );
            }
        });
}

fn i8c(v: f32) -> i8 {
    v.round().clamp(0.0, 120.0) as i8
}

/// 把所有控件的圆角设为与卡片一致（随主题动态变化）。
fn apply_style(ctx: &egui::Context, p: &Palette) {
    let mut style = (*ctx.style()).clone();
    let r: egui::CornerRadius = p.card_radius.into();
    for w in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        w.corner_radius = r;
    }
    ctx.set_style(style);
}

// ---------- 背景 ----------

fn gradient(painter: &egui::Painter, rect: Rect, p: &Palette) {
    let steps = 40;
    let mut mesh = egui::Mesh::default();
    let stops = theme::vgrad(p);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let c = lerp_stops(&stops, t);
        let y = rect.top() + rect.height() * t;
        mesh.colored_vertex(Pos2::new(rect.left(), y), c);
        mesh.colored_vertex(Pos2::new(rect.right(), y), c);
    }
    for i in 0..steps {
        let a = (i * 2) as u32;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn lerp_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t >= t0 && t <= t1 {
            let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return lerp_color(c0, c1, k);
        }
    }
    stops.last().map(|(_, c)| *c).unwrap_or(Color32::BLACK)
}

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

// ---------- 通用 ----------

/// 统一的卡片容器：内边距 / 圆角 / 填充 / 描边 / 阴影均来自主题。
fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut frame = egui::Frame::NONE
        .fill(p.card_fill)
        .corner_radius(p.card_radius)
        .inner_margin(p.card_pad)
        .shadow(p.card_shadow);
    if p.card_stroke != Color32::TRANSPARENT {
        frame = frame.stroke(Stroke::new(1.0_f32, p.card_stroke));
    }
    frame
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = SP_SM;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// 下载按钮：静止是矢量下载图标，下载中过渡为环形进度（`frac=None` 为不确定进度）。
///
/// egui 不解析 SVG，这里用矢量画法——同样清晰且跟随屏幕密度。
fn dl_button(ui: &mut egui::Ui, p: &Palette, salt: &str, busy: bool, frac: Option<f32>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(egui::Id::new(("dlbtn", salt)), busy, 0.35);
    let c = rect.center();
    let r = rect.width() * 0.5;
    let painter = ui.painter();

    // 图标态：圆角底 + 下箭头 + 托盘
    let icon_a = 1.0 - t;
    if icon_a > 0.02 {
        painter.rect_filled(rect.shrink(1.0), r - 1.0, with_alpha(p.accent_soft, icon_a));
        let s = 1.0 - 0.3 * t;
        let st = Stroke::new(1.7_f32, with_alpha(p.gold, icon_a));
        let (cx, cy) = (c.x, c.y - 0.5);
        painter.line_segment([Pos2::new(cx, cy - 6.0 * s), Pos2::new(cx, cy + 2.0 * s)], st);
        painter.line_segment(
            [Pos2::new(cx - 4.3 * s, cy - 0.4 * s), Pos2::new(cx, cy + 2.0 * s)],
            st,
        );
        painter.line_segment(
            [Pos2::new(cx + 4.3 * s, cy - 0.4 * s), Pos2::new(cx, cy + 2.0 * s)],
            st,
        );
        painter.line_segment(
            [Pos2::new(cx - 6.5 * s, cy + 6.0 * s), Pos2::new(cx + 6.5 * s, cy + 6.0 * s)],
            st,
        );
    }

    // 进度态：轨道圆环 + 进度弧
    if t > 0.02 {
        let rr = (r - 2.5) * (0.7 + 0.3 * t);
        painter.circle_stroke(c, rr, Stroke::new(2.6_f32, with_alpha(p.track, t)));
        let sweep = frac.unwrap_or_else(|| {
            let time = ui.input(|i| i.time) as f32;
            (time * 1.5).fract() * 0.25 + 0.05
        });
        let start = -std::f32::consts::FRAC_PI_2;
        let n = 48usize;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = start + sweep * std::f32::consts::TAU * (i as f32 / n as f32);
                Pos2::new(c.x + rr * a.cos(), c.y + rr * a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(
            pts,
            Stroke::new(2.6_f32, with_alpha(p.gold, t)),
        ));
    }
    if busy {
        ui.ctx().request_repaint();
    }
    resp.clicked()
}

/// 胶囊按钮（次级操作；材质全部来自主题）。
fn pill(ui: &mut egui::Ui, p: &Palette, text: &str, primary: bool) -> bool {
    let (fill, stroke, col) = if primary {
        (
            with_alpha(p.accent, 0.14),
            Stroke::new(1.0_f32, with_alpha(p.accent, 0.42)),
            p.accent,
        )
    } else {
        (
            p.card_fill,
            Stroke::new(1.0_f32, p.card_stroke),
            p.dim,
        )
    };
    ui.add(
        egui::Button::new(RichText::new(text).size(T_SEG).color(col))
            .fill(fill)
            .stroke(stroke)
            .min_size(Vec2::new(0.0, H_CHIP)),
    )
    .clicked()
}

/// 卡片内通栏主按钮（与 `pill` 同一套材质）。
fn wide_button(ui: &mut egui::Ui, p: &Palette, text: &str) -> bool {
    ui.add_sized(
        [ui.available_width(), H_BTN],
        egui::Button::new(RichText::new(text).size(T_PRIMARY).color(p.bg))
            .fill(p.fg)
            .stroke(Stroke::NONE),
    )
    .clicked()
}

/// 在固定宽度内绘制自动换行文本（不截断），返回占用高度。
fn wrapped(ui: &mut egui::Ui, w: f32, text: &str, size: f32, color: Color32) -> f32 {
    let galley = ui
        .painter()
        .layout(text.to_owned(), FontId::proportional(size), color, w);
    let h = galley.size().y;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    h
}

/// 固定宽度列（用于短文本，保证左右对齐）。
fn text_col(ui: &mut egui::Ui, w: f32, h: f32, align: Align, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(w.max(1.0), h), Layout::top_down(align), add);
}

/// 取地点显示名（`compact` 时只显示第一段）。
fn display_name(name: &str, compact: bool) -> String {
    if name.is_empty() {
        "未命名".to_string()
    } else if compact {
        name.split(" · ").next().unwrap_or(name).to_string()
    } else {
        name.to_string()
    }
}

// ---------- 预报页 ----------

fn forecast(app: &mut DawnGlowApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let limit = ui.max_rect();
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(10.0);

            home_head(app, ui, ctx);

            if app.loading && app.bundle.days.is_empty() {
                skeleton_page(ui, ctx, &app.p);
            } else if app.bundle.days.is_empty() {
                empty_card(app, ui, ctx);
            } else {
                let p = app.p;
                let ra = app.reveal_at;
                let idx = app.day.min(app.bundle.days.len() - 1);
                app.day = idx;
                let day = app.bundle.days[idx].clone();
                let kind = app.kind;
                reveal_block(ui, &p, ra, 0, |ui| sky_card(ui, &p, &day, kind, idx));
                reveal_block(ui, &p, ra, 1, |ui| event_seg(app, ui));
                reveal_block(ui, &p, ra, 2, |ui| day_chips(app, ui));
                reveal_block(ui, &p, ra, 3, |ui| metric_grid(ui, &p, &day, kind));
                reveal_block(ui, &p, ra, 4, |ui| detail_toggle(app, ui));
                reveal_block(ui, &p, ra, 5, |ui| fine_print(app, ui));
                if app.show_factors {
                    reveal_block(ui, &p, ra, 6, |ui| week_list(app, ui));
                    reveal_block(ui, &p, ra, 7, |ui| cloud_card(app, ui, &day, kind));
                    reveal_block(ui, &p, ra, 8, |ui| factors_card(app, ui, &day, kind));
                }
                if app.show_attribution {
                    reveal_block(ui, &p, ra, 9, |ui| attribution(app, ui));
                }
            }
            ui.add_space(16.0);

            let used = ui.min_rect();
            if used.right() > limit.right() + 1.0 || used.left() < limit.left() - 1.0 {
                log::warn!(
                    "forecast 横向溢出: used=[{:.1},{:.1}] limit=[{:.1},{:.1}]",
                    used.left(),
                    used.right(),
                    limit.left(),
                    limit.right()
                );
            }
        });
}


/// 按透明度合成（保留 r,g,b，缩放 a）。
fn with_alpha(c: Color32, k: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * k.clamp(0.0, 1.0)) as u8)
}

/// 内容块浮现：从下 8px 上移 + 淡入，并让骨架同步淡出，错开 60ms。
fn reveal_block(
    ui: &mut egui::Ui,
    p: &Palette,
    reveal_at: Option<f64>,
    index: usize,
    add: impl FnOnce(&mut egui::Ui),
) {
    let now = ui.ctx().input(|i| i.time);
    let t = match reveal_at {
        Some(t0) => (((now - t0) as f32) - index as f32 * 0.06) / 0.28,
        None => 1.0,
    }
    .clamp(0.0, 1.0);
    let off = (1.0 - t) * 8.0;
    if off > 0.01 {
        ui.add_space(off);
    }
    let top = ui.cursor().min.y;
    ui.scope(|ui| {
        ui.multiply_opacity(t);
        add(ui);
    });
    let bottom = ui.cursor().min.y;
    if off > 0.01 {
        ui.add_space(-off);
    }
    if t < 0.999 {
        let rect = Rect::from_min_max(
            Pos2::new(ui.max_rect().left(), top),
            Pos2::new(ui.max_rect().right(), bottom.max(top + 1.0)),
        );
        ui.painter()
            .rect_filled(rect, p.card_radius, with_alpha(p.skel, 1.0 - t));
        ui.ctx().request_repaint();
    }
}

/// 骨架条：占位并记录矩形，稍后统一上色 + 扫光。
fn skel_bar(ui: &mut egui::Ui, rects: &mut Vec<(Rect, f32)>, w: f32, h: f32, round: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(w.max(4.0), h), Sense::hover());
    rects.push((r, round));
}

fn skel_row(ui: &mut egui::Ui, rects: &mut Vec<(Rect, f32)>, n: usize, h: f32, gap: f32, round: f32) {
    let w = ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).max(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for _ in 0..n {
            skel_bar(ui, rects, w, h, round);
        }
    });
}

/// 骨架屏：与真实首页同形（天空卡 / 三天胶囊 / 2×2 要素 / 主按钮），共用一道斜向扫光。
fn skeleton_page(ui: &mut egui::Ui, ctx: &egui::Context, p: &Palette) {
    let mut rects: Vec<(Rect, f32)> = Vec::new();
    let top = ui.cursor().min.y;
    let w = ui.available_width();

    ui.add_space(15.0);
    skel_bar(ui, &mut rects, w, 196.0, 22.0);
    ui.add_space(10.0);
    skel_row(ui, &mut rects, 3, 56.0, 8.0, 12.0);
    ui.add_space(10.0);
    skel_row(ui, &mut rects, 2, 74.0, 8.0, 14.0);
    ui.add_space(8.0);
    skel_row(ui, &mut rects, 2, 74.0, 8.0, 14.0);
    ui.add_space(14.0);
    skel_bar(ui, &mut rects, w, 44.0, 12.0);
    ui.add_space(9.0);
    skel_bar(ui, &mut rects, (w * 0.55).min(200.0), 10.0, 5.0);

    let bottom = ui.cursor().min.y;
    let full = Rect::from_min_max(
        Pos2::new(ui.max_rect().left(), top),
        Pos2::new(ui.max_rect().right(), bottom.max(top + 1.0)),
    );
    let phase = (ctx.input(|i| i.time) as f32 / 1.5) % 1.0;
    paint_skeleton(ui, p, &rects, phase, full);
    ctx.request_repaint();
}


/// 画骨架底色 + 一道贯穿全页的斜向扫光带。
fn paint_skeleton(ui: &egui::Ui, p: &Palette, rects: &[(Rect, f32)], phase: f32, full: Rect) {
    let painter = ui.painter();
    for &(r, round) in rects {
        painter.rect_filled(r, round, p.skel);
    }
    let band = (full.width() * 0.30).max(90.0);
    let cx = full.left() - band + phase * (full.width() + 2.0 * band);
    let slope = 0.45_f32;
    let hi = p.skel_hi;
    let clear = with_alpha(hi, 0.0);
    for &(r, _) in rects {
        let shift_lo = (r.top() - full.top()) * slope;
        let shift_hi = (r.bottom() - full.top()) * slope;
        if r.right() < cx - band * 0.5 + shift_lo.min(shift_hi)
            || r.left() > cx + band * 0.5 + shift_lo.max(shift_hi)
        {
            continue;
        }
        let clip = painter.with_clip_rect(r);
        let mut mesh = egui::Mesh::default();
        for &y in &[r.top(), r.bottom()] {
            let shift = (y - full.top()) * slope;
            let xs = [cx - band * 0.5 + shift, cx + shift, cx + band * 0.5 + shift];
            for (i, &x) in xs.iter().enumerate() {
                let c = if i == 1 { hi } else { clear };
                mesh.colored_vertex(Pos2::new(x, y), c);
            }
        }
        mesh.add_triangle(0, 1, 3);
        mesh.add_triangle(1, 4, 3);
        mesh.add_triangle(1, 2, 4);
        mesh.add_triangle(2, 5, 4);
        clip.add(egui::Shape::mesh(mesh));
    }
}

// ---------- 首页 ----------

/// 自 1970-01-01 起的天数（用于算星期与月份）。
fn epoch_day(date: &str) -> i64 {
    let mut it = date.split('-');
    let y: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1970);
    let m: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 「周一」…「周日」
fn weekday_cn(date: &str) -> &'static str {
    const W: [&str; 7] = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
    W[((epoch_day(date) + 4).rem_euclid(7)) as usize]
}

/// 页眉小字：`OCT 18 · 周日`
fn kicker_of(date: &str) -> String {
    const M: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let md = date.get(5..).unwrap_or("");
    let m: usize = md.get(0..2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let d = md.get(3..5).unwrap_or("01");
    format!("{} {} · {}", M[(m - 1).min(11)], d, weekday_cn(date))
}

/// `10月19日 · 周一`
fn cn_date(date: &str) -> String {
    let md = date.get(5..).unwrap_or("");
    let m: i64 = md.get(0..2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let d: i64 = md.get(3..5).and_then(|s| s.parse().ok()).unwrap_or(0);
    format!("{m}月{d}日 · {}", weekday_cn(date))
}

/// 事件标签：`明早朝霞` / `今晚晚霞`
fn event_label(idx: usize, kind: EventKind) -> String {
    let when = match idx {
        0 => "今",
        1 => "明",
        2 => "后",
        _ => "",
    };
    if when.is_empty() {
        kind.label().to_string()
    } else {
        format!("{when}{}{}", if kind == EventKind::Sunrise { "早" } else { "晚" }, kind.label())
    }
}

/// 一句话结论。
fn verdict(score: f32, idx: usize, kind: EventKind) -> String {
    let when = match idx {
        0 => "今天",
        1 => "明天",
        2 => "后天",
        _ => "这天",
    };
    let phrase = if score >= 0.75 {
        "极佳，值得提前踩点"
    } else if score >= 0.6 {
        "值得出工"
    } else if score >= 0.45 {
        "一般，可以碰碰运气"
    } else if score >= 0.3 {
        "偏弱，别抱太大期望"
    } else {
        "基本无望，安心休息"
    };
    format!("{when}的{}，{phrase}", kind.label())
}

/// 概率文案。
fn chance_text(sc: &crate::model::EventScore) -> String {
    match sc.ens_prob {
        Some(p) => format!("{:.0}%", p * 100.0),
        None => "—".to_string(),
    }
}

fn home_head(app: &mut DawnGlowApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let p = app.p;
    let w = ui.available_width();
    let pa0 = ui.painter().clone();
    let name = display_name(&app.location.name, app.compact_name);
    let kicker = kicker_of(&today_local());
    let sub = match app.bundle.days.get(app.day.min(app.bundle.days.len().saturating_sub(1))) {
        Some(d) => verdict(d.score_for(app.kind).score, app.day, app.kind),
        None => "正在获取预报……".to_string(),
    };
    let text_w = (w - 96.0).max(60.0);
    let g_name = pa0.layout(name, FontId::proportional(T_H1), p.fg, text_w);
    let g_sub = pa0.layout(sub, FontId::proportional(T_SUB), p.dim, text_w);
    let h = 2.0 + 13.0 + 5.0 + g_name.size().y + 5.0 + g_sub.size().y + 8.0;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let pa = ui.painter().with_clip_rect(rect);
    let mut y = rect.top() + 2.0;
    tracked_text(&pa, Pos2::new(rect.left() + 1.0, y), &kicker, T_KICKER, 1.3, p.fg3);
    y += 13.0 + 5.0;
    pa.galley(Pos2::new(rect.left(), y), g_name.clone(), p.fg);
    // 点城市名 → 地图选点
    let name_rect = Rect::from_min_size(Pos2::new(rect.left(), y), g_name.size());
    if ui
        .interact(name_rect, egui::Id::new("home_city"), Sense::click())
        .clicked()
    {
        app.screen = Screen::Map;
    }
    y += g_name.size().y + 5.0;
    pa.galley(Pos2::new(rect.left(), y), g_sub, p.dim);

    // 右上角：设置 / 刷新
    let sz = 38.0;
    let top = rect.top() + 2.0;
    let refresh = Rect::from_min_size(Pos2::new(rect.right() - sz, top), Vec2::splat(sz));
    let gear = Rect::from_min_size(
        Pos2::new(rect.right() - sz * 2.0 - 8.0, top),
        Vec2::splat(sz),
    );
    if icon_button(ui, &p, "gear", BtnIcon::Gear, gear, false) {
        app.screen = Screen::Settings;
    }
    if icon_button(ui, &p, "refresh", BtnIcon::Refresh, refresh, app.loading) {
        app.start_fetch(ctx);
    }
}

/// 天空卡片：渐变底 + 光晕 + 大分数 + 三项要素。
fn sky_card(ui: &mut egui::Ui, p: &Palette, day: &DayForecast, kind: EventKind, idx: usize) {
    let sc = day.score_for(kind);
    ui.add_space(15.0);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 196.0), Sense::hover());
    let pa = ui.painter().with_clip_rect(rect);
    linear_gradient(&pa, rect, &theme::sky_gradient(), 150.0);
    let mood = ((sc.score - 0.35) / 0.45).clamp(0.0, 1.0);
    radial_glow(
        &pa,
        Pos2::new(rect.right() - 83.0, rect.top() + 29.0),
        116.0,
        0.35 + 0.65 * mood,
    );
    round_mask(&pa, p, rect, 22.0);

    let white = Color32::WHITE;
    let w78 = Color32::from_rgba_unmultiplied(255, 255, 255, 199);
    let w60 = Color32::from_rgba_unmultiplied(255, 255, 255, 153);
    let w58 = Color32::from_rgba_unmultiplied(255, 255, 255, 148);
    let w90 = Color32::from_rgba_unmultiplied(255, 255, 255, 230);
    let w20 = Color32::from_rgba_unmultiplied(255, 255, 255, 51);

    let inner_l = rect.left() + 18.0;
    let inner_r = rect.right() - 18.0;
    let mut y = rect.top() + 17.0;
    pa.text(
        Pos2::new(inner_l, y),
        Align2::LEFT_TOP,
        event_label(idx, kind),
        FontId::proportional(10.5),
        w78,
    );
    pa.text(
        Pos2::new(inner_r, y + 1.0),
        Align2::RIGHT_TOP,
        cn_date(&day.date),
        FontId::proportional(9.5),
        w60,
    );
    y += 20.0;

    let g_score = pa.layout_no_wrap(
        format!("{:.0}", (sc.score * 100.0).round()),
        FontId::proportional(T_SCORE),
        white,
    );
    let sw = g_score.size().x;
    pa.galley(Pos2::new(inner_l, y), g_score.clone(), white);
    pa.text(
        Pos2::new(inner_l + sw + 6.0, y + T_SCORE * 0.92),
        Align2::LEFT_BOTTOM,
        "/ 100",
        FontId::proportional(11.0),
        w58,
    );
    y += g_score.size().y + 2.0;
    pa.text(
        Pos2::new(inner_l, y),
        Align2::LEFT_TOP,
        sc.grade(),
        FontId::proportional(T_QUALITY),
        w90,
    );

    // 底部三项要素
    let line_y = rect.bottom() - 58.0;
    pa.rect_filled(
        Rect::from_min_max(Pos2::new(inner_l, line_y), Pos2::new(inner_r, line_y + 1.0)),
        0.0,
        w20,
    );
    let time_label = match kind {
        EventKind::Sunrise => "日出",
        EventKind::Sunset => "日落",
    };
    let time_val = day.time_for(kind).unwrap_or("—").to_string();
    let metas = [
        (time_label.to_string(), time_val),
        ("概率".to_string(), chance_text(sc)),
        ("云型".to_string(), sc.cloud_type.label().to_string()),
    ];
    let col_w = (inner_r - inner_l) / 3.0;
    for (i, (k, v)) in metas.iter().enumerate() {
        let x = inner_l + col_w * i as f32;
        pa.text(
            Pos2::new(x, line_y + 12.0),
            Align2::LEFT_TOP,
            k,
            FontId::proportional(9.0),
            w58,
        );
        pa.text(
            Pos2::new(x, line_y + 26.0),
            Align2::LEFT_TOP,
            v,
            FontId::proportional(12.5),
            white,
        );
    }
}

/// 朝霞 / 晚霞 切换。
fn event_seg(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    ui.add_space(10.0);
    let sel = if app.kind == EventKind::Sunrise { 0 } else { 1 };
    if let Some(i) = seg(ui, &p, "home_kind", &["朝霞", "晚霞"], sel) {
        app.kind = if i == 0 {
            EventKind::Sunrise
        } else {
            EventKind::Sunset
        };
    }
}

/// 三天胶囊（今天 / 明天 / 后天）。
fn day_chips(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    ui.add_space(10.0);
    let w = ui.available_width();
    let gap = 8.0;
    let cw = (w - gap * 2.0) / 3.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::hover());
    let mut pick = None;
    for i in 0..3 {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * (cw + gap), rect.top()),
            Vec2::new(cw, 56.0),
        );
        let hit = ui.interact(r, egui::Id::new(("daychip", i)), Sense::click());
        let on = app.day == i;
        let pa = ui.painter();
        let fill = if on {
            with_alpha(p.accent, 0.10)
        } else if hit.hovered() {
            p.card_press
        } else {
            p.card_fill
        };
        pa.rect_filled(r, 12.0, fill);
        pa.rect_stroke(
            r,
            12.0,
            Stroke::new(
                1.0_f32,
                if on {
                    with_alpha(p.accent, 0.40)
                } else {
                    p.card_stroke
                },
            ),
            egui::StrokeKind::Inside,
        );
        let label = match i {
            0 => "今天".to_string(),
            1 => "明天".to_string(),
            _ => "后天".to_string(),
        };
        pa.text(
            Pos2::new(r.left() + 11.0, r.top() + 9.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(9.5),
            if on { p.accent } else { p.fg3 },
        );
        let score = app
            .bundle
            .days
            .get(i)
            .map(|d| format!("{:.0}", (d.score_for(app.kind).score * 100.0).round()))
            .unwrap_or_else(|| "–".to_string());
        pa.text(
            Pos2::new(r.left() + 11.0, r.top() + 24.0),
            Align2::LEFT_TOP,
            score,
            FontId::proportional(T_DAY),
            if on { p.accent } else { p.fg },
        );
        if hit.clicked() {
            pick = Some(i);
        }
    }
    if let Some(i) = pick {
        app.day = i.min(app.bundle.days.len().saturating_sub(1));
    }
}

/// 2×2 要素格。
fn metric_grid(ui: &mut egui::Ui, p: &Palette, day: &DayForecast, kind: EventKind) {
    let sc = day.score_for(kind);
    let cloud = sc
        .cloud_total
        .map(|v| format!("{v:.0}%"))
        .unwrap_or_else(|| "—".into());
    let vis = sc
        .visibility_km
        .map(|v| format!("{v:.0} km"))
        .unwrap_or_else(|| "—".into());
    let hum = sc
        .humidity
        .map(|v| format!("{v:.0}%"))
        .unwrap_or_else(|| "—".into());
    let aod = sc
        .aod
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "—".into());
    let vis_note = match sc.visibility_km {
        Some(v) if v >= 25.0 => "通透",
        Some(v) if v >= 15.0 => "轻微霾",
        Some(_) => "能见度低",
        None => "—",
    };
    let hum_note = match sc.humidity {
        Some(h) if h >= 80.0 => "潮湿",
        Some(h) if h >= 40.0 => "舒适",
        Some(_) => "干燥",
        None => "—",
    };
    let aod_note = match sc.aod {
        Some(a) if a >= 0.4 => "偏浊",
        Some(a) if a >= 0.15 => "一般",
        Some(_) => "洁净",
        None => "—",
    };
    let items = [
        ("云量", cloud, sc.cloud_type.label().to_string()),
        ("能见度", vis, vis_note.to_string()),
        ("湿度", hum, hum_note.to_string()),
        ("气溶胶", aod, aod_note.to_string()),
    ];
    ui.add_space(10.0);
    let w = ui.available_width();
    let gap = 8.0;
    let cw = (w - gap) / 2.0;
    let ch = 74.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ch * 2.0 + gap), Sense::hover());
    for (i, (k, v, note)) in items.iter().enumerate() {
        let col = i % 2;
        let row = i / 2;
        let r = Rect::from_min_size(
            Pos2::new(
                rect.left() + col as f32 * (cw + gap),
                rect.top() + row as f32 * (ch + gap),
            ),
            Vec2::new(cw, ch),
        );
        let pa = ui.painter();
        pa.rect_filled(r, 14.0, p.card_fill);
        pa.rect_stroke(
            r,
            14.0,
            Stroke::new(1.0_f32, p.card_stroke),
            egui::StrokeKind::Inside,
        );
        pa.text(
            Pos2::new(r.left() + 12.0, r.top() + 10.0),
            Align2::LEFT_TOP,
            *k,
            FontId::proportional(9.5),
            p.fg3,
        );
        pa.text(
            Pos2::new(r.left() + 12.0, r.top() + 23.0),
            Align2::LEFT_TOP,
            v,
            FontId::proportional(T_METRIC),
            p.fg,
        );
        pa.text(
            Pos2::new(r.left() + 12.0, r.top() + 48.0),
            Align2::LEFT_TOP,
            note,
            FontId::proportional(T_METRIC_S),
            p.fg3,
        );
    }
}

/// 主按钮（白底黑字，按下微缩）。
fn primary_button(ui: &mut egui::Ui, p: &Palette, label: &str) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let k = if down { 0.98 } else { 1.0 };
    let r = Rect::from_center_size(rect.center(), rect.size() * k);
    let fill = if down {
        Color32::from_rgb(0xDC, 0xE1, 0xEF)
    } else if resp.hovered() {
        Color32::from_rgb(0xE8, 0xEB, 0xF4)
    } else {
        p.fg
    };
    ui.painter().rect_filled(r, 12.0, fill);
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(T_PRIMARY),
        p.bg,
    );
    resp.clicked()
}

/// 明细开关。
fn detail_toggle(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    ui.add_space(14.0);
    let label = if app.show_factors {
        "收起评分明细"
    } else {
        "查看评分明细"
    };
    if primary_button(ui, &p, label) {
        app.show_factors = !app.show_factors;
        app.save_config();
    }
}

/// 页脚小字。
fn fine_print(app: &DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    ui.add_space(9.0);
    let upd = match &app.last_updated {
        Some(t) => format!(" · {t} 更新"),
        None => String::new(),
    };
    let text = format!("数据来自 Open-Meteo · 8 模式集合{upd}");
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 13.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(T_FINE),
        p.fg3,
    );
}

/// 未来一周（明细里的紧凑列表，点选切换日期）。
fn week_list(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    group_label(ui, &p, "未来一周");
    let mut pick = None;
    group_card(ui, &p, |ui| {
        let n = app.bundle.days.len();
        for i in 0..n {
            if i > 0 {
                divider(ui, &p);
            }
            let d = &app.bundle.days[i];
            let title = format!("{} · {}", day_label(&d.date), weekday_cn(&d.date));
            let score = format!("{:.0}", (d.score_for(app.kind).score * 100.0).round());
            let on = app.day == i;
            let g = row_geom(ui, &p, 56.0, true);
            row_texts(ui, &p, &g, &title, None);
            let pa = ui.painter().with_clip_rect(g.rect);
            pa.text(
                Pos2::new(g.rect.right() - ROW_PAD, g.rect.center().y),
                Align2::RIGHT_CENTER,
                score,
                FontId::proportional(T_VALUE),
                if on { p.accent } else { p.fg },
            );
            if g.resp.clicked() {
                pick = Some(i);
            }
        }
    });
    if let Some(i) = pick {
        app.day = i;
    }
}

fn empty_card(app: &mut DawnGlowApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let p = app.p;
    let loading = app.loading;
    let status = app.status.clone();
    card(ui, &p, |ui| {
        let w = ui.available_width();
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("🌅").size(FS_EMOJI));
            ui.add_space(6.0);
        });
        wrapped(
            ui,
            w,
            if loading { "正在拉取预报数据…" } else { "暂无数据" },
            FS_ITEM,
            p.fg,
        );
        if !status.is_empty() {
            ui.add_space(4.0);
            wrapped(ui, w, &status, FS_CAPTION, p.warn);
        }
        if !loading {
            ui.add_space(SP_MD);
            if wide_button(ui, &p, "重试") {
                app.start_fetch(ctx);
            }
        }
        ui.add_space(4.0);
    });
}

fn cloud_card(app: &DawnGlowApp, ui: &mut egui::Ui, day: &DayForecast, kind: EventKind) {
    let p = app.p;
    let sc = day.score_for(kind);
    let ct = sc.cloud_type;
    let title = format!("云型 · {}", ct.label());
    let desc = ct.desc();
    card(ui, &p, |ui| {
        let w = ui.available_width();
        wrapped(ui, w, &title, FS_SECTION, p.fg);
        wrapped(ui, w, desc, FS_CAPTION, p.dim);
        ui.add_space(8.0);
        cloud_bar(ui, &p, "高云", sc.cloud_high, Color32::from_rgb(0x9C, 0xC6, 0xFF));
        cloud_bar(ui, &p, "中云", sc.cloud_mid, Color32::from_rgb(0x8F, 0xA7, 0xD8));
        cloud_bar(ui, &p, "低云", sc.cloud_low, Color32::from_rgb(0x6E, 0x7C, 0xA6));
        cloud_bar(ui, &p, "总云量", sc.cloud_total, p.accent);
    });
}

fn cloud_bar(ui: &mut egui::Ui, p: &Palette, label: &str, val: Option<f32>, color: Color32) {
    let frac = val.map(|v| (v / 100.0).clamp(0.0, 1.0)).unwrap_or(0.0);
    let val_txt = val.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "—".into());
    // 固定三列：标签 / 进度条 / 数值；列宽固定，保证各行严格对齐
    const LABEL_W: f32 = 46.0;
    const VALUE_W: f32 = 44.0;
    const GAP: f32 = 10.0;
    const BAR_H: f32 = 10.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        text_col(ui, LABEL_W, 18.0, Align::LEFT, |ui| {
            ui.label(RichText::new(label).size(FS_CAPTION).color(p.dim));
        });
        ui.add_space(GAP);
        let bar_w = (ui.available_width() - VALUE_W - GAP).max(30.0);
        let (resp, painter) = ui.allocate_painter(Vec2::new(bar_w, 18.0), Sense::hover());
        let rect = resp.rect;
        let track = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - BAR_H / 2.0),
            Vec2::new(rect.width(), BAR_H),
        );
        painter.rect_filled(track, BAR_H / 2.0, p.track);
        if frac > 0.0 {
            painter.rect_filled(
                Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())),
                BAR_H / 2.0,
                color,
            );
        }
        ui.add_space(GAP);
        ui.allocate_ui_with_layout(Vec2::new(VALUE_W, 18.0), Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(val_txt).size(FS_CAPTION).color(p.fg));
        });
    });
}

fn factors_card(app: &mut DawnGlowApp, ui: &mut egui::Ui, day: &DayForecast, kind: EventKind) {
    let p = app.p;
    let sc = day.score_for(kind).clone();
    let show = app.show_factors;
    card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 74.0).max(80.0);
            text_col(ui, w, 30.0, Align::LEFT, |ui| {
                ui.label(RichText::new("评分因子").size(FS_SECTION).strong().color(p.fg));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, &p, if show { "^" } else { "v" }, false) {
                    app.show_factors = !show;
                }
            });
        });
        if show {
            ui.add_space(6.0);
            for f in &sc.factors {
                ui.horizontal(|ui| {
                    text_col(ui, 76.0, 18.0, Align::LEFT, |ui| {
                        ui.label(RichText::new(f.label.as_str()).size(FS_CAPTION).color(p.dim));
                    });
                    let delta_w = 46.0;
                    let val_w = (ui.available_width() - delta_w - 8.0).max(40.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(val_w);
                        wrapped(ui, val_w, f.value.as_str(), FS_CAPTION, p.fg);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (sign, col) = if f.delta >= 0.0 {
                            ("+", p.gold)
                        } else {
                            ("", p.warn)
                        };
                        ui.label(RichText::new(format!("{sign}{:.2}", f.delta)).size(FS_CAPTION).color(col));
                    });
                });
            }
        }
    });
}

fn attribution(app: &DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: i8c(p.card_pad),
            right: i8c(p.card_pad),
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            let w = ui.available_width();
            let src = match &app.last_updated {
                Some(t) => format!("{} · 更新 {t}", app.model_mode.name()),
                None => app.model_mode.name().to_string(),
            };
            wrapped(ui, w, &src, FS_MICRO, p.gold);
            wrapped(
                ui,
                w,
                "数据：Open-Meteo（NCEP GFS / ECMWF IFS / Copernicus CAMS，CC BY 4.0）",
                FS_MICRO,
                p.dim,
            );
            wrapped(
                ui,
                w,
                "地图 © OpenStreetMap contributors / © CARTO；预报为概率性参考",
                FS_MICRO,
                p.dim,
            );
        });
}

// ---------- 地图页 ----------

fn map(app: &mut DawnGlowApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let p = app.p;
    ui.spacing_mut().item_spacing.y = p.gap;
    ui.add_space(4.0);

    if header(ui, &p, "地图选点", None, "← 返回") {
        app.screen = Screen::Forecast;
    }

    // 搜索
    card(ui, &p, |ui| {
        let (resp, submit) =
            search_field(ui, &p, "map", "搜索地名，如 上海", &mut app.search_query, true);
        let go = submit || (resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)));
        if go {
            app.start_search(ctx);
        }
        if !app.search_results.is_empty() {
            let mut chosen: Option<crate::model::Location> = None;
            let w = ui.available_width();
            for loc in &app.search_results {
                let galley = ui.painter().layout(
                    loc.name.clone(),
                    FontId::proportional(FS_BODY),
                    p.fg,
                    w,
                );
                let h = galley.size().y.max(26.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                ui.painter().galley(rect.min, galley, p.fg);
                if resp.clicked() {
                    chosen = Some(loc.clone());
                }
            }
            if let Some(loc) = chosen {
                app.search_results.clear();
                app.search_query.clear();
                app.set_location(loc, ctx);
                app.screen = Screen::Forecast;
                return;
            }
        }
    });

    // 地图区域
    let bottom = 66.0;
    let size = Vec2::new(ui.available_width(), (ui.available_height() - bottom).max(120.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, p.card_radius, p.surface);

    // 双指捏合缩放 / 双指平移（以触点为锚点）
    let multi = ctx.input(|i| i.multi_touch());
    let multi_active = multi
        .as_ref()
        .map_or(false, |m| m.num_touches >= 2 && rect.contains(m.center_pos));
    if multi_active {
        if let Some(m) = &multi {
            app.map.dragging = true;
            let dz = m.zoom_delta.max(1e-4).log2();
            if dz.abs() > 1e-4 {
                let dx = (m.center_pos.x - rect.center().x) as f64;
                let dy = (m.center_pos.y - rect.center().y) as f64;
                app.map.zoom_at(dz, dx, dy);
            }
            if m.translation_delta.length_sq() > 0.25 {
                app.map.pan_by(m.translation_delta.x as f64, m.translation_delta.y as f64);
            }
        }
    }

    // —— 瓦片绘制（分数缩放）——
    let source = app.map.source;
    let zoom = app.map.zoom;
    let zi = zoom.floor().max(0.0) as u32;
    let f = (zoom - zi as f32).max(0.0);
    let scale = TILE as f32 * 2f32.powf(f);
    let scale64 = scale as f64;
    let (fx, fy) = geo::lonlat_to_tile(app.map.center_lon, app.map.center_lat, zoom as f64);
    let cw = (fx * TILE, fy * TILE);
    let half_w = (rect.width() / 2.0) as f64;
    let half_h = (rect.height() / 2.0) as f64;
    let clip = painter.with_clip_rect(rect);
    let n = 1i64 << zi;
    let min_tx = ((cw.0 - half_w) / scale64).floor() as i64;
    let max_tx = ((cw.0 + half_w) / scale64).floor() as i64;
    let min_ty = ((cw.1 - half_h) / scale64).floor() as i64;
    let max_ty = ((cw.1 + half_h) / scale64).floor() as i64;

    // 静止时预取外圈瓦片，拖动中只补可见区域，并限制每帧新增请求数。
    let idle = !resp.dragged() && app.map.vel.length_sq() <= 1.0;
    let mut budget = 8i32;
    for tx in (min_tx - 1)..=(max_tx + 1) {
        for ty in (min_ty - 1)..=(max_ty + 1) {
            if ty < 0 || ty >= n {
                continue;
            }
            let inside = tx >= min_tx && tx <= max_tx && ty >= min_ty && ty <= max_ty;
            let wx = tx.rem_euclid(n);
            let key = (source.index(), zi, wx as u32, ty as u32);
            let sx = rect.center().x as f64 + (tx as f64 * scale64 - cw.0);
            let sy = rect.center().y as f64 + (ty as f64 * scale64 - cw.1);
            let trect = Rect::from_min_size(Pos2::new(sx as f32, sy as f32), Vec2::splat(scale));
            match app.map.texture(&key).map(|t| t.id()) {
                Some(id) => {
                    if inside {
                        clip.image(
                            id,
                            trect,
                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
                None => {
                    if inside {
                        clip.rect_filled(trect, 0.0, p.track);
                    }
                    if budget > 0 && (inside || idle) {
                        app.map.request(key, ctx);
                        budget -= 1;
                    }
                }
            }
        }
    }

    // 拖动：平移 + 记录速度（用于松手后的惯性）；双指手势时不重复平移
    app.map.dragging = resp.dragged() || multi_active;
    if resp.dragged() && !multi_active {
        let d = resp.drag_delta();
        app.map.pan_by(d.x as f64, d.y as f64);
        let dt = ctx.input(|i| i.stable_dt).max(0.008);
        let mut v = d / dt;
        let len = v.length();
        if len > 4000.0 {
            v *= 4000.0 / len;
        }
        app.map.vel = v;
    }

    // 选点取屏幕中心
    app.map.pin_lat = app.map.center_lat;
    app.map.pin_lon = app.map.center_lon;

    // 中心准星
    let c = rect.center();
    let line = Stroke::new(2.0_f32, p.fg);
    clip.line_segment(
        [Pos2::new(c.x, c.y - 56.0), Pos2::new(c.x, c.y - 13.0)],
        Stroke::new(2.0_f32, p.dusk),
    );
    clip.circle_filled(Pos2::new(c.x, c.y - 9.0), 6.5, p.dusk);
    clip.circle_stroke(Pos2::new(c.x, c.y - 9.0), 6.5, line);
    clip.line_segment([Pos2::new(c.x, c.y - 2.5), Pos2::new(c.x, c.y + 9.0)], line);

    // 缩放 / 回正
    let btn = 46.0;
    let bx = rect.right() - btn - 10.0;
    let by = rect.top() + 10.0;
    let plus = Rect::from_min_size(Pos2::new(bx, by), Vec2::splat(btn));
    let minus = Rect::from_min_size(Pos2::new(bx, by + btn + 8.0), Vec2::splat(btn));
    let recenter = Rect::from_min_size(Pos2::new(bx, by + (btn + 8.0) * 2.0), Vec2::splat(btn));
    for (r, label, delta) in [(plus, "+", 1.0_f32), (minus, "−", -1.0_f32)] {
        clip.rect_filled(r, p.card_radius, p.track);
        clip.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(24.0), p.fg);
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if r.contains(pos) {
                    app.map.zoom_by(delta);
                }
            }
        }
    }
    clip.rect_filled(recenter, p.card_radius, p.track);
    clip.text(recenter.center(), Align2::CENTER_CENTER, "◎", FontId::proportional(22.0), p.fg);
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if recenter.contains(pos) {
                app.map.center_lat = app.location.lat;
                app.map.center_lon = app.location.lon;
            }
        }
    }
    if resp.double_clicked() {
        app.map.zoom_by(1.0);
    }

    // 版权标注（图源要求）
    clip.text(
        Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0),
        Align2::RIGHT_BOTTOM,
        source.attribution(),
        FontId::proportional(FS_MICRO),
        p.dim,
    );

    // 底部操作
    card(ui, &p, |ui| {
        let full = ui.available_width();
        wrapped(
            ui,
            full,
            "拖动地图，红点处即选点；右上角可缩放 / 回到当前位置",
            FS_CAPTION,
            p.dim,
        );
        ui.add_space(SP_XS);
        ui.horizontal(|ui| {
            let (lat, lon) = (app.map.pin_lat, app.map.pin_lon);
            let coord = format!("{lat:.3}, {lon:.3}");
            let w = (ui.available_width() - 104.0).max(80.0);
            ui.vertical(|ui| {
                ui.set_max_width(w);
                wrapped(ui, w, &coord, FS_CAPTION, p.fg);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, &p, "使用此点", true) {
                    app.use_map_point(lat, lon, ctx);
                    app.screen = Screen::Forecast;
                }
            });
        });
    });
}

// ---------- 设置页 ----------

// ---------- 自研组件：分组设置列表 ----------
//
// 结构：分组标签 + 零内边距卡片 + 行（图标方块 / 标题 / 副标题 / 右侧控件）+ 行分隔线。
// 整行可点（`Sense::click`）并带悬停/按下反馈；右侧控件全用绘制而非嵌套 Button，
// 避免与全局 Button 样式互相打架，也便于统一动效。

// ---------- 设计组件（对齐设计稿）----------

/// 标准行高
const H_ROW: f32 = 52.0;
/// 行左右内边距
const ROW_PAD: f32 = 14.0;
/// 分段控件高度
const H_SEG: f32 = 30.0;

/// 带字距的文本（egui 没有 letter-spacing，逐字排版模拟），返回总宽。
fn tracked_text(
    pa: &egui::Painter,
    pos: Pos2,
    text: &str,
    size: f32,
    spacing: f32,
    color: Color32,
) -> f32 {
    let font = FontId::proportional(size);
    let mut x = pos.x;
    for ch in text.chars() {
        let g = pa.layout_no_wrap(ch.to_string(), font.clone(), color);
        let w = g.size().x;
        pa.galley(Pos2::new(x, pos.y), g, color);
        x += w + spacing;
    }
    (x - pos.x - spacing).max(0.0)
}

/// 分组标题（卡片上方的小字）。
fn group_label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
    let pa = ui.painter().with_clip_rect(rect);
    tracked_text(
        &pa,
        Pos2::new(rect.left() + 4.0, rect.top() + 1.0),
        text,
        T_GROUP,
        1.1,
        p.fg3,
    );
    ui.add_space(8.0);
}

/// 行列表卡片：零内边距，行自带左右内边距，行间用 `divider` 分隔。
fn group_card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut frame = egui::Frame::NONE
        .fill(p.card_fill)
        .corner_radius(p.card_radius);
    if p.card_stroke != Color32::TRANSPARENT {
        frame = frame.stroke(Stroke::new(1.0_f32, p.card_stroke));
    }
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// 行分隔线（通栏）。
fn divider(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.card_stroke);
}

struct RowGeom {
    rect: Rect,
    resp: Response,
    /// 右侧控件区域（垂直居中，30 高）
    trail: Rect,
    /// 左侧文字可用宽度
    text_w: f32,
}

fn row_geom(ui: &mut egui::Ui, p: &Palette, trail_w: f32, clickable: bool) -> RowGeom {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(w, H_ROW),
        if clickable { Sense::click() } else { Sense::hover() },
    );
    if clickable {
        if resp.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 0.0, p.card_press);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 0.0, with_alpha(p.card_press, 0.55));
        }
    }
    let trail = Rect::from_min_size(
        Pos2::new(rect.right() - ROW_PAD - trail_w, rect.center().y - H_SEG / 2.0),
        Vec2::new(trail_w.max(1.0), H_SEG),
    );
    let text_w = (trail.left() - 12.0 - (rect.left() + ROW_PAD)).max(40.0);
    RowGeom {
        rect,
        resp,
        trail,
        text_w,
    }
}

/// 行标题（可选副标题），整体垂直居中。
fn row_texts(ui: &egui::Ui, p: &Palette, g: &RowGeom, title: &str, sub: Option<&str>) {
    let pa = ui.painter().with_clip_rect(g.rect);
    let x = g.rect.left() + ROW_PAD;
    let gt = pa.layout(title.to_string(), FontId::proportional(T_ROW), p.fg, g.text_w);
    let h1 = gt.size().y;
    let gs = sub
        .filter(|s| !s.is_empty())
        .map(|s| pa.layout(s.to_string(), FontId::proportional(T_ROW_S), p.fg3, g.text_w));
    let h2 = gs.as_ref().map(|g| g.size().y + 2.0).unwrap_or(0.0);
    let mut y = g.rect.center().y - (h1 + h2) / 2.0;
    pa.galley(Pos2::new(x, y), gt, p.fg);
    if let Some(gs) = gs {
        y += h1 + 2.0;
        pa.galley(Pos2::new(x, y), gs, p.fg3);
    }
}

/// 右侧箭头 ›
fn chevron(ui: &egui::Ui, c: Pos2, col: Color32) {
    let st = Stroke::new(1.8_f32, col);
    ui.painter()
        .line_segment([Pos2::new(c.x - 2.6, c.y - 5.0), Pos2::new(c.x + 2.8, c.y)], st);
    ui.painter()
        .line_segment([Pos2::new(c.x + 2.8, c.y), Pos2::new(c.x - 2.6, c.y + 5.0)], st);
}

/// 静态行：标题 + 右侧数值，不可点。
fn static_row(ui: &mut egui::Ui, p: &Palette, title: &str, value: &str) {
    let vw = ui
        .painter()
        .layout_no_wrap(value.to_string(), FontId::proportional(T_VALUE), p.fg3)
        .size()
        .x;
    let g = row_geom(ui, p, vw.min(180.0), false);
    row_texts(ui, p, &g, title, None);
    let pa = ui.painter().with_clip_rect(g.rect);
    let gal = pa.layout(value.to_string(), FontId::proportional(T_VALUE), p.fg3, 180.0);
    let w = gal.size().x;
    pa.galley(
        Pos2::new(g.rect.right() - ROW_PAD - w, g.rect.center().y - gal.size().y / 2.0),
        gal,
        p.fg3,
    );
}

/// 跳转行：标题 + 数值 + ›。
fn nav_row(ui: &mut egui::Ui, p: &Palette, title: &str, value: &str) -> bool {
    let vw = ui
        .painter()
        .layout_no_wrap(value.to_string(), FontId::proportional(T_VALUE), p.fg3)
        .size()
        .x;
    let g = row_geom(ui, p, vw.min(170.0) + 21.0, true);
    row_texts(ui, p, &g, title, None);
    let pa = ui.painter().with_clip_rect(g.rect);
    let gal = pa.layout(value.to_string(), FontId::proportional(T_VALUE), p.fg3, 170.0);
    let w = gal.size().x;
    pa.galley(
        Pos2::new(
            g.rect.right() - ROW_PAD - 21.0 - w,
            g.rect.center().y - gal.size().y / 2.0,
        ),
        gal,
        p.fg3,
    );
    chevron(
        ui,
        Pos2::new(g.rect.right() - ROW_PAD - 6.5, g.rect.center().y),
        p.fg3,
    );
    g.resp.clicked()
}

/// 单选行：右侧勾选圈。
fn check_row(ui: &mut egui::Ui, p: &Palette, title: &str, sub: Option<&str>, on: bool) -> bool {
    let g = row_geom(ui, p, 22.0, true);
    row_texts(ui, p, &g, title, sub);
    let c = Pos2::new(g.trail.right() - 11.0, g.rect.center().y);
    let pa = ui.painter();
    if on {
        pa.circle_filled(c, 10.5, p.accent);
        let st = Stroke::new(1.8_f32, Color32::WHITE);
        pa.line_segment(
            [Pos2::new(c.x - 4.0, c.y + 0.2), Pos2::new(c.x - 1.2, c.y + 3.0)],
            st,
        );
        pa.line_segment(
            [Pos2::new(c.x - 1.2, c.y + 3.0), Pos2::new(c.x + 4.2, c.y - 3.2)],
            st,
        );
    } else {
        pa.circle_stroke(c, 9.5, Stroke::new(1.4_f32, p.fg3));
    }
    g.resp.clicked()
}

/// 开关行（整行可点）。
fn switch_row(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    title: &str,
    sub: Option<&str>,
    on: bool,
) -> bool {
    let g = row_geom(ui, p, 44.0, true);
    row_texts(ui, p, &g, title, sub);
    let rect = Rect::from_center_size(
        Pos2::new(g.trail.right() - 22.0, g.rect.center().y),
        Vec2::new(44.0, 26.0),
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(egui::Id::new(("switch", salt)), on, 0.2);
    paint_switch(&ui.painter().with_clip_rect(g.rect), p, t, rect);
    g.resp.clicked()
}

/// 分段控件的自然宽度。
fn seg_width(ui: &egui::Ui, items: &[&str]) -> f32 {
    let font = FontId::proportional(T_SEG);
    let mut w = 6.0;
    for s in items {
        w += ui
            .painter()
            .layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
            + 18.0;
    }
    w
}

/// 在给定矩形内绘制分段控件，返回被点中的项。
fn seg_at(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    items: &[&str],
    sel: usize,
    rect: Rect,
) -> Option<usize> {
    let font = FontId::proportional(T_SEG);
    let pa = ui.painter().with_clip_rect(rect.expand(2.0));
    let widths: Vec<f32> = items
        .iter()
        .map(|s| {
            pa.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
                + 18.0
        })
        .collect();
    let mut xs = Vec::with_capacity(items.len() + 1);
    let mut x = rect.left() + 3.0;
    xs.push(x);
    for w in &widths {
        x += w;
        xs.push(x);
    }
    pa.rect_filled(rect, 9.0, p.chip_fill);
    // 滑块（按选中项滑动）
    let n = items.len().saturating_sub(1);
    let t = ui
        .ctx()
        .animate_value_with_time(egui::Id::new(("seg", salt)), sel.min(n) as f32, 0.3);
    let i0 = (t.floor() as usize).min(n);
    let i1 = (i0 + 1).min(n);
    let k = (t - i0 as f32).clamp(0.0, 1.0);
    let left = xs[i0] + (xs[i1] - xs[i0]) * k;
    let wid = widths[i0] + (widths[i1] - widths[i0]) * k;
    pa.rect_filled(
        Rect::from_min_size(
            Pos2::new(left, rect.top() + 3.0),
            Vec2::new(wid, H_SEG - 6.0),
        ),
        7.0,
        p.chip_stroke,
    );
    // 文字 + 命中
    let mut picked = None;
    let mut cx = rect.left() + 3.0;
    for (i, s) in items.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(cx, rect.top()),
            Vec2::new(widths[i], rect.height()),
        );
        let hit = ui.interact(r, egui::Id::new(("segi", salt, i)), Sense::click());
        if hit.clicked() {
            picked = Some(i);
        }
        let col = if i == sel {
            p.fg
        } else if hit.hovered() {
            p.dim
        } else {
            p.fg3
        };
        pa.text(r.center(), Align2::CENTER_CENTER, *s, font.clone(), col);
        cx += widths[i];
    }
    picked
}

/// 独立分段控件（宽度随内容，左对齐）。
fn seg(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    items: &[&str],
    sel: usize,
) -> Option<usize> {
    let w = seg_width(ui, items);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, H_SEG), Sense::hover());
    seg_at(ui, p, salt, items, sel, rect)
}

/// 行内分段（右侧）。
fn seg_row(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    title: &str,
    items: &[&str],
    sel: usize,
) -> Option<usize> {
    let sw = seg_width(ui, items);
    let g = row_geom(ui, p, sw, false);
    row_texts(ui, p, &g, title, None);
    let rect = Rect::from_center_size(g.trail.center(), Vec2::new(sw, H_SEG));
    seg_at(ui, p, salt, items, sel, rect)
}

/// 开关的绘制（44×26，滑块随 `t` 移动）。
fn paint_switch(pa: &egui::Painter, p: &Palette, t: f32, rect: Rect) {
    let r = rect.height() / 2.0;
    pa.rect_filled(rect, r, lerp_color(p.track, p.accent, t));
    let knob_r = r - 2.0;
    let x = rect.left() + r + (rect.width() - 2.0 * r) * t;
    pa.circle_filled(Pos2::new(x, rect.center().y), knob_r, Color32::WHITE);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BtnIcon {
    Gear,
    Refresh,
}

/// 图标按钮（38×38，圆角 12）。
fn icon_button(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    kind: BtnIcon,
    rect: Rect,
    busy: bool,
) -> bool {
    let hit = ui.interact(rect, egui::Id::new(("iconbtn", salt)), Sense::click());
    let pa = ui.painter();
    let fill = if hit.is_pointer_button_down_on() {
        p.card_press
    } else if hit.hovered() {
        with_alpha(p.chip_fill, 1.4)
    } else {
        p.card_fill
    };
    pa.rect_filled(rect, 12.0, fill);
    pa.rect_stroke(
        rect,
        12.0,
        Stroke::new(1.0_f32, p.card_stroke),
        egui::StrokeKind::Inside,
    );
    let col = if busy {
        p.accent
    } else if hit.hovered() {
        p.fg
    } else {
        p.dim
    };
    let c = rect.center();
    match kind {
        BtnIcon::Gear => {
            let st = Stroke::new(1.6_f32, col);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, k) = (a.sin(), -a.cos());
                pa.line_segment(
                    [
                        Pos2::new(c.x + s * 5.4, c.y + k * 5.4),
                        Pos2::new(c.x + s * 8.2, c.y + k * 8.2),
                    ],
                    st,
                );
            }
            pa.circle_stroke(c, 5.4, st);
            pa.circle_filled(c, 2.0, col);
        }
        BtnIcon::Refresh => {
            let rot = if busy {
                (ui.ctx().input(|i| i.time) as f32 * 5.5) % std::f32::consts::TAU
            } else {
                0.0
            };
            let st = Stroke::new(1.8_f32, col);
            let r = 7.0;
            let a0 = -0.55 + rot;
            let a1 = a0 + 5.1;
            let steps = 26;
            let mut prev: Option<Pos2> = None;
            for i in 0..=steps {
                let a = a0 + (a1 - a0) * i as f32 / steps as f32;
                let pt = Pos2::new(c.x + a.sin() * r, c.y - a.cos() * r);
                if let Some(pp) = prev {
                    pa.line_segment([pp, pt], st);
                }
                prev = Some(pt);
            }
            if let Some(end) = prev {
                let dir = Vec2::new(end.x - c.x, end.y - c.y).normalized();
                let n = Vec2::new(-dir.y, dir.x);
                pa.line_segment([end, end - dir * 4.2 + n * 3.4], st);
                pa.line_segment([end, end - dir * 4.2 - n * 3.4], st);
            }
            if busy {
                ui.ctx().request_repaint();
            }
        }
    }
    hit.clicked()
}

/// 输入框外观：圆角底 + 细描边 + 内边距，文字仍交给 egui TextEdit。
/// `lead` / `trail` 是左右内侧留给图标、按钮的宽度。
fn input_row(
    ui: &mut egui::Ui,
    p: &Palette,
    hint: &str,
    text: &mut String,
    lead: i8,
    trail: i8,
) -> Response {
    let w = ui.available_width();
    ui.scope(|ui| {
        let v = &mut ui.style_mut().visuals;
        v.extreme_bg_color = p.card_fill;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.card_stroke);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.chip_stroke);
        v.widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
        v.selection.bg_fill = p.accent_soft;
        ui.add_sized(
            [w, 40.0],
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .margin(egui::Margin {
                    left: lead,
                    right: trail,
                    top: 8,
                    bottom: 8,
                }),
        )
    })
    .inner
}

/// 搜索框：自绘放大镜 / 清空 / 提交按钮。返回 (编辑器响应, 是否点了搜索)。
fn search_field(
    ui: &mut egui::Ui,
    p: &Palette,
    salt: &str,
    hint: &str,
    text: &mut String,
    with_submit: bool,
) -> (Response, bool) {
    let right_pad = if with_submit { 64 } else { 36 };
    let resp = input_row(ui, p, hint, text, 36, right_pad);
    let r = resp.rect;
    let pa = ui.painter().with_clip_rect(r);
    let icon_col = if resp.has_focus() { p.accent } else { p.fg3 };
    let c = Pos2::new(r.left() + 20.0, r.center().y);
    pa.circle_stroke(
        Pos2::new(c.x - 1.0, c.y - 1.0),
        5.0,
        Stroke::new(1.5_f32, icon_col),
    );
    pa.line_segment(
        [Pos2::new(c.x + 2.8, c.y + 2.8), Pos2::new(c.x + 6.2, c.y + 6.2)],
        Stroke::new(1.5_f32, icon_col),
    );

    let mut submitted = false;
    if with_submit {
        let chip = Rect::from_center_size(
            Pos2::new(r.right() - 34.0, r.center().y),
            Vec2::new(52.0, 28.0),
        );
        let hit = ui.interact(chip, egui::Id::new(("searchgo", salt)), Sense::click());
        let down = hit.is_pointer_button_down_on();
        let fill = if down {
            with_alpha(p.accent, 0.4)
        } else {
            p.accent_soft
        };
        let col = if down { p.fg } else { p.accent };
        pa.rect_filled(chip, 14.0, fill);
        pa.text(
            chip.center(),
            Align2::CENTER_CENTER,
            "搜索",
            FontId::proportional(T_SEG),
            col,
        );
        submitted = hit.clicked();
    } else if !text.is_empty() {
        let cx = Pos2::new(r.right() - 20.0, r.center().y);
        let cr = Rect::from_center_size(cx, Vec2::splat(22.0));
        let hit = ui.interact(cr, egui::Id::new(("searchclear", salt)), Sense::click());
        let col = if hit.hovered() { p.fg } else { p.fg3 };
        let st = Stroke::new(1.5_f32, col);
        pa.line_segment(
            [cx + Vec2::new(-4.0, -4.0), cx + Vec2::new(4.0, 4.0)],
            st,
        );
        pa.line_segment(
            [cx + Vec2::new(-4.0, 4.0), cx + Vec2::new(4.0, -4.0)],
            st,
        );
        if hit.clicked() {
            text.clear();
        }
    }
    (resp, submitted)
}

/// 字体页的预设行（名称 + 说明 + 选中圈）。
fn option_row(ui: &mut egui::Ui, p: &Palette, name: &str, desc: &str, on: bool) -> bool {
    check_row(ui, p, name, Some(desc), on)
}

/// 线性渐变网格（任意角度），圆角用背景色三角补齐。
fn linear_gradient(
    pa: &egui::Painter,
    rect: Rect,
    stops: &[(f32, Color32)],
    deg: f32,
) {
    let rad = deg.to_radians();
    let dir = Vec2::new(rad.sin(), -rad.cos());
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for c in [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ] {
        let t = (c - rect.min).dot(dir);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let span = (hi - lo).max(0.001);
    let n = 6u32;
    let mut mesh = egui::Mesh::default();
    for iy in 0..=n {
        for ix in 0..=n {
            let pos = Pos2::new(
                rect.left() + rect.width() * ix as f32 / n as f32,
                rect.top() + rect.height() * iy as f32 / n as f32,
            );
            let t = ((pos - rect.min).dot(dir) - lo) / span;
            mesh.colored_vertex(pos, lerp_stops(stops, t));
        }
    }
    let stride = n + 1;
    for iy in 0..n {
        for ix in 0..n {
            let a = iy * stride + ix;
            mesh.add_triangle(a, a + 1, a + stride);
            mesh.add_triangle(a + 1, a + stride + 1, a + stride);
        }
    }
    pa.add(egui::Shape::mesh(mesh));
}

/// 圆角遮罩：四角用页面背景色三角补齐（网格渐变本身不带圆角）。
fn round_mask(pa: &egui::Painter, p: &Palette, rect: Rect, radius: f32) {
    if radius > 0.0 {
        let bg = p_at_bg(pa, p, rect.center().y);
        let steps = 6;
        let corners = [
            (rect.left() + radius, rect.top() + radius, std::f32::consts::PI,
             Pos2::new(rect.left(), rect.top())),
            (rect.right() - radius, rect.top() + radius, 1.5 * std::f32::consts::PI,
             Pos2::new(rect.right(), rect.top())),
            (rect.right() - radius, rect.bottom() - radius, 0.0,
             Pos2::new(rect.right(), rect.bottom())),
            (rect.left() + radius, rect.bottom() - radius, 0.5 * std::f32::consts::PI,
             Pos2::new(rect.left(), rect.bottom())),
        ];
        for (cx, cy, a0, corner) in corners {
            for i in 0..steps {
                let a1 = a0 + std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
                let a2 = a0 + std::f32::consts::FRAC_PI_2 * (i + 1) as f32 / steps as f32;
                let p1 = Pos2::new(cx + a1.cos() * radius, cy + a1.sin() * radius);
                let p2 = Pos2::new(cx + a2.cos() * radius, cy + a2.sin() * radius);
                pa.add(egui::Shape::convex_polygon(
                    vec![corner, p1, p2],
                    bg,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// 页面背景在某高度处的颜色（用于补齐渐变卡的圆角）。
fn p_at_bg(pa: &egui::Painter, p: &Palette, y: f32) -> Color32 {
    let full = pa.clip_rect();
    let t = ((y - full.top()) / full.height().max(1.0)).clamp(0.0, 1.0);
    lerp_stops(&theme::vgrad(p), t)
}

/// 光晕：由外向内叠加的低透明度圆，形成柔和径向渐变。
fn radial_glow(pa: &egui::Painter, c: Pos2, r: f32, strength: f32) {
    let steps = 22;
    let a = (255.0 * strength * 0.055) as u8;
    let col = Color32::from_rgba_unmultiplied(255, 219, 178, a);
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        pa.circle_filled(c, r * (1.0 - t), col);
    }
}

// ---------- 子页顶栏 ----------

/// 二级页顶栏：返回箭头 + 居中标题 + 底部细线。返回是否点了返回。
fn header(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    _subtitle: Option<&str>,
    _back: &str,
) -> bool {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::hover());
    let br = Rect::from_center_size(
        Pos2::new(rect.left() + 21.0, rect.center().y),
        Vec2::splat(34.0),
    );
    let hit = ui.interact(br, egui::Id::new(("back", title)), Sense::click());
    let pa = ui.painter().with_clip_rect(rect);
    if hit.hovered() {
        pa.circle_filled(br.center(), 17.0, p.card_fill);
    }
    let col = if hit.hovered() { p.fg } else { p.dim };
    let st = Stroke::new(1.9_f32, col);
    pa.line_segment(
        [
            Pos2::new(br.center().x + 3.5, br.center().y - 6.0),
            Pos2::new(br.center().x - 3.5, br.center().y),
        ],
        st,
    );
    pa.line_segment(
        [
            Pos2::new(br.center().x - 3.5, br.center().y),
            Pos2::new(br.center().x + 3.5, br.center().y + 6.0),
        ],
        st,
    );
    pa.text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(15.0),
        p.fg,
    );
    pa.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - 1.0),
            Pos2::new(rect.right(), rect.bottom()),
        ),
        0.0,
        p.card_stroke,
    );
    hit.clicked()
}

// ---------- 设置页 ----------

fn settings(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let p = app.p;
            ui.spacing_mut().item_spacing.y = 0.0;

            if header(ui, &p, "设置", None, "← 返回") {
                app.screen = Screen::Forecast;
            }

            group_label(ui, &p, "外观");
            group_card(ui, &p, |ui| {
                let cur = match &app.font_file {
                    None => "系统默认".to_string(),
                    Some(f) => display_name(f, true),
                };
                if nav_row(ui, &p, "字体", &cur) {
                    app.screen = Screen::Font;
                }
                divider(ui, &p);
                if let Some(i) = seg_row(
                    ui,
                    &p,
                    "compact",
                    "地点名称",
                    &["精简", "完整"],
                    if app.compact_name { 0 } else { 1 },
                ) {
                    app.compact_name = i == 0;
                    app.save_config();
                }
            });

            group_label(ui, &p, "数据");
            group_card(ui, &p, |ui| {
                let items = ["综合", "GFS", "ECMWF"];
                let sel = ModelMode::ALL
                    .iter()
                    .position(|m| *m == app.model_mode)
                    .unwrap_or(0);
                if let Some(i) = seg_row(ui, &p, "mode", "预报模式", &items, sel) {
                    app.model_mode = ModelMode::ALL[i];
                    app.save_config();
                    app.start_fetch(ui.ctx());
                }
                divider(ui, &p);
                static_row(ui, &p, "要素来源", "Open-Meteo · CAMS");
                divider(ui, &p);
                let upd = app.last_updated.clone().unwrap_or_else(|| "—".into());
                if nav_row(ui, &p, "立即刷新", &upd) {
                    app.start_fetch(ui.ctx());
                }
            });

            group_label(ui, &p, "地图");
            group_card(ui, &p, |ui| {
                let items = ["深色", "浅色", "高德", "OSM"];
                let sel = crate::map::MapSource::ALL
                    .iter()
                    .position(|m| *m == app.map.source)
                    .unwrap_or(0);
                if let Some(i) = seg_row(ui, &p, "mapsrc", "地图样式", &items, sel) {
                    app.map.source = crate::map::MapSource::ALL[i];
                    app.save_config();
                }
                divider(ui, &p);
                if nav_row(ui, &p, "地图选点", "") {
                    app.screen = Screen::Map;
                }
            });

            group_label(ui, &p, "显示");
            group_card(ui, &p, |ui| {
                if let Some(i) = seg_row(
                    ui,
                    &p,
                    "kind",
                    "事件",
                    &["朝霞", "晚霞"],
                    if app.kind == EventKind::Sunrise { 0 } else { 1 },
                ) {
                    app.kind = if i == 0 {
                        EventKind::Sunrise
                    } else {
                        EventKind::Sunset
                    };
                    app.default_sunrise = i == 0;
                    app.save_config();
                }
                divider(ui, &p);
                if switch_row(
                    ui,
                    &p,
                    "factors",
                    "评分因子默认展开",
                    Some("进入首页时直接显示明细"),
                    app.show_factors,
                ) {
                    app.show_factors = !app.show_factors;
                    app.save_config();
                }
                divider(ui, &p);
                if switch_row(
                    ui,
                    &p,
                    "attr",
                    "显示数据来源",
                    Some("页脚显示来源与地图版权"),
                    app.show_attribution,
                ) {
                    app.show_attribution = !app.show_attribution;
                    app.save_config();
                }
            });

            group_label(ui, &p, "关于");
            group_card(ui, &p, |ui| {
                if nav_row(ui, &p, "版本与更新日志", env!("CARGO_PKG_VERSION")) {
                    app.screen = Screen::Version;
                }
            });

            ui.add_space(24.0);
        });
}

// ---------- 版本页 ----------

/// 关于页。
fn version(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let p = app.p;
            let w = ui.available_width();
            ui.spacing_mut().item_spacing.y = 0.0;

            if header(ui, &p, "关于", None, "← 返回") {
                app.screen = Screen::Settings;
            }

            // 图标
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, 76.0),
                Sense::hover(),
            );
            let icon = Rect::from_center_size(row.center(), Vec2::splat(76.0));
            let pa = ui.painter();
            linear_gradient(&pa, icon, &theme::sky_gradient(), 150.0);
            sun_mark(&pa, icon.center(), 17.0, Color32::WHITE);
            round_mask(&pa, &p, icon, 20.0);

            // 名称
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, 30.0),
                Sense::hover(),
            );
            ui.painter().text(
                Pos2::new(row.center().x, row.center().y),
                Align2::CENTER_CENTER,
                "朝霞预报",
                FontId::proportional(22.0),
                p.fg,
            );

            // 英文名（带字距）
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, 14.0),
                Sense::hover(),
            );
            let tw = ui
                .painter()
                .layout_no_wrap("DAWNGLOW".into(), FontId::proportional(9.0), p.fg3)
                .size()
                .x;
            let spacing = 2.2;
            let total = tw + spacing * 8.0;
            tracked_text(
                &ui.painter(),
                Pos2::new(row.center().x - total / 2.0, row.top() + 2.0),
                "DAWNGLOW",
                9.0,
                spacing,
                p.fg3,
            );

            // 版本胶囊
            let vtxt = format!("版本 {}", env!("CARGO_PKG_VERSION"));
            let vw = ui
                .painter()
                .layout_no_wrap(vtxt.clone(), FontId::proportional(10.5), p.dim)
                .size()
                .x
                + 22.0;
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, 34.0),
                Sense::hover(),
            );
            let vr = Rect::from_center_size(
                Pos2::new(row.center().x, row.center().y + 3.0),
                Vec2::new(vw, 24.0),
            );
            let pa = ui.painter();
            pa.rect_filled(vr, 12.0, p.card_fill);
            pa.rect_stroke(
                vr,
                12.0,
                Stroke::new(1.0_f32, p.card_stroke),
                egui::StrokeKind::Inside,
            );
            pa.text(
                vr.center(),
                Align2::CENTER_CENTER,
                vtxt,
                FontId::proportional(10.5),
                p.dim,
            );

            // 简介
            let desc = "面向摄影与追霞的朝霞 / 晚霞预报：逐日给出最佳时间与 0–100 鲜艳度评分，并展示云量、能见度、湿度与气溶胶等要素明细。";
            let dw = (ui.available_width() - 44.0).min(268.0);
            ui.add_space(14.0);
            let gal = ui
                .painter()
                .layout(desc.to_string(), FontId::proportional(11.5), p.dim, dw);
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, gal.size().y),
                Sense::hover(),
            );
            ui.painter().galley(
                Pos2::new(row.center().x - gal.size().x / 2.0, row.top()),
                gal,
                p.dim,
            );

            // 检查更新
            ui.add_space(14.0);
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, 36.0),
                Sense::hover(),
            );
            let status = app.update_status.clone();
            let done = status.starts_with("已是最新");
            let bw = 132.0;
            let br = Rect::from_center_size(row.center(), Vec2::new(bw, 36.0));
            let hit = ui.interact(br, egui::Id::new("check_update"), Sense::click());
            let accent = p.accent;
            let fill = if hit.is_pointer_button_down_on() {
                with_alpha(p.accent, 0.22)
            } else {
                with_alpha(p.accent, 0.12)
            };
            let pa = ui.painter();
            pa.rect_filled(br, 18.0, fill);
            pa.rect_stroke(
                br,
                18.0,
                Stroke::new(1.0_f32, with_alpha(accent, 0.34)),
                egui::StrokeKind::Inside,
            );
            pa.text(
                br.center(),
                Align2::CENTER_CENTER,
                if done { "已是最新" } else { "检查更新" },
                FontId::proportional(11.5),
                accent,
            );
            if hit.clicked() {
                app.check_update(ui.ctx());
            }
            if !status.is_empty() {
                ui.add_space(8.0);
                let sw = ui.available_width();
                let gal = ui.painter().layout(
                    status.clone(),
                    FontId::proportional(10.5),
                    if status.starts_with("发现") { p.accent } else { p.fg3 },
                    sw - 40.0,
                );
                let (row, _) = ui.allocate_exact_size(
                    Vec2::new(sw, gal.size().y),
                    Sense::hover(),
                );
                ui.painter().galley(
                    Pos2::new(row.center().x - gal.size().x / 2.0, row.top()),
                    gal,
                    p.fg3,
                );
            }

            // 更新日志
            group_label(ui, &p, "更新日志");
            group_card(ui, &p, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(w, 58.0),
                    Sense::hover(),
                );
                let pa = ui.painter().with_clip_rect(rect);
                let ver = env!("CARGO_PKG_VERSION");
                let vg = pa.layout_no_wrap(
                    ver.to_string(),
                    FontId::proportional(9.5),
                    p.dim,
                );
                let vr = Rect::from_min_size(
                    Pos2::new(rect.left() + 14.0, rect.top() + 12.0),
                    Vec2::new(vg.size().x + 12.0, 18.0),
                );
                pa.rect_filled(vr, 5.0, p.card_stroke);
                pa.text(
                    vr.center(),
                    Align2::CENTER_CENTER,
                    ver,
                    FontId::proportional(9.5),
                    p.dim,
                );
                let cr = Rect::from_min_size(
                    Pos2::new(vr.right() + 7.0, vr.top()),
                    Vec2::new(56.0, 18.0),
                );
                pa.rect_filled(cr, 5.0, with_alpha(p.accent, 0.12));
                pa.rect_stroke(
                    cr,
                    5.0,
                    Stroke::new(1.0_f32, with_alpha(p.accent, 0.34)),
                    egui::StrokeKind::Inside,
                );
                pa.text(
                    cr.center(),
                    Align2::CENTER_CENTER,
                    "当前版本",
                    FontId::proportional(9.0),
                    p.accent,
                );
                pa.text(
                    Pos2::new(rect.right() - 14.0, rect.top() + 15.0),
                    Align2::RIGHT_TOP,
                    "2026.10.18",
                    FontId::proportional(9.5),
                    p.fg3,
                );
                pa.text(
                    Pos2::new(rect.left() + 14.0, rect.top() + 35.0),
                    Align2::LEFT_TOP,
                    "初版：天空卡片、三天评分、8 模式集合概率与要素明细。",
                    FontId::proportional(10.5),
                    p.dim,
                );
            });

            // 应用信息
            group_label(ui, &p, "应用信息");
            group_card(ui, &p, |ui| {
                static_row(ui, &p, "开发者", "灰调雨弦");
                divider(ui, &p);
                static_row(ui, &p, "数据来源", "Open-Meteo");
                divider(ui, &p);
                static_row(ui, &p, "地图", "OSM / CARTO / 高德");
                divider(ui, &p);
                static_row(ui, &p, "许可", "MIT");
            });

            // 安全区调试
            ui.add_space(20.0);
            let dbg = format!(
                "安全区 px L{:.0} T{:.0} R{:.0} B{:.0} · 密度 {:.2} · 留白 {:.0}/{:.0}/{:.0}/{:.0}",
                app.insets_px.0,
                app.insets_px.1,
                app.insets_px.2,
                app.insets_px.3,
                app.ppp,
                app.pad.0,
                app.pad.1,
                app.pad.2,
                app.pad.3
            );
            let dw = ui.available_width() - 40.0;
            let gal = ui.painter().layout(dbg, FontId::proportional(9.0), p.fg3, dw);
            let (row, _) = ui.allocate_exact_size(
                Vec2::new(w, gal.size().y),
                Sense::hover(),
            );
            ui.painter().galley(
                Pos2::new(row.center().x - gal.size().x / 2.0, row.top()),
                gal,
                p.fg3,
            );
            ui.add_space(20.0);
        });
}

/// 太阳图形（圆 + 地平线），用于关于页图标。
fn sun_mark(pa: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    pa.circle_filled(Pos2::new(c.x, c.y + 1.0), r * 0.62, col);
    let st = Stroke::new(1.6_f32, col);
    let y = c.y + r * 0.62;
    pa.line_segment([Pos2::new(c.x - r, y), Pos2::new(c.x + r, y)], st);
    let y2 = c.y + r * 0.28;
    pa.line_segment(
        [
            Pos2::new(c.x - r * 0.72, y2),
            Pos2::new(c.x + r * 0.72, y2),
        ],
        Stroke::new(1.4_f32, with_alpha(col, 0.45)),
    );
}


// ---------- 日期标签 ----------

fn day_label(date: &str) -> String {
    let md = date.get(5..).unwrap_or(date);
    match date_diff(date) {
        0 => "今天".to_string(),
        1 => "明天".to_string(),
        2 => "后天".to_string(),
        _ => {
            let dm = md.get(3..).unwrap_or(md);
            format!("{dm}日")
        }
    }
}

fn date_diff(date: &str) -> i64 {
    let today = today_local();
    parse_days(date) - parse_days(&today)
}

fn parse_days(date: &str) -> i64 {
    let mut it = date.split('-');
    let y: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    y * 372 + m * 31 + d
}

fn today_local() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_from_days((secs + 8 * 3600) / 86400)
}

fn civil_from_days(z: i64) -> String {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 字体预设（名称 → 匹配关键字 → 下载直链 → 下载 Referer → 未安装时的说明）。
///
/// **字体一律不内置**；仅链接可合法免费下载的字体（官方/开源授权）。
const FONT_PRESETS: &[(&str, &[&str], Option<&str>, Option<&str>, &str)] = &[
    (
        "MiSans",
        &["MiSans"],
        Some("https://hyperos.mi.com/font-download/MiSans.zip"),
        None,
        "未安装 · 可下载（约 227 MB）",
    ),
    (
        "OPPO Sans",
        &["OPPOSans"],
        Some("https://coloros-website-cn.allawnfs.com/font/OPPO_Sans_4.0.zip"),
        Some("https://www.coloros.com/"),
        "未安装 · 可下载（约 16 MB）",
    ),
    (
        "思源黑体 Source Han Sans",
        &["SourceHanSans", "NotoSansCJK"],
        Some("https://raw.githubusercontent.com/google/fonts/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf"),
        None,
        "未安装 · 可下载（约 18 MB）",
    ),
    (
        "思源宋体 Source Han Serif",
        &["SourceHanSerif", "NotoSerifCJK"],
        Some("https://raw.githubusercontent.com/google/fonts/main/ofl/notoserifsc/NotoSerifSC%5Bwght%5D.ttf"),
        None,
        "未安装 · 可下载（约 25 MB）",
    ),
    (
        "霞鹜文楷 LXGW WenKai",
        &["LXGWWenKai"],
        Some("https://raw.githubusercontent.com/lxgw/LxgwWenKai/main/fonts/TTF/LXGWWenKai-Regular.ttf"),
        None,
        "未安装 · 可下载（约 26 MB）",
    ),
    (
        "得意黑 Smiley Sans",
        &["SmileySans"],
        Some("https://raw.githubusercontent.com/deepin-community/fonts-smiley-sans/master/SmileySans-Oblique.ttf"),
        None,
        "未安装 · 可下载（约 3 MB）",
    ),
    (
        "抖音美好体 Douyin Sans",
        &["Douyin"],
        Some("https://raw.githubusercontent.com/bytedance/fonts/main/DouyinSans/DouyinSansBold.ttf"),
        None,
        "未安装 · 可下载（约 2 MB）",
    ),
    (
        "vivo Sans",
        &["vivo"],
        Some("https://developersstatic.vivo.com/developers/1cdcaef3b39848f290bf90346642ed1d/20241022/vivo%20Sans.zip"),
        None,
        "未安装 · 可下载（约 89 MB）",
    ),
    (
        "HarmonyOS Sans",
        &["HarmonyOS"],
        None,
        None,
        "未安装 · 免费商用，请到官网下载",
    ),
    (
        "阿里巴巴普惠体",
        &["AlibabaPuHuiTi"],
        Some("https://fonts.alibabadesign.com/AlibabaPuHuiTi-3.zip"),
        Some("https://fonts.alibabadesign.com/"),
        "未安装 · 可下载（约 279 MB）",
    ),
    (
        "圆体 / 圆黑",
        &["Yuanti", "FZYT", "Round"],
        None,
        None,
        "未安装 · 免费商用，请到官网下载",
    ),
];

/// 字体选择页（全屏页面，与设置页同款卡片风格；非浮动窗口）。
fn font_screen(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = p.gap;
            ui.add_space(SP_XS);

            if header(ui, &p, "选择字体", None, "← 返回") {
                app.screen = Screen::Settings;
            }

            let mut picked: Option<Option<String>> = None;
            let mut to_dl: Option<(String, String, Option<String>)> = None;

            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(
                    ui,
                    w,
                    "以下字体均不内置，需系统已安装、或自行下载后填入路径",
                    FS_CAPTION,
                    p.dim,
                );
                ui.add_space(SP_SM);
                search_field(ui, &p, "font", "搜索字体", &mut app.font_query, false);
                if !app.font_dl_msg.is_empty() {
                    ui.add_space(SP_XS);
                    wrapped(ui, w, &app.font_dl_msg, FS_CAPTION, p.gold);
                }
            });

            let q = app.font_query.to_lowercase();

            card(ui, &p, |ui| {
                if option_row(ui, &p, "系统默认", "使用设备系统字体", app.font_file.is_none()) {
                    picked = Some(None);
                }
            });

            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "预设字体", FS_SECTION, p.fg);
                ui.add_space(SP_XS);
                for (label, kws, url, referer, hint) in FONT_PRESETS {
                    if !q.is_empty()
                        && !label.to_lowercase().contains(&q)
                        && !kws.iter().any(|k| k.to_lowercase().contains(&q))
                    {
                        continue;
                    }
                    let sys = app
                        .font_names
                        .iter()
                        .find(|n| kws.iter().any(|k| n.contains(k)))
                        .cloned();
                    let cached = crate::app::font_cache_path(label);
                    let cached_s = cached.to_string_lossy().to_string();
                    let installed = sys.is_some() || cached.exists();
                    let on = app.font_file.as_deref() == Some(cached_s.as_str())
                        || sys.as_deref() == app.font_file.as_deref();
                    let busy = app.font_downloading(label);
                    let text_w = (w - 84.0).max(80.0);
                    ui.horizontal(|ui| {
                        let resp = ui
                            .allocate_ui_with_layout(
                                Vec2::new(text_w, 40.0),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                    wrapped(ui, text_w, label, FS_SECTION, if on { p.gold } else { p.fg });
                                    let st: String = if installed {
                                        "已就绪".into()
                                    } else if busy {
                                        match app.font_dl_frac_of(label) {
                                            Some(f) => format!(
                                                "下载中 {:.0}% · {:.1} MB",
                                                f * 100.0,
                                                app.font_dl_mb_of(label)
                                            ),
                                            None => {
                                                format!("下载中 {:.1} MB", app.font_dl_mb_of(label))
                                            }
                                        }
                                    } else {
                                        (*hint).into()
                                    };
                                    wrapped(ui, text_w, &st, FS_MICRO, p.dim);
                                },
                            )
                            .response;
                        let hit_text = resp.interact(Sense::click()).clicked();
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if installed {
                                let sel = if on { "使用中" } else { "使用" };
                                if pill(ui, &p, sel, on) || hit_text {
                                    let target = if cached.exists() {
                                        cached_s.clone()
                                    } else {
                                        sys.clone().unwrap_or_default()
                                    };
                                    picked = Some(Some(target));
                                }
                            } else if let Some(u) = url {
                                if dl_button(ui, &p, *label, busy, app.font_dl_frac_of(label)) {
                                    to_dl = Some((
                                        label.to_string(),
                                        u.to_string(),
                                        (*referer).map(str::to_string),
                                    ));
                                }
                            } else {
                                ui.label(RichText::new("—").size(FS_CAPTION).color(p.dim));
                            }
                        });
                    });
                    ui.add_space(SP_SM);
                }
            });

            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "系统全部字体", FS_SECTION, p.fg);
                ui.add_space(SP_XS);
                for n in &app.font_names {
                    if !q.is_empty() && !n.to_lowercase().contains(&q) {
                        continue;
                    }
                    let on = app.font_file.as_deref() == Some(n.as_str());
                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(n.as_str())
                                .size(FS_BODY)
                                .color(if on { p.gold } else { p.fg }),
                        )
                        .sense(Sense::click()),
                    );
                    if resp.clicked() {
                        picked = Some(Some(n.clone()));
                    }
                }
            });

            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "或填入字体文件绝对路径", FS_SECTION, p.fg);
                ui.add_space(SP_SM);
                let mut path = app.font_path_input.clone();
                input_row(ui, &p, "/sdcard/Download/xxx.ttf", &mut path, 12, 12);
                ui.add_space(SP_SM);
                if wide_button(ui, &p, "使用该文件") && !path.trim().is_empty() {
                    picked = Some(Some(path.trim().to_string()));
                }
                app.font_path_input = path;
            });

            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "或填入字体下载链接", FS_SECTION, p.fg);
                ui.add_space(SP_SM);
                input_row(
                    ui,
                    &p,
                    "https://…/xxx.ttf 或 xxx.zip",
                    &mut app.font_url_input,
                    12,
                    12,
                );
                let url = app.font_url_input.trim().to_string();
                let stem: String = url
                    .rsplit('/')
                    .next()
                    .unwrap_or("font")
                    .split(['?', '#'])
                    .next()
                    .unwrap_or("font")
                    .to_string();
                let name = if stem.is_empty() { "font".to_string() } else { stem };
                ui.add_space(SP_SM);
                if app.font_downloading(&name) {
                    ui.horizontal(|ui| {
                        dl_button(ui, &p, &name, true, app.font_dl_frac_of(&name));
                        let txt = match app.font_dl_frac_of(&name) {
                            Some(f) => format!(
                                "下载中 {:.0}% · {:.1} MB",
                                f * 100.0,
                                app.font_dl_mb_of(&name)
                            ),
                            None => format!("下载中 {:.1} MB", app.font_dl_mb_of(&name)),
                        };
                        ui.label(RichText::new(txt).size(FS_BODY).color(p.gold));
                    });
                } else if wide_button(ui, &p, "下载该链接") && !url.is_empty() {
                    to_dl = Some((name.clone(), url.clone(), None));
                }
            });

            if let Some(f) = picked {
                let ctx = ui.ctx().clone();
                app.set_font_file(f, &ctx);
            }
            if let Some((n, u, r)) = to_dl {
                let ctx = ui.ctx().clone();
                app.download_font(&n, &u, r.as_deref(), &ctx);
            }
            ui.add_space(SP_MD);
        });
}
