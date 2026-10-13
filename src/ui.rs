//! 界面绘制。
//!
//! 布局规范：所有区块都包在 `card()` 里（同一内边距/圆角），卡片间垂直间距一致。
//! 横向行按 `available_width` 计算列宽；**过长的文本不截断**，而是用 `wrapped()`
//! 在固定列宽内自动换行（先算出换行后高度再精确占位，绝不横向溢出）。

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::DawnGlowApp;
use crate::api::ModelMode;
use crate::geo;
use crate::map::MapSource;
use crate::model::{DayForecast, EventKind, Screen};
use crate::theme::{self, Palette};

const TILE: f64 = 256.0;

// ---------- 视觉标尺（全站统一，勿再散落字面量）----------

/// 环内大数字
const FS_HERO: f32 = 34.0;
/// 空态图标
const FS_EMOJI: f32 = 40.0;
/// 页面标题
const FS_PAGE: f32 = 18.0;
/// 区块大标题 / 主数值
const FS_TITLE: f32 = 16.0;
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
/// 页面标题行高度
const H_HEADER: f32 = 34.0;

/// 紧邻元素间距
const SP_XS: f32 = 4.0;
/// 卡片内默认间距
const SP_SM: f32 = 6.0;
/// 段落间距
const SP_MD: f32 = 8.0;
/// 卡片内层元素的圆角（比卡片小 4，形成「容器 → 控件」层级）。
fn inner_radius(p: &Palette) -> f32 {
    (p.card_radius - 4.0).max(6.0)
}

pub fn draw(app: &mut DawnGlowApp, ctx: &egui::Context) {
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    gradient(&painter, rect, &app.p);
    apply_style(ctx, &app.p);

    let (l, t, r, b) = app.pad;
    let margin = egui::Margin {
        left: i8c(l),
        top: i8c(t),
        right: i8c(r),
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
    let (fill, stroke) = if primary {
        (p.accent_soft, Stroke::new(1.0_f32, p.gold))
    } else {
        (p.chip_fill, Stroke::new(1.0_f32, p.chip_stroke))
    };
    ui.add(
        egui::Button::new(RichText::new(text).size(FS_BODY).color(p.fg))
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
        egui::Button::new(RichText::new(text).size(FS_BODY).color(p.fg))
            .fill(p.accent_soft)
            .stroke(Stroke::new(1.0_f32, p.gold)),
    )
    .clicked()
}

/// 统一的页面标题卡：标题（可选副标题）+ 右侧胶囊按钮。返回右侧按钮是否被点击。
fn header(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    subtitle: Option<&str>,
    right: &str,
) -> bool {
    let mut clicked = false;
    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 74.0).max(80.0);
            text_col(ui, w, H_HEADER, Align::LEFT, |ui| {
                ui.label(RichText::new(title).size(FS_PAGE).strong().color(p.fg));
                if let Some(s) = subtitle {
                    ui.label(RichText::new(s).size(FS_CAPTION).color(p.dim));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                clicked = pill(ui, p, right, false);
            });
        });
    });
    clicked
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
            ui.spacing_mut().item_spacing.y = app.p.gap;
            ui.add_space(4.0);

            // 标题卡始终显示真实内容（不参与骨架扫光 / 浮现）
            title_card(app, ui);

            if app.loading && app.bundle.days.is_empty() {
                // 首次加载：骨架屏（与真实内容同形）
                skeleton_page(ui, ctx, &app.p);
            } else if app.bundle.days.is_empty() {
                empty_card(app, ui, ctx);
            } else {
                let p = app.p;
                let ra = app.reveal_at;
                reveal_block(ui, &p, ra, 0, |ui| location_card(app, ui, ctx));
                reveal_block(ui, &p, ra, 1, |ui| celestial_card(app, ui));
                let idx = app.day.min(app.bundle.days.len() - 1);
                let day = app.bundle.days[idx].clone();
                let kind = app.kind;
                reveal_block(ui, &p, ra, 3, |ui| score_card(app, ui, &day, kind));
                reveal_block(ui, &p, ra, 4, |ui| metrics_card(app, ui, &day, kind));
                reveal_block(ui, &p, ra, 5, |ui| cloud_card(app, ui, &day, kind));
                reveal_block(ui, &p, ra, 6, |ui| factors_card(app, ui, &day, kind));
                if app.show_attribution {
                    reveal_block(ui, &p, ra, 7, |ui| attribution(app, ui));
                }
            }
            ui.add_space(8.0);

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

/// 骨架屏：结构与真实页面一一对应，共用一道斜向扫光。
fn skeleton_page(ui: &mut egui::Ui, ctx: &egui::Context, p: &Palette) {
    let mut rects: Vec<(Rect, f32)> = Vec::new();
    let top = ui.cursor().min.y;

    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            skel_bar(ui, &mut rects, 16.0, 16.0, 5.0);
            ui.vertical(|ui| {
                skel_bar(ui, &mut rects, 110.0, 15.0, 5.0);
                skel_bar(ui, &mut rects, 80.0, 11.0, 4.0);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                skel_bar(ui, &mut rects, 62.0, 30.0, 14.0);
                skel_bar(ui, &mut rects, 56.0, 30.0, 14.0);
            });
        });
    });
    card(ui, p, |ui| skel_row(ui, &mut rects, 4, 38.0, 8.0, 12.0));
    card(ui, p, |ui| skel_row(ui, &mut rects, 2, 42.0, 8.0, 14.0));
    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            skel_bar(ui, &mut rects, 116.0, 116.0, 58.0);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                skel_bar(ui, &mut rects, 120.0, 14.0, 5.0);
                ui.add_space(8.0);
                skel_bar(ui, &mut rects, 150.0, 19.0, 6.0);
                ui.add_space(8.0);
                skel_bar(ui, &mut rects, 100.0, 14.0, 5.0);
                ui.add_space(4.0);
                skel_bar(ui, &mut rects, 80.0, 12.0, 4.0);
            });
        });
    });
    card(ui, p, |ui| {
        let w = ui.available_width() / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for _ in 0..4 {
                ui.allocate_ui_with_layout(
                    Vec2::new(w, 44.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        skel_bar(ui, &mut rects, 44.0, 17.0, 5.0);
                        ui.add_space(4.0);
                        skel_bar(ui, &mut rects, 40.0, 10.0, 4.0);
                    },
                );
            }
        });
    });
    card(ui, p, |ui| {
        skel_bar(ui, &mut rects, 90.0, 14.0, 5.0);
        skel_bar(ui, &mut rects, 170.0, 11.0, 4.0);
        ui.add_space(8.0);
        for _ in 0..4 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                skel_bar(ui, &mut rects, 46.0, 12.0, 4.0);
                ui.add_space(10.0);
                let bw = (ui.available_width() - 44.0 - 10.0).max(40.0);
                skel_bar(ui, &mut rects, bw, 10.0, 5.0);
                ui.add_space(10.0);
                skel_bar(ui, &mut rects, 44.0, 12.0, 4.0);
            });
        }
    });
    card(ui, p, |ui| skel_bar(ui, &mut rects, 90.0, 14.0, 5.0));

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

fn title_card(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    if header(ui, &p, "朝霞预报", Some("GFS · ECMWF · CAMS"), "设置") {
        app.screen = Screen::Settings;
    }
}

fn location_card(app: &mut DawnGlowApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let p = app.p;
    let name = display_name(&app.location.name, app.compact_name);
    let coord = format!("{:.3}, {:.3}", app.location.lat, app.location.lon);
    card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("📍").size(FS_ITEM));
            let reserve = 152.0;
            let w = (ui.available_width() - reserve).max(60.0);
            ui.vertical(|ui| {
                ui.set_max_width(w);
                wrapped(ui, w, &name, FS_ITEM, p.fg);
                wrapped(ui, w, &coord, FS_CAPTION, p.dim);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if pill(ui, &p, "🛰 定位", false) {
                    app.use_gps(ctx);
                }
                if pill(ui, &p, "🗺 选点", true) {
                    app.screen = Screen::Map;
                }
            });
        });
    });
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

/// 朝暮天象切换 + 全周天象日历（合并卡片）。
fn celestial_card(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    let p = app.p;
    let is_dawn = app.kind == EventKind::Sunrise;
    let sel = app.day;
    let rows: Vec<(String, f32)> = app
        .bundle
        .days
        .iter()
        .map(|d| (day_label(&d.date), d.score_for(app.kind).score))
        .collect();

    card(ui, &p, |ui| {
        let full_w = ui.available_width();

        // 朝暮双生一体化胶囊切换器
        let switch_h = 38.0;
        let (bg_rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, switch_h), Sense::click());
        let painter = ui.painter();
        let half_w = full_w / 2.0;

        painter.rect_filled(bg_rect, 19.0, p.card_fill);
        painter.rect_stroke(
            bg_rect,
            19.0,
            Stroke::new(1.0_f32, p.chip_fill),
            egui::StrokeKind::Inside,
        );

        // 滑块位置与配色都做非线性（ease-out）平滑过渡
        let t = ui.ctx().animate_bool_with_time_and_easing(
            egui::Id::new("dawn_dusk_toggle"),
            !is_dawn,
            0.32,
            egui::emath::easing::cubic_out,
        );
        let active_rect = Rect::from_min_size(
            Pos2::new(bg_rect.min.x + half_w * t, bg_rect.min.y),
            Vec2::new(half_w, switch_h),
        );
        let active_color = lerp_color(p.dawn, p.dusk, t);
        let active_fill = Color32::from_rgba_unmultiplied(
            active_color.r(),
            active_color.g(),
            active_color.b(),
            50,
        );
        painter.rect_filled(active_rect, 19.0, active_fill);
        painter.rect_stroke(
            active_rect,
            19.0,
            Stroke::new(1.0_f32, with_alpha(active_color, 0.8)),
            egui::StrokeKind::Inside,
        );

        let dawn_center = Pos2::new(bg_rect.min.x + half_w * 0.5, bg_rect.center().y);
        let dusk_center = Pos2::new(bg_rect.min.x + half_w * 1.5, bg_rect.center().y);
        let dawn_txt = lerp_color(p.dim, p.dawn, 1.0 - t);
        let dusk_txt = lerp_color(p.dim, p.dusk, t);

        painter.text(
            dawn_center,
            Align2::CENTER_CENTER,
            "🌅 晨曦 · 朝霞",
            FontId::proportional(FS_SECTION),
            dawn_txt,
        );
        painter.text(
            dusk_center,
            Align2::CENTER_CENTER,
            "🌇 暮色 · 晚霞",
            FontId::proportional(FS_SECTION),
            dusk_txt,
        );

        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if pos.x < bg_rect.min.x + half_w {
                    app.kind = EventKind::Sunrise;
                } else {
                    app.kind = EventKind::Sunset;
                }
            }
        }

        ui.add_space(8.0);

        // 全周天象日历：鲜艳度 ≥60% 的日期以火烧云微光点缀
        if !rows.is_empty() {
            let gap = 6.0;
            let n = rows.len() as f32;
            let card_w = ((full_w - gap * (n - 1.0)) / n).max(46.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (i, (label, score_val)) in rows.iter().enumerate() {
                    let on = i == sel;
                    let (item_rect, item_resp) =
                        ui.allocate_exact_size(Vec2::new(card_w, 52.0), Sense::click());
                    let ipainter = ui.painter();

                    let item_fill = if on { p.accent_soft } else { p.card_fill };
                    let item_stroke = if on {
                        Stroke::new(1.2_f32, p.accent)
                    } else {
                        Stroke::new(1.0_f32, p.card_stroke)
                    };
                    let ir = inner_radius(&p);
                    ipainter.rect_filled(item_rect, ir, item_fill);
                    ipainter.rect_stroke(item_rect, ir, item_stroke, egui::StrokeKind::Inside);

                    ipainter.text(
                        Pos2::new(item_rect.center().x, item_rect.top() + 15.0),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(FS_CAPTION),
                        if on { p.gold } else { p.dim },
                    );

                    let dot = Pos2::new(item_rect.center().x, item_rect.bottom() - 14.0);
                    if *score_val >= 0.60 {
                        let dot_col = theme::score_color(&p, *score_val);
                        ipainter.circle_filled(dot, 3.5, dot_col);
                        ipainter.circle_filled(dot, 6.0, with_alpha(dot_col, 0.35));
                    } else {
                        ipainter.text(
                            dot,
                            Align2::CENTER_CENTER,
                            format!("{:.0}", score_val * 100.0),
                            FontId::proportional(FS_MICRO),
                            with_alpha(p.dim, 0.7),
                        );
                    }

                    if item_resp.clicked() {
                        app.day = i;
                    }
                }
            });
        }
    });
}

fn score_card(app: &DawnGlowApp, ui: &mut egui::Ui, day: &DayForecast, kind: EventKind) {
    let p = app.p;
    let sc = day.score_for(kind);
    let accent = match kind {
        EventKind::Sunrise => p.dawn,
        EventKind::Sunset => p.dusk,
    };
    let time_text = day
        .time_for(kind)
        .map(|t| format!("时间 {t}"))
        .unwrap_or_else(|| "时间 —".into());
    let conf_text = format!("置信度 {:.0}%", sc.confidence * 100.0);
    let title = format!("{} {} · {}", kind.emoji(), kind.label(), day.date.get(5..).unwrap_or(""));
    let grade = sc.grade().to_string();
    card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            let size = 116.0;
            let (resp, painter) = ui.allocate_painter(Vec2::splat(size), Sense::hover());
            let center = resp.rect.center();
            let radius = size / 2.0 - 9.0;
            painter.circle_stroke(center, radius, Stroke::new(9.0_f32, p.track));
            draw_arc(&painter, center, radius, sc.score, theme::score_color(&p, sc.score));
            painter.text(
                center + Vec2::new(0.0, -6.0),
                Align2::CENTER_CENTER,
                format!("{:.0}", sc.score * 100.0),
                FontId::proportional(FS_HERO),
                p.fg,
            );
            painter.text(
                center + Vec2::new(0.0, 20.0),
                Align2::CENTER_CENTER,
                "鲜艳度",
                FontId::proportional(FS_CAPTION),
                p.dim,
            );

            ui.add_space(14.0);
            let tw = ui.available_width().max(60.0);
            ui.vertical(|ui| {
                ui.set_max_width(tw);
                ui.add_space(4.0);
                wrapped(ui, tw, &title, FS_SECTION, accent);
                ui.add_space(2.0);
                wrapped(ui, tw, &grade, FS_PAGE, p.fg);
                ui.add_space(2.0);
                wrapped(ui, tw, &time_text, FS_SECTION, p.dim);
                wrapped(ui, tw, &conf_text, FS_CAPTION, p.dim);
            });
        });
    });
}

fn draw_arc(painter: &egui::Painter, center: Pos2, radius: f32, frac: f32, color: Color32) {
    let start = -std::f32::consts::FRAC_PI_2;
    let total = frac.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let segs = 72;
    let mut prev: Option<Pos2> = None;
    for i in 0..=segs {
        let a = start + total * (i as f32 / segs as f32);
        let pt = center + Vec2::new(a.cos(), a.sin()) * radius;
        if let Some(pp) = prev {
            painter.line_segment([pp, pt], Stroke::new(9.0_f32, color));
        }
        prev = Some(pt);
    }
}

fn metrics_card(app: &DawnGlowApp, ui: &mut egui::Ui, day: &DayForecast, kind: EventKind) {
    let p = app.p;
    let sc = day.score_for(kind);
    let aod = sc.aod.map(|a| format!("{a:.2}")).unwrap_or_else(|| "—".into());
    let vis = sc
        .visibility_km
        .map(|v| format!("{v:.0}km"))
        .unwrap_or_else(|| "—".into());
    let hum = sc
        .humidity
        .map(|h| format!("{h:.0}%"))
        .unwrap_or_else(|| "—".into());
    let rain = sc
        .rain_prob
        .map(|r| format!("{r:.0}%"))
        .unwrap_or_else(|| "—".into());
    let items = [
        ("气溶胶", aod, p.gold),
        ("能见度", vis, p.fg),
        ("湿度", hum, p.fg),
        ("降水", rain, p.fg),
    ];
    card(ui, &p, |ui| {
        let w = ui.available_width() / items.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (label, value, color) in items {
                ui.allocate_ui_with_layout(
                    Vec2::new(w, 44.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.label(RichText::new(value).size(FS_TITLE).strong().color(color));
                        ui.label(RichText::new(label).size(FS_MICRO).color(p.dim));
                    },
                );
            }
        });
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
        ui.horizontal(|ui| {
            let btn_w = 72.0;
            let edit_w = (ui.available_width() - btn_w - 8.0).max(80.0);
            let edit = egui::TextEdit::singleline(&mut app.search_query)
                .hint_text("搜索地名，如 上海")
                .desired_width(edit_w);
            let resp = ui.add_sized([edit_w, 30.0], edit);
            let go = pill(ui, &p, "搜索", true)
                || (resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                app.start_search(ctx);
            }
        });
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

fn settings(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let p = app.p;
            ui.spacing_mut().item_spacing.y = p.gap;
            ui.add_space(SP_XS);

            if header(ui, &p, "设置", None, "← 返回") {
                app.screen = Screen::Forecast;
            }

            // —— 字体（自定义，不内置）——
            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "字体", FS_SECTION, p.fg);
                let cur = app.font_file.clone().unwrap_or_else(|| "系统默认".into());
                wrapped(ui, w, &format!("当前：{cur}"), FS_CAPTION, p.dim);
                ui.add_space(SP_MD);
                if wide_button(ui, &p, "选择字体") {
                    app.screen = Screen::Font;
                }
            });

            // —— 预报模式 ——
            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "预报模式", FS_SECTION, p.fg);
                ui.add_space(SP_XS);
                let mut picked = None;
                for m in ModelMode::ALL {
                    if option_row(ui, &p, m.name(), m.desc(), m == app.model_mode) {
                        picked = Some(m);
                    }
                    ui.add_space(SP_SM);
                }
                if let Some(m) = picked {
                    let ctx = ui.ctx().clone();
                    app.set_model_mode(m, &ctx);
                }
            });

            // —— 地图 ——
            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "地图底图", FS_SECTION, p.fg);
                ui.add_space(SP_XS);
                let mut picked = None;
                for s in MapSource::ALL {
                    if option_row(ui, &p, s.name(), s.desc(), s == app.map.source) {
                        picked = Some(s);
                    }
                    ui.add_space(SP_SM);
                }
                if let Some(s) = picked {
                    app.set_map_source(s);
                }
            });

            // —— 显示 ——
            card(ui, &p, |ui| {
                if let Some(i) = choice_row(
                    ui,
                    &p,
                    "地点名称显示",
                    "精简只显示第一段；完整名称较长时自动换行",
                    &["精简", "完整"],
                    if app.compact_name { 0 } else { 1 },
                ) {
                    app.compact_name = i == 0;
                    app.save_config();
                }
                ui.add_space(SP_MD);
                if let Some(i) = choice_row(
                    ui,
                    &p,
                    "默认事件",
                    "打开时先看朝霞还是晚霞",
                    &["朝霞", "晚霞"],
                    if app.default_sunrise { 0 } else { 1 },
                ) {
                    let sunrise = i == 0;
                    app.default_sunrise = sunrise;
                    app.kind = if sunrise {
                        EventKind::Sunrise
                    } else {
                        EventKind::Sunset
                    };
                    app.save_config();
                }
                ui.add_space(SP_MD);
                let nf = toggle_row(
                    ui,
                    &p,
                    "评分因子默认展开",
                    "进入时默认展示各因子明细",
                    "factors",
                    app.show_factors,
                );
                if nf != app.show_factors {
                    app.show_factors = nf;
                    app.save_config();
                }
                ui.add_space(SP_MD);
                let na = toggle_row(
                    ui,
                    &p,
                    "显示数据来源署名",
                    "页脚显示数据来源与地图署名",
                    "attrib",
                    app.show_attribution,
                );
                if na != app.show_attribution {
                    app.show_attribution = na;
                    app.save_config();
                }
            });

            // —— 关于（并入设置）——
            card(ui, &p, |ui| {
                let w = ui.available_width();
                wrapped(ui, w, "关于", FS_SECTION, p.gold);
                let lines = [
                    ("数据来源", "Open-Meteo 聚合：NCEP GFS、ECMWF IFS、Copernicus CAMS"),
                    ("预报要素", "日出/日落时间、火烧云鲜艳度、气溶胶 AOD、分层云量"),
                    ("评分模型", "云型配置 / 能见度 / 气溶胶 / 湿度 / 降水 多因子加权"),
                    ("许可", "数据 CC BY 4.0；地图 © OpenStreetMap contributors / © CARTO"),
                ];
                for (k, v) in lines {
                    ui.add_space(SP_XS);
                    wrapped(ui, w, k, FS_BODY, p.fg);
                    wrapped(ui, w, v, FS_CAPTION, p.dim);
                }
                ui.add_space(SP_SM);
                wrapped(
                    ui,
                    w,
                    "预报本质为概率性参考，>48 小时误差较大，建议当天临近再确认。",
                    FS_CAPTION,
                    p.dim,
                );
                ui.add_space(SP_MD);
                if pill(ui, &p, "版本信息 · 检查更新", true) {
                    app.screen = Screen::Version;
                }
            });
            ui.add_space(SP_MD);
        });
}

/// 可选行（点整行选择）：名称 + 说明 + 选中标记。
fn option_row(ui: &mut egui::Ui, p: &Palette, name: &str, desc: &str, on: bool) -> bool {
    let w = ui.available_width();
    let text_w = (w - 42.0).max(60.0);
    let name = ui.painter().layout(
        name.to_string(),
        FontId::proportional(FS_ITEM),
        if on { p.gold } else { p.fg },
        text_w,
    );
    let name_h = name.size().y;
    let desc = ui
        .painter()
        .layout(desc.to_string(), FontId::proportional(FS_CAPTION), p.dim, text_w);
    let h = name_h + desc.size().y + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, inner_radius(&p), p.accent_soft);
    }
    painter.galley(Pos2::new(rect.left() + 10.0, rect.top() + 7.0), name, p.fg);
    painter.galley(
        Pos2::new(rect.left() + 10.0, rect.top() + 7.0 + name_h),
        desc,
        p.dim,
    );
    if on {
        painter.text(
            Pos2::new(rect.right() - 16.0, rect.center().y),
            Align2::CENTER_CENTER,
            "●",
            FontId::proportional(FS_CAPTION),
            p.gold,
        );
    }
    resp.clicked()
}

/// 设置项：标题 + 说明在上，等宽选项按钮在下（返回新选项索引）。
fn choice_row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    desc: &str,
    options: &[&str],
    sel: usize,
) -> Option<usize> {
    let mut picked = None;
    let w = ui.available_width();
    wrapped(ui, w, title, FS_SECTION, p.fg);
    wrapped(ui, w, desc, FS_CAPTION, p.dim);
    ui.add_space(6.0);
    let n = options.len() as f32;
    let gap = 8.0;
    let bw = ((w - gap * (n - 1.0)) / n).max(46.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, opt) in options.iter().enumerate() {
            let on = i == sel;
            let fill = if on { p.accent_soft } else { p.card_fill };
            let txt = if on { p.gold } else { p.dim };
            let b = egui::Button::new(RichText::new(*opt).size(FS_BODY).color(txt)).fill(fill);
            if ui.add_sized([bw, H_CHIP], b).clicked() {
                picked = Some(i);
            }
        }
    });
    picked
}

/// 滑动开关（带滑块动画），返回是否被点击。
fn switch(ui: &mut egui::Ui, p: &Palette, salt: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(egui::Id::new(("switch", salt)), on, 0.18);
    let painter = ui.painter();
    let r = rect.height() / 2.0;
    painter.rect_filled(rect, r, lerp_color(p.track, p.accent, t));
    if p.card_stroke != Color32::TRANSPARENT {
        painter.rect_stroke(
            rect,
            r,
            Stroke::new(1.0_f32, with_alpha(p.card_stroke, 0.6)),
            egui::StrokeKind::Inside,
        );
    }
    let knob_r = r - 3.0;
    let x = rect.left() + r + (rect.width() - 2.0 * r) * t;
    painter.circle_filled(Pos2::new(x, rect.center().y), knob_r, Color32::WHITE);
    resp.clicked()
}

/// 开关设置行，返回新状态。
fn toggle_row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    desc: &str,
    salt: &str,
    on: bool,
) -> bool {
    let mut toggled = false;
    ui.horizontal(|ui| {
        let text_w = (ui.available_width() - 62.0).max(80.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_w);
            wrapped(ui, text_w, title, FS_SECTION, p.fg);
            wrapped(ui, text_w, desc, FS_CAPTION, p.dim);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if switch(ui, p, salt, on) {
                toggled = true;
            }
        });
    });
    if toggled {
        !on
    } else {
        on
    }
}

// ---------- 版本页 ----------

fn version(app: &mut DawnGlowApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let p = app.p;
            ui.spacing_mut().item_spacing.y = p.gap;
            ui.add_space(SP_XS);

    if header(ui, &p, "版本", None, "← 返回") {
        app.screen = Screen::Settings;
    }

    card(ui, &p, |ui| {
        let w = ui.available_width();
        wrapped(ui, w, "朝霞预报 DawnGlow", FS_TITLE, p.fg);
        wrapped(
            ui,
            w,
            &format!("版本 {}", env!("CARGO_PKG_VERSION")),
            FS_CAPTION,
            p.dim,
        );
        ui.add_space(SP_MD);
        wrapped(ui, w, "开发者", FS_CAPTION, p.gold);
        wrapped(ui, w, "灰调雨弦", FS_TITLE, p.fg);
        ui.add_space(SP_MD);
        wrapped(ui, w, "数据来源：Open-Meteo（GFS / ECMWF / CAMS）", FS_CAPTION, p.dim);
        wrapped(ui, w, "地图：OpenStreetMap contributors / CARTO", FS_CAPTION, p.dim);
    });

    card(ui, &p, |ui| {
        let w = ui.available_width();
        wrapped(ui, w, "软件更新", FS_SECTION, p.fg);
        ui.add_space(SP_SM);
        let status = app.update_status.clone();
        if wide_button(ui, &p, "检查更新") {
            app.check_update(ui.ctx());
        }
        if !status.is_empty() {
            ui.add_space(SP_MD);
            let color = if status.starts_with("发现") { p.gold } else { p.dim };
            wrapped(ui, w, &status, FS_CAPTION, color);
        }
        ui.add_space(SP_MD);
        wrapped(
            ui,
            w,
            &format!(
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
            ),
            FS_MICRO,
            p.dim,
        );
            });
        });
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
                ui.add(
                    egui::TextEdit::singleline(&mut app.font_query)
                        .hint_text("搜索字体")
                        .desired_width(w),
                );
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
                ui.add(
                    egui::TextEdit::singleline(&mut path)
                        .hint_text("/sdcard/Download/xxx.ttf")
                        .desired_width(w),
                );
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
                ui.add(
                    egui::TextEdit::singleline(&mut app.font_url_input)
                        .hint_text("https://…/xxx.ttf 或 xxx.zip")
                        .desired_width(w),
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
