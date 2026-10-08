//! ボックスのポケモンの追加・編集ウィンドウ

use eframe::egui;
use pokesle_core::box_io;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::BoxPokemon;

use super::combo;

pub struct PokemonEdit {
    /// 編集中のボックス位置(None は新規)
    pub index: Option<usize>,
    pub pokemon: BoxPokemon,
    search: String,
    error: String,
}

pub enum EditResult {
    None,
    Save(Option<usize>, BoxPokemon),
    Cancel,
}

impl PokemonEdit {
    pub fn new(index: Option<usize>, pokemon: Option<BoxPokemon>) -> PokemonEdit {
        let m = master();
        let pokemon = pokemon.unwrap_or_else(|| {
            let base = &m.pokemon[0];
            BoxPokemon {
                name: base.name.clone(),
                lv: 10.0,
                food_list: vec![base.food_name_list[0].clone(); 3],
                sub_skill_list: m.sub_skills.iter().take(5).map(|s| s.name.clone()).collect(),
                nature: "まじめ".into(),
                ..Default::default()
            }
        });
        PokemonEdit { index, search: pokemon.name.clone(), pokemon, error: String::new() }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> EditResult {
        let m = master();
        let mut result = EditResult::None;
        let mut open = true;
        let title = if self.index.is_some() { "ポケモンを編集" } else { "ポケモンを追加" };
        egui::Window::new(title).open(&mut open).collapsible(false).resizable(true).default_width(520.0).show(ctx, |ui| {
            egui::Grid::new("edit_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                // ポケモン名(日本語入力での検索)
                ui.label("ポケモン");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("名前で検索").desired_width(240.0));
                    let query = self.search.trim();
                    if !query.is_empty() && query != self.pokemon.name {
                        let hits: Vec<_> = m.pokemon.iter().filter(|p| p.name.contains(query)).take(12).collect();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            if hits.is_empty() {
                                ui.label("該当なし");
                            }
                            for p in hits {
                                if ui.selectable_label(false, format!("{} ({}・{})", p.name, p.specialty.name(), m.pokemon_skill(p).name)).clicked() {
                                    self.set_pokemon(&p.name);
                                }
                            }
                        });
                    }
                });
                ui.end_row();

                let Some(base) = m.pokemon(&self.pokemon.name) else { return };
                ui.label("");
                ui.label(format!("{}とくい / {} / {}", base.specialty.name(), m.pokemon_skill(base).name, base.type_name));
                ui.end_row();

                ui.label("レベル");
                ui.add(egui::DragValue::new(&mut self.pokemon.lv).range(1.0..=100.0).speed(0.2));
                ui.end_row();

                ui.label("スキルレベル");
                ui.horizontal(|ui| {
                    let mut specified = self.pokemon.skill_lv.is_some();
                    if ui.checkbox(&mut specified, "指定する").changed() {
                        self.pokemon.skill_lv = specified.then_some(base.evolve_lv as f64);
                    }
                    if let Some(lv) = &mut self.pokemon.skill_lv {
                        ui.add(egui::DragValue::new(lv).range(1.0..=m.pokemon_skill(base).max_lv() as f64));
                    }
                });
                ui.end_row();

                ui.label("食材");
                ui.horizontal(|ui| {
                    self.pokemon.food_list.resize(3, base.food_name_list[0].clone());
                    for slot in 0..3 {
                        // 1枠目はA、2枠目はAかB、3枠目はA～C(4種以上のポケモンは全て)
                        let options: Vec<String> = base
                            .food_name_list
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| base.food_name_list.len() > 3 || *i <= slot)
                            .map(|(_, n)| n.clone())
                            .collect();
                        combo(ui, ("food", slot), &mut self.pokemon.food_list[slot], options, 120.0);
                    }
                });
                ui.end_row();

                ui.label("サブスキル");
                ui.vertical(|ui| {
                    self.pokemon.sub_skill_list.resize(5, String::new());
                    for (slot, lv) in [10, 25, 50, 70, 80].iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Lv{lv}"));
                            let options = m.sub_skills.iter().map(|s| s.name.clone());
                            combo(ui, ("sub", slot), &mut self.pokemon.sub_skill_list[slot], options, 200.0);
                        });
                    }
                });
                ui.end_row();

                ui.label("せいかく");
                let natures = m.natures.iter().map(|n| n.name.clone());
                combo(ui, "nature", &mut self.pokemon.nature, natures, 160.0);
                ui.end_row();

                ui.label("睡眠時間");
                ui.horizontal(|ui| {
                    let mut v = self.pokemon.sleep_time.unwrap_or(0.0);
                    if ui.add(egui::DragValue::new(&mut v).range(0.0..=99999.0).suffix(" 時間")).changed() {
                        self.pokemon.sleep_time = (v > 0.0).then_some(v);
                    }
                });
                ui.end_row();

                ui.label("固定");
                let mut fixed = self.pokemon.fix == Some(1.0);
                if ui.checkbox(&mut fixed, "チーム編成で必ず入れる").changed() {
                    self.pokemon.fix = fixed.then_some(1.0);
                }
                ui.end_row();

                ui.label("メモ");
                let mut memo = self.pokemon.memo.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut memo).changed() {
                    self.pokemon.memo = (!memo.is_empty()).then_some(memo);
                }
                ui.end_row();
            });

            if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    match box_io::check(&self.pokemon) {
                        Ok(()) => result = EditResult::Save(self.index, self.pokemon.clone()),
                        Err(e) => self.error = e,
                    }
                }
                if ui.button("キャンセル").clicked() {
                    result = EditResult::Cancel;
                }
            });
        });
        if !open {
            return EditResult::Cancel;
        }
        result
    }

    fn set_pokemon(&mut self, name: &str) {
        let m = master();
        let Some(base) = m.pokemon(name) else { return };
        self.pokemon.name = name.to_string();
        self.search = name.to_string();
        // 食材をこのポケモンの候補に合わせる
        for (slot, f) in self.pokemon.food_list.iter_mut().enumerate() {
            let allowed = base.food_name_list.iter().enumerate().any(|(i, n)| n == f && (base.food_name_list.len() > 3 || i <= slot));
            if !allowed {
                *f = base.food_name_list[0].clone();
            }
        }
        if let Some(lv) = self.pokemon.skill_lv {
            self.pokemon.skill_lv = Some(lv.min(m.pokemon_skill(base).max_lv() as f64));
        }
    }
}
