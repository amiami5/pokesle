//! 食材アイコン(assets/food、tools/food-icons/make.py で作成)
//!
//! ファイル名は本家の画像名(master.json の foods[].img)と同じ。ここに無い食材は名前を文字で出す。

use std::collections::HashMap;

use eframe::egui;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::BoxPokemon;

use super::style;

const FOOD_ICONS: &[(&str, &[u8])] = &[
    ("avocado.png", include_bytes!("../../assets/food/avocado.png")),
    ("beansausage.png", include_bytes!("../../assets/food/beansausage.png")),
    ("coffee.png", include_bytes!("../../assets/food/coffee.png")),
    ("fancyapple.png", include_bytes!("../../assets/food/fancyapple.png")),
    ("fancyegg.png", include_bytes!("../../assets/food/fancyegg.png")),
    ("fieryherb.png", include_bytes!("../../assets/food/fieryherb.png")),
    ("greengrasscorn.png", include_bytes!("../../assets/food/greengrasscorn.png")),
    ("greengrasssoybeans.png", include_bytes!("../../assets/food/greengrasssoybeans.png")),
    ("honey.png", include_bytes!("../../assets/food/honey.png")),
    ("largeleek.png", include_bytes!("../../assets/food/largeleek.png")),
    ("moomoomilk.png", include_bytes!("../../assets/food/moomoomilk.png")),
    ("pumpkin.png", include_bytes!("../../assets/food/pumpkin.png")),
    ("pureoil.png", include_bytes!("../../assets/food/pureoil.png")),
    ("slowpoketail.png", include_bytes!("../../assets/food/slowpoketail.png")),
    ("snoozytomato.png", include_bytes!("../../assets/food/snoozytomato.png")),
    ("softpotato.png", include_bytes!("../../assets/food/softpotato.png")),
    ("soothingcacao.png", include_bytes!("../../assets/food/soothingcacao.png")),
    ("tastymushroom.png", include_bytes!("../../assets/food/tastymushroom.png")),
    ("warmingginger.png", include_bytes!("../../assets/food/warmingginger.png")),
];

/// 食材名からアイコンのテクスチャを得る(初回に読み込んで egui のメモリに保持する)
fn food_texture(ctx: &egui::Context, food_name: &str) -> Option<egui::TextureHandle> {
    let img = master().food(food_name)?.img.clone()?;
    let id = egui::Id::new("food_icons");
    if let Some(t) = ctx.data(|d| d.get_temp::<HashMap<String, Option<egui::TextureHandle>>>(id).and_then(|m| m.get(&img).cloned())) {
        return t;
    }
    let texture = FOOD_ICONS.iter().find(|(name, _)| *name == img).and_then(|(_, bytes)| {
        let rgba = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        Some(ctx.load_texture(format!("food/{img}"), color, egui::TextureOptions::LINEAR))
    });
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<String, Option<egui::TextureHandle>>>(id).insert(img, texture.clone()));
    texture
}

/// 食材1つをアイコン(なければ名前)で表示する
pub fn food_icon(ui: &mut egui::Ui, food_name: &str, size: f32) -> egui::Response {
    match food_texture(ui.ctx(), food_name) {
        Some(t) => ui.add(egui::Image::new((t.id(), egui::vec2(size, size)))).on_hover_text(food_name),
        None => ui.label(food_name),
    }
}

/// 食材の選択肢1つ(36px の枠)。選ばれているものは明るい背景に緑の太枠、選ばれていないものは暗く沈める
pub fn food_choice(ui: &mut egui::Ui, name: &str, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    let bright = selected || r.hovered();
    let fill = if bright { egui::Color32::WHITE } else { style::pick(0xb9bfcb, 0x3a4152) };
    let stroke = if selected { egui::Stroke::new(2.0, style::mint()) } else { egui::Stroke::new(1.0, style::line_color()) };
    ui.painter().rect(rect, 6.0, fill, stroke, egui::StrokeKind::Inside);
    let inner = egui::Rect::from_center_size(rect.center(), egui::vec2(29.0, 29.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        if !bright {
            ui.multiply_opacity(0.55);
        }
        food_icon(ui, name, 29.0);
    });
    r.on_hover_text(name)
}

/// ボックスのポケモンの食材3枠をアイコンで表示する(本家 index.vue の foodNameList)。
/// 右下にその枠で採れる個数を出し、Lv が足りず解放されていない枠は薄く表示する。
pub fn food_list(ui: &mut egui::Ui, pokemon: &BoxPokemon) {
    let m = master();
    let base = m.pokemon(&pokemon.name);
    // 本家 pokemon-simulator.ts の foodUnlock
    let unlock = if pokemon.lv >= 60.0 { 3 } else if pokemon.lv >= 30.0 { 2 } else { 1 };
    let size = (ui.available_height() - 2.0).clamp(16.0, 24.0);
    ui.spacing_mut().item_spacing.x = 2.0;
    for (slot, name) in pokemon.food_list.iter().enumerate() {
        let num = base.and_then(|b| b.food_list.iter().find(|f| &f.name == name)).and_then(|f| f.num_list.get(slot).copied().flatten());
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let painter = ui.painter();
        let alpha = if slot < unlock { 1.0 } else { 0.35 };
        if num.is_none() {
            // この枠では採れない食材(本家の error 表示)
            painter.rect_filled(rect, 3.0, egui::Color32::from_rgba_unmultiplied(220, 40, 40, 160));
        }
        match food_texture(ui.ctx(), name) {
            Some(t) => {
                let tint = egui::Color32::WHITE.gamma_multiply(alpha);
                painter.image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
            }
            None => {
                let text: String = name.chars().take(2).collect();
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(size * 0.45), ui.visuals().text_color().gamma_multiply(alpha));
            }
        }
        if let Some(n) = num {
            // 右下に個数(縁取りして読みやすく)
            let font = egui::FontId::proportional(size * 0.5);
            let pos = rect.right_bottom() + egui::vec2(0.0, 1.0);
            let text = format!("{n}");
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                painter.text(pos + egui::vec2(dx, dy), egui::Align2::RIGHT_BOTTOM, &text, font.clone(), egui::Color32::BLACK.gamma_multiply(alpha));
            }
            painter.text(pos, egui::Align2::RIGHT_BOTTOM, &text, font, egui::Color32::WHITE.gamma_multiply(alpha));
        }
        let hover = match num {
            Some(n) if slot < unlock => format!("{name} ×{n}"),
            Some(n) => format!("{name} ×{n}(Lv{}で解放)", [1, 30, 60][slot.min(2)]),
            None => format!("{name}(この枠では採れません)"),
        };
        response.on_hover_text(hover);
    }
}

/// きのみ1つの絵(タイプ色の実に葉をつけた自作の簡易アイコン。ゲームの画像は使わない)
pub fn berry_icon(ui: &mut egui::Ui, berry: &pokesle_core::data::Berry, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let hex = berry.type_color.strip_prefix('#').and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0x888888);
    let base = egui::Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    let edge = egui::Color32::from_rgb(base.r() / 2, base.g() / 2, base.b() / 2);
    let p = ui.painter();
    let center = rect.center() + egui::vec2(0.0, size * 0.06);
    let r = size * 0.38;
    p.circle(center, r, base, egui::Stroke::new(1.2, edge));
    p.circle_filled(center + egui::vec2(-r * 0.35, -r * 0.35), r * 0.22, egui::Color32::from_white_alpha(150));
    // 葉
    let top = center + egui::vec2(0.0, -r);
    let leaf = egui::Color32::from_rgb(0x4c, 0xaf, 0x50);
    let pts = vec![top, top + egui::vec2(size * 0.26, -size * 0.04), top + egui::vec2(size * 0.08, -size * 0.2)];
    p.add(egui::Shape::convex_polygon(pts, leaf, egui::Stroke::new(1.0, egui::Color32::from_rgb(0x2e, 0x7d, 0x32))));
    response.on_hover_text(&berry.name)
}
