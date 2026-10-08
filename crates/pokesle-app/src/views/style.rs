//! 見た目(本家 style.scss の配色と、design/ 以下の部品)

use eframe::egui::{self, Color32};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const INK: Color32 = hex(0x30394d);
pub const MUTED: Color32 = hex(0x6b7486);
pub const PRIMARY: Color32 = hex(0x66769a);
pub const PRIMARY_STRONG: Color32 = hex(0x4d5d7d);
pub const PRIMARY_SOFT: Color32 = hex(0xeff1f6);
pub const EXECUTE: Color32 = hex(0x3d73c9);
pub const EXECUTE_STRONG: Color32 = hex(0x2e5caa);
pub const ACCENT: Color32 = hex(0xdf8650);
pub const DANGER: Color32 = hex(0xbf6575);
pub const DANGER_STRONG: Color32 = hex(0xa85162);
pub const LINE: Color32 = hex(0xdce0e8);
pub const SURFACE: Color32 = hex(0xfffefd);
pub const SURFACE_SUBTLE: Color32 = hex(0xf6f7f9);
/// 設定ボタン上の注意書き(本家 .caution)
pub const CAUTION: Color32 = Color32::YELLOW;
/// 設定表の見出し(本家 setting-table th)
pub const TH: Color32 = hex(0x888888);

/// アプリ全体の配色を本家に合わせる(ライトテーマ固定)
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut v = egui::Visuals::light();
    v.override_text_color = Some(INK);
    v.panel_fill = SURFACE_SUBTLE;
    v.window_fill = SURFACE;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = hex(0xf2f4f8);
    v.window_stroke = egui::Stroke::new(1.0, LINE);
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.selection.bg_fill = hex(0xc9d2e6);
    v.selection.stroke = egui::Stroke::new(1.0, PRIMARY_STRONG);
    v.hyperlink_color = EXECUTE;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    w.inactive.bg_stroke = egui::Stroke::new(1.0, hex(0xc7cdda));
    w.inactive.weak_bg_fill = Color32::WHITE;
    w.inactive.bg_fill = Color32::WHITE;
    w.hovered.bg_stroke = egui::Stroke::new(1.0, hex(0x909bb3));
    w.hovered.weak_bg_fill = PRIMARY_SOFT;
    w.active.bg_stroke = egui::Stroke::new(1.0, PRIMARY);
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = egui::CornerRadius::same(5);
    }
    ctx.set_visuals_of(egui::Theme::Light, v);
}

/// 左から右へのグラデーション
pub fn hgradient(painter: &egui::Painter, rect: egui::Rect, stops: &[(f32, Color32)]) {
    let mut mesh = egui::Mesh::default();
    for (i, &(t, c)) in stops.iter().enumerate() {
        let x = egui::lerp(rect.left()..=rect.right(), t);
        mesh.colored_vertex(egui::pos2(x, rect.top()), c);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), c);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// 設定ボタンのラベル。`true` の部分は注意色(黄色)で出す
pub type Label = Vec<(String, bool)>;

pub fn plain(s: impl Into<String>) -> Label {
    vec![(s.into(), false)]
}

/// 本家 design/setting-button.vue の見た目のボタン(右端に ▼)
pub fn setting_button(ui: &mut egui::Ui, label: &Label, important: bool) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let mut job = egui::text::LayoutJob::default();
    for (text, caution) in label {
        let format = egui::TextFormat { font_id: font.clone(), color: if *caution { CAUTION } else { Color32::WHITE }, ..Default::default() };
        job.append(text, 0.0, format);
    }
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let pad = egui::vec2(11.0, 6.0);
    let arrow = 12.0;
    let size = egui::vec2(galley.size().x + pad.x * 2.0 + arrow + 6.0, galley.size().y.max(14.0) + pad.y * 2.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let rect = if hovered { rect.translate(egui::vec2(0.0, -1.0)) } else { rect };
        let fill = match (important, hovered) {
            (true, _) => ACCENT,
            (false, true) => hex(0x44526f),
            (false, false) => PRIMARY_STRONG,
        };
        let p = ui.painter();
        p.rect(rect, 6.0, fill, egui::Stroke::new(1.0, Color32::from_white_alpha(64)), egui::StrokeKind::Inside);
        let text_pos = egui::pos2(rect.left() + pad.x, rect.center().y - galley.size().y / 2.0);
        p.galley(text_pos, galley, Color32::WHITE);
        let c = egui::pos2(rect.right() - pad.x - arrow / 2.0, rect.center().y);
        let h = arrow * 0.25;
        p.line(vec![c + egui::vec2(-arrow * 0.4, -h), c + egui::vec2(0.0, h), c + egui::vec2(arrow * 0.4, -h)], egui::Stroke::new(2.0, Color32::WHITE));
    }
    response
}

/// 本家 form-button.vue。`kind` は通常/実行(青)/重要(赤)
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Normal,
    Execute,
    Important,
}

pub fn form_button(ui: &mut egui::Ui, text: &str, kind: ButtonKind) -> egui::Response {
    let (fill, hover) = match kind {
        ButtonKind::Normal => (PRIMARY, PRIMARY_STRONG),
        ButtonKind::Execute => (EXECUTE, EXECUTE_STRONG),
        ButtonKind::Important => (DANGER, DANGER_STRONG),
    };
    let enabled = ui.is_enabled();
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), Color32::WHITE));
    let pad = egui::vec2(11.0, 5.0);
    let (rect, response) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::click());
    let response = if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response };
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let rect = if hovered { rect.translate(egui::vec2(0.0, -1.0)) } else { rect };
        let fill = if !enabled { hex(0xb7c0da) } else if hovered { hover } else { fill };
        ui.painter().rect(rect, 6.0, fill, egui::Stroke::new(1.0, Color32::from_white_alpha(64)), egui::StrokeKind::Inside);
        ui.painter().galley(rect.min + pad, galley, Color32::WHITE);
    }
    response
}

fn popup_id(ctx_id: &str) -> egui::Id {
    egui::Id::new(("setting_popup", ctx_id))
}

/// 設定ポップアップを開く
pub fn open_popup(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.insert_temp(popup_id(id), true));
}

pub fn is_popup_open(ctx: &egui::Context, id: &str) -> bool {
    ctx.data(|d| d.get_temp::<bool>(popup_id(id))).unwrap_or(false)
}

/// 本家 setting-button のポップアップ(見出し付き・外側クリックで閉じる)。
/// 開いていればモーダルで表示し、中身を描く。
pub fn popup(ctx: &egui::Context, id: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    if !is_popup_open(ctx, id) {
        return;
    }
    let max_h = (ctx.content_rect().height() - 140.0).max(200.0);
    let max_w = (ctx.content_rect().width() - 100.0).max(300.0);
    let frame = egui::Frame::new().fill(SURFACE).stroke(egui::Stroke::new(1.0, LINE)).corner_radius(10).shadow(egui::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(40, 49, 91, 50),
    });
    let mut close = false;
    let r = egui::Modal::new(popup_id(id).with("modal")).frame(frame).show(ctx, |ui| {
        ui.set_max_width(max_w);
        // 見出し
        let header = ui.allocate_ui_with_layout(egui::vec2(ui.available_width().max(320.0), 44.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(15.0);
            ui.label(egui::RichText::new(title).size(20.0).strong().color(PRIMARY_STRONG));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.add(egui::Button::new(egui::RichText::new("×").size(22.0).color(TH)).frame(false)).clicked() {
                    close = true;
                }
            });
        });
        let hr = header.response.rect;
        ui.painter().with_clip_rect(hr).hline(hr.x_range(), hr.bottom(), egui::Stroke::new(1.0, LINE));
        egui::ScrollArea::vertical().max_height(max_h - 44.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Frame::new().inner_margin(20).show(ui, add_contents);
        });
    });
    if close || r.should_close() {
        ctx.data_mut(|d| d.remove::<bool>(popup_id(id)));
    }
}

/// 設定ボタンとポップアップの組(本家 SettingButton)
pub fn setting(ui: &mut egui::Ui, id: &str, title: &str, label: &Label, important: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    if setting_button(ui, label, important).clicked() {
        open_popup(ui.ctx(), id);
    }
    popup(ui.ctx(), id, title, add_contents);
}

/// 本家 setting-table の見出しセル
pub fn th(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TH).strong());
}

/// 本家の <small> 注記
pub fn small(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.label(egui::RichText::new(text).size(11.0).color(MUTED));
    });
}

/// 本家 BaseAlert(情報)
pub fn alert(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(hex(0xedf3fb)).stroke(egui::Stroke::new(1.0, hex(0xbcd0ec))).corner_radius(6).inner_margin(10).show(ui, |ui| {
        ui.label(text);
    });
}

/// 本家 DangerAlert
pub fn danger(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(hex(0xfbeef0)).stroke(egui::Stroke::new(1.0, hex(0xe8b8c1))).corner_radius(6).inner_margin(10).show(ui, |ui| {
        ui.label(egui::RichText::new(text).color(DANGER_STRONG));
    });
}

/// 見出し付きの区切り(本家 setting-section-title)
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().color(PRIMARY_STRONG));
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, LINE));
    ui.add_space(4.0);
}

/// 数値入力(本家 InputNumber)。`percent` なら値×100 で表示する
pub fn number(ui: &mut egui::Ui, value: &mut f64, speed: f64, width: f32) -> bool {
    ui.add_sized([width, 22.0], egui::DragValue::new(value).speed(speed)).changed()
}

pub fn percent_number(ui: &mut egui::Ui, value: &mut f64, width: f32) -> bool {
    let mut p = *value * 100.0;
    let changed = ui.add_sized([width, 22.0], egui::DragValue::new(&mut p).speed(1.0).max_decimals(2)).changed();
    if changed {
        *value = p / 100.0;
    }
    changed
}

/// 数値を JS の文字列化に近い形で出す(整数なら小数点なし)
pub fn js_num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

/// 空欄で null になる数値入力(`zero_is_null` なら本家 `$event || null` と同じく 0 も null)
pub fn opt_number(ui: &mut egui::Ui, id: &str, value: &mut Option<f64>, zero_is_null: bool, width: f32, hint: &str) -> bool {
    let id = egui::Id::new(("opt_number", id));
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.map(js_num).unwrap_or_default());
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width));
    let mut changed = false;
    if r.changed() {
        let v = text.trim().parse::<f64>().ok().filter(|x| !zero_is_null || *x != 0.0);
        if v != *value {
            *value = v;
            changed = true;
        }
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    changed
}

/// 本家 HelpButton(「?」にマウスを乗せると説明を出す)
pub fn help(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new("?").strong().color(PRIMARY)).on_hover_text(text);
}
