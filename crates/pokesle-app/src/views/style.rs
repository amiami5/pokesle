//! 見た目(本家 style.scss の配色と、design/ 以下の部品)

use eframe::egui::{self, Color32};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// ダークテーマか
pub fn is_dark() -> bool {
    DARK.load(std::sync::atomic::Ordering::Relaxed)
}

/// (ライト, ダーク)の色を現在のテーマで選ぶ
pub fn pick(light: u32, dark: u32) -> Color32 {
    hex(if is_dark() { dark } else { light })
}

pub fn ink() -> Color32 {
    pick(0x30394d, 0xe3e7ef)
}
pub fn muted() -> Color32 {
    pick(0x6b7486, 0x9aa4b8)
}
pub fn primary() -> Color32 {
    pick(0x66769a, 0x5d6d93)
}
pub fn primary_strong() -> Color32 {
    pick(0x4d5d7d, 0x8da0cc)
}
pub fn primary_soft() -> Color32 {
    pick(0xeff1f6, 0x2d3447)
}
pub fn mint() -> Color32 {
    hex(0x59a98f)
}
pub fn mint_soft() -> Color32 {
    pick(0xedf7f3, 0x24382f)
}
pub fn execute() -> Color32 {
    pick(0x3d73c9, 0x3d73c9)
}
pub fn execute_strong() -> Color32 {
    pick(0x2e5caa, 0x4f86e0)
}
pub fn accent() -> Color32 {
    hex(0xdf8650)
}
pub fn danger_color() -> Color32 {
    hex(0xbf6575)
}
pub fn danger_strong() -> Color32 {
    hex(0xa85162)
}
pub fn line_color() -> Color32 {
    pick(0xdce0e8, 0x3b4356)
}
pub fn surface() -> Color32 {
    pick(0xfffefd, 0x232834)
}
pub fn surface_subtle() -> Color32 {
    pick(0xf6f7f9, 0x1a1e27)
}
/// 設定ボタン上の注意書き(本家 .caution)
pub fn caution_color() -> Color32 {
    Color32::YELLOW
}
/// 設定表の見出し(本家 setting-table th)
pub fn th_color() -> Color32 {
    pick(0x888888, 0x9ea6b6)
}
/// 未解放(無効)の表示の不透明度。ダークでは明るい部品が背景に埋もれず、解放済みとの差がはっきり出るよう低めにする
pub fn disabled_alpha() -> f32 {
    if is_dark() { 0.4 } else { 0.5 }
}

/// 不足などの警告の文字色
pub fn shortage() -> Color32 {
    pick(0xb21d1d, 0xff7b7b)
}

/// アプリ全体の配色を本家に合わせる。`dark` ならダークテーマ
pub fn apply(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, std::sync::atomic::Ordering::Relaxed);
    let (theme, mut v) = if dark { (egui::Theme::Dark, egui::Visuals::dark()) } else { (egui::Theme::Light, egui::Visuals::light()) };
    ctx.set_theme(theme);
    let field = pick(0xffffff, 0x2a3040);
    v.override_text_color = Some(ink());
    v.panel_fill = surface_subtle();
    v.window_fill = surface();
    v.extreme_bg_color = field;
    v.faint_bg_color = pick(0xf2f4f8, 0x2c3344);
    v.window_stroke = egui::Stroke::new(1.0, line_color());
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.selection.bg_fill = pick(0xc9d2e6, 0x3c4a6e);
    v.selection.stroke = egui::Stroke::new(1.0, primary_strong());
    v.hyperlink_color = if dark { hex(0x7fb0ff) } else { execute() };
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0, line_color());
    w.inactive.bg_stroke = egui::Stroke::new(1.0, pick(0xc7cdda, 0x4a5368));
    w.inactive.weak_bg_fill = field;
    w.inactive.bg_fill = field;
    w.hovered.bg_stroke = egui::Stroke::new(1.0, pick(0x909bb3, 0x7080a0));
    w.hovered.weak_bg_fill = primary_soft();
    w.active.bg_stroke = egui::Stroke::new(1.0, primary());
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = egui::CornerRadius::same(5);
    }
    ctx.set_visuals_of(theme, v);
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
        let format = egui::TextFormat { font_id: font.clone(), color: if *caution { caution_color() } else { Color32::WHITE }, ..Default::default() };
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
            (true, _) => accent(),
            (false, true) => hex(0x44526f),
            (false, false) => primary_strong(),
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
        ButtonKind::Normal => (primary(), primary_strong()),
        ButtonKind::Execute => (execute(), execute_strong()),
        ButtonKind::Important => (danger_color(), danger_strong()),
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

/// 小窓の位置(外枠の左上)とサイズ(内側)。終了後も復元できるよう egui のメモリに保存する
fn popup_geometry_id(id: &str) -> egui::Id {
    egui::Id::new(("setting_popup_geometry", id))
}

/// 本家 setting-button のポップアップ。
/// メインウィンドウとは別の OS ウィンドウとして開き、移動・サイズ変更ができる(位置とサイズは保存される)。
pub fn popup(ctx: &egui::Context, id: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    if !is_popup_open(ctx, id) {
        return;
    }
    let saved: Option<[f32; 4]> = ctx.data_mut(|d| d.get_persisted(popup_geometry_id(id)));
    // ビルダーに渡す初期位置・サイズは開いたときの値に固定する。毎フレーム最新の保存値を渡すと、
    // 実際のウィンドウの寸法(DPI の丸め)との差で毎フレーム位置・サイズ変更を送り合い、小窓が痙攣する
    let init_id = popup_geometry_id(id).with("init");
    let init: Option<[f32; 4]> = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(init_id, || saved));
    let size = init.map_or(egui::vec2(520.0, 480.0), |g| egui::vec2(g[2], g[3]));
    let mut builder = egui::ViewportBuilder::default().with_title(title).with_inner_size(size).with_min_inner_size([300.0, 160.0]);
    if let Some(icon) = crate::icon::window_icon() {
        builder = builder.with_icon(icon);
    }
    if let Some(g) = init {
        builder = builder.with_position(egui::pos2(g[0], g[1]));
    } else if let Some(parent) = ctx.input(|i| i.viewport().outer_rect) {
        builder = builder.with_position(parent.center() - size / 2.0);
    }

    let mut contents = Some(add_contents);
    let mut close = false;
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("setting_popup", id)), builder, |ctx, class| {
        if ctx.input(|i| i.viewport().close_requested()) {
            close = true;
        }
        let Some(add_contents) = contents.take() else { return };
        let body = |ui: &mut egui::Ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::new().inner_margin(20).show(ui, add_contents);
            });
        };
        if class == egui::ViewportClass::EmbeddedWindow {
            // 別ウィンドウに対応しない環境では、従来どおりアプリ内の小窓にする
            let mut open = true;
            egui::Window::new(title).id(popup_id(id).with("win")).open(&mut open).collapsible(false).resizable(true).default_size(size).show(ctx, body);
            if !open {
                close = true;
            }
            return;
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(surface())).show(ctx, |ui| body(ui));
        // 位置とサイズを覚える
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        if let (Some(o), Some(i)) = (outer, inner)
            && o.min.x > -10000.0
            && i.width() > 0.0
        {
            let g = [o.min.x, o.min.y, i.width(), i.height()];
            let moved = saved.is_none_or(|s| s.iter().zip(&g).any(|(a, b)| (a - b).abs() > 0.5));
            if moved {
                ctx.data_mut(|d| d.insert_persisted(popup_geometry_id(id), g));
            }
        }
    });
    if close {
        ctx.data_mut(|d| {
            d.remove::<bool>(popup_id(id));
            d.remove::<Option<[f32; 4]>>(init_id);
        });
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
    ui.label(egui::RichText::new(text).color(th_color()).strong());
}

/// 本家の <small> 注記
pub fn small(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.label(egui::RichText::new(text).size(11.0).color(muted()));
    });
}

/// 本家 BaseAlert(情報)
pub fn alert(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(pick(0xedf3fb, 0x243044)).stroke(egui::Stroke::new(1.0, pick(0xbcd0ec, 0x3d5278))).corner_radius(6).inner_margin(10).show(ui, |ui| {
        ui.label(text);
    });
}

/// 本家 DangerAlert
pub fn danger(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(pick(0xfbeef0, 0x3d2a30)).stroke(egui::Stroke::new(1.0, pick(0xe8b8c1, 0x6e3f4a))).corner_radius(6).inner_margin(10).show(ui, |ui| {
        ui.label(egui::RichText::new(text).color(danger_strong()));
    });
}

/// 本家 TabList のタブの並び。選ばれたタブの位置を返す
pub fn tab_list(ui: &mut egui::Ui, labels: &[String], active: usize) -> Option<usize> {
    let mut clicked = None;
    // 下の線は全幅に引き、選ばれたタブの線をその上に描く
    let line = ui.painter().add(egui::Shape::Noop);
    let width = ui.available_width();
    let r = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, label) in labels.iter().enumerate() {
            let on = i == active;
            let text = egui::RichText::new(label).color(if on { primary_strong() } else { ink() });
            let galley = egui::WidgetText::from(if on { text.strong() } else { text }).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
            let size = galley.size() + egui::vec2(30.0, 10.0);
            let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
            let p = ui.painter();
            if on {
                p.rect_filled(rect, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, primary_soft());
            }
            p.galley(rect.center() - galley.size() / 2.0, galley, ink());
            if on {
                p.hline(rect.x_range(), rect.bottom() - 1.0, egui::Stroke::new(2.0, primary()));
            }
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(i);
            }
        }
    });
    let rect = r.response.rect;
    ui.painter().set(line, egui::Shape::hline(rect.left()..=rect.left() + width, rect.bottom() - 1.0, egui::Stroke::new(2.0, line_color())));
    clicked
}

/// 見出し付きの区切り(本家 setting-section-title)
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().color(primary_strong()));
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, line_color()));
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
    ui.label(egui::RichText::new("?").strong().color(primary())).on_hover_text(text);
}
