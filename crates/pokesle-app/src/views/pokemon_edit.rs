//! ボックスのポケモンの追加・編集ウィンドウ

use eframe::egui;
use pokesle_core::assist;
use pokesle_core::box_io;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::BoxPokemon;

use pokesle_core::box_sim::BoxResult;
use pokesle_core::config::Config;
use pokesle_core::evaluate_table::{EvalLv, EvaluateTable};
use pokesle_core::exp::calc_require_info;
use pokesle_core::team_sim::simulate_box;

use super::{combo, fmt_fixed, food_icon, style};
use crate::job::Job;

/// 厳選情報・アメ情報の計算に使う、開いたときの設定・ボックス・厳選テーブル
pub struct EditEnv {
    pub config: Config,
    pub box_list: Vec<BoxPokemon>,
    pub table: Option<EvaluateTable>,
}

pub struct PokemonEdit {
    /// 編集中のボックス位置(None は新規)
    pub index: Option<usize>,
    pub pokemon: BoxPokemon,
    /// 保存先のNo(1始まり。本家の「挿入先」)。None なら今の位置・末尾
    pub insert_to: Option<usize>,
    search: String,
    error: String,
    env: Option<EditEnv>,
    /// ボックス内比較用の概算(編集中の個体と進化先が重なるポケモン)
    box_job: Option<Job<Vec<BoxResult>>>,
    box_results: Option<Vec<BoxResult>>,
    box_key: Option<String>,
    /// 編集中の個体の概算(チェックリスト該当つき)
    own: Option<(String, Option<BoxResult>)>,
    graph: bool,
    show_secondary: bool,
    /// 所持アメの編集値(種ポケのアメ名, 個数)。保存後に設定へ反映してもらう
    candy_bag: Option<(String, f64)>,
    /// 入力アシストのテキスト(本家 assistText)
    assist_text: String,
    assist_focused: bool,
    /// Enter で保存した(続けて新規登録する)
    pub continue_next: bool,
}

pub enum EditResult {
    None,
    Save(Option<usize>, BoxPokemon),
    Cancel,
}

impl PokemonEdit {
    pub fn new(index: Option<usize>, pokemon: Option<BoxPokemon>) -> PokemonEdit {
        let m = master();
        let is_initial = pokemon.is_some();
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
        let assist_text = if index.is_some() || is_initial { assist::to_text(&pokemon) } else { String::new() };
        PokemonEdit {
            index,
            search: pokemon.name.clone(),
            assist_text,
            assist_focused: false,
            continue_next: false,
            pokemon,
            insert_to: None,
            error: String::new(),
            env: None,
            box_job: None,
            box_results: None,
            box_key: None,
            own: None,
            graph: true,
            show_secondary: false,
            candy_bag: None,
        }
    }

    /// 厳選情報・アメ情報を出せるようにする
    pub fn with_env(mut self, env: EditEnv) -> Self {
        let pe = env.config.extra.get("pokemonEdit");
        self.graph = pe.and_then(|v| v.get("selectDisplayMode")).and_then(|v| v.as_str()).is_none_or(|s| s != "table");
        self.show_secondary = pe.and_then(|v| v.get("showSecondarySelectMetrics")).and_then(|v| v.as_bool()).unwrap_or(false);
        self.env = Some(env);
        self
    }

    /// 所持アメを編集していたら (アメ名, 個数)
    pub fn candy_change(&self) -> Option<(String, f64)> {
        self.candy_bag.clone()
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> EditResult {
        let m = master();
        let mut result = EditResult::None;
        let mut open = true;
        let title = if self.index.is_some() { "ポケモンを編集" } else { "ポケモンを追加" };
        // 追加と編集でタイトルが違っても同じ位置を使う
        egui::Window::new(title).id(egui::Id::new("pokemon_edit")).open(&mut open).collapsible(false).resizable(true).default_width(520.0).show(ctx, |ui| {
            if let Some(r) = self.assist_area(ui) {
                result = r;
            }
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

                ui.label("挿入先No");
                ui.horizontal(|ui| {
                    let mut v = self.insert_to.unwrap_or(0);
                    if ui.add(egui::DragValue::new(&mut v).range(0..=100000).speed(0.2)).changed() {
                        self.insert_to = (v > 0).then_some(v);
                    }
                    ui.label(egui::RichText::new("0 は指定なし").size(11.0).color(style::muted()));
                });
                ui.end_row();

                ui.label("メモ");
                let mut memo = self.pokemon.memo.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut memo).changed() {
                    self.pokemon.memo = (!memo.is_empty()).then_some(memo);
                }
                ui.end_row();
            });

            self.extra_sections(ui, ctx);

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

    /// 入力アシスト欄(本家 pokemon-edit-popup.vue)。Enter で保存して続けて登録、ESC で全てクリア、Alt+C で色違い
    fn assist_area(&mut self, ui: &mut egui::Ui) -> Option<EditResult> {
        const HINT: &str = "入力例)
fushigidane
10
aab
otebo
kinos
sukim
shokus
shojil
nonki

せいかくを入力してEnterを押すまでに、以下を入力するとその他の設定も可能です
shiny / irochi(色違い)
favorite / okiniiri(お気に入り)
s:5 (スキルレベル)";
        style::alert(
            ui,
            "入力アシスト欄に、名前・Lv・食材・サブスキル・せいかくを1行ずつ入力することでも入力できます。
ローマ字でも反応します。必須項目がそろった状態でEnterを押すと、保存して続けて登録します。
Shift+Enter:改行、ESC:全てクリア、Alt+C:色違い",
        );
        ui.add_space(6.0);
        ui.label(egui::RichText::new("入力アシスト").strong().color(style::th_color()));
        let id = egui::Id::new("pokemon_assist_input");
        if !self.assist_focused {
            ui.memory_mut(|m| m.request_focus(id));
            self.assist_focused = true;
        }
        let focused = ui.memory(|m| m.has_focus(id));
        let mut result = None;
        let mut text_changed = false;
        if focused {
            let (enter, esc, alt_c) = ui.input_mut(|i| {
                let plain = !i.modifiers.shift && !i.modifiers.alt;
                (
                    plain && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                    i.consume_key(egui::Modifiers::ALT, egui::Key::C),
                )
            });
            if alt_c {
                self.pokemon.shiny = !self.pokemon.shiny;
                self.assist_text = assist::to_text(&self.pokemon);
            }
            if esc {
                if self.assist_text.is_empty() {
                    return Some(EditResult::Cancel);
                }
                self.reset();
            }
            if enter {
                match box_io::check(&self.pokemon) {
                    Ok(()) => {
                        self.continue_next = self.index.is_none();
                        result = Some(EditResult::Save(self.index, self.pokemon.clone()));
                    }
                    Err(e) => self.error = e,
                }
            }
        }
        let response = ui.add(egui::TextEdit::multiline(&mut self.assist_text).id(id).desired_rows(5).desired_width(f32::INFINITY).hint_text(HINT));
        text_changed |= response.changed();
        if text_changed {
            assist::infer(&self.assist_text, &mut self.pokemon);
            self.search = self.pokemon.name.clone();
            self.error.clear();
        }
        ui.add_space(8.0);
        result
    }

    /// 本家 reset: 入力を全てクリアする
    fn reset(&mut self) {
        let m = master();
        let base = &m.pokemon[0];
        self.pokemon = BoxPokemon {
            name: base.name.clone(),
            lv: 10.0,
            food_list: vec![base.food_name_list[0].clone(); 3],
            sub_skill_list: m.sub_skills.iter().take(5).map(|s| s.name.clone()).collect(),
            nature: "まじめ".into(),
            ..Default::default()
        };
        self.search = self.pokemon.name.clone();
        self.assist_text.clear();
        self.insert_to = None;
        self.error.clear();
    }

    /// 厳選情報とアメ情報(本家 pokemon-edit-popup.vue の ToggleArea)
    fn extra_sections(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(env) = self.env.take() else { return };
        ui.add_space(6.0);
        egui::CollapsingHeader::new("厳選情報(ボックス内比較)").id_salt("edit_select_info").default_open(true).show(ui, |ui| {
            self.select_section(ui, ctx, &env);
        });
        egui::CollapsingHeader::new("アメ＆EXP情報").id_salt("edit_candy_info").default_open(false).show(ui, |ui| {
            self.candy_section(ui, &env);
        });
        self.env = Some(env);
    }

    fn select_section(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, env: &EditEnv) {
        let m = master();
        let Some(base) = m.pokemon(&self.pokemon.name) else { return };
        if base.kaihou {
            ui.label("このポケモンは厳選情報を表示できません");
            return;
        }
        let Some(table) = env.table.as_ref().filter(|t| t.is_enable(&env.config)) else {
            ui.label("厳選テーブルがありません(基準生成で作成してください)");
            return;
        };
        if box_io::check(&self.pokemon).is_err() {
            ui.label("せいかくまで入力すると表示されます");
            return;
        }

        // ボックス内の比較用概算: 進化先が重なるポケモンだけを、ポケモン(進化先)が変わったときに計算する
        let key = base.after_list.join("/");
        if self.box_key.as_deref() != Some(key.as_str()) && self.box_job.is_none() {
            self.box_key = Some(key);
            self.box_results = None;
            let list: Vec<BoxPokemon> = env
                .box_list
                .iter()
                .filter(|b| m.pokemon(&b.name).is_some_and(|p| p.after_list.iter().any(|a| base.after_list.contains(a))))
                .cloned()
                .collect();
            let config = env.config.clone();
            let table = table.clone();
            self.box_job = Some(Job::spawn(ctx, move |_| simulate_box(&config, &list, Some(&table), false)));
        }
        if let Some(job) = &mut self.box_job
            && let Some(r) = job.poll()
        {
            self.box_job = None;
            self.box_results = r.ok();
        }
        if let Some(job) = &self.box_job {
            job.show_progress(ui);
            return;
        }

        // 編集中の個体の概算(ボックス整理と同じチェックリスト判定をする)
        let own_key = serde_json::to_string(&self.pokemon).unwrap_or_default();
        if self.own.as_ref().is_none_or(|(k, _)| *k != own_key) {
            let mut c = env.config.clone();
            c.cleaning = true;
            let r = simulate_box(&c, std::slice::from_ref(&self.pokemon), Some(table), false).ok().and_then(|v| v.into_iter().next());
            self.own = Some((own_key, r));
        }
        let Some((_, Some(own))) = &self.own else {
            ui.label("評価できませんでした");
            return;
        };
        let Some(er) = own.evaluate_result.as_ref() else {
            ui.label("評価できませんでした");
            return;
        };

        // チェックリスト
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("チェックリスト").strong());
            if own.hit_check_list.is_empty() {
                ui.label("該当なし");
            }
            for hit in &own.hit_check_list {
                use pokesle_core::check_list::CheckHit;
                ui.label("✅");
                match hit {
                    CheckHit::Food(i) => {
                        food_icon::food_icon(ui, &m.foods[*i].name, 20.0);
                    }
                    other => {
                        ui.label(other.label());
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            ui.radio_value(&mut self.graph, true, "グラフ");
            ui.radio_value(&mut self.graph, false, "表");
            ui.checkbox(&mut self.show_secondary, "その他の厳選度も表示する");
        });

        // 列: 総合、とくい、(その他)
        let specialty = base.specialty.name();
        let mut columns: Vec<(&str, &str, egui::Color32, u8)> = vec![
            ("energy", "総合スコア", egui::Color32::from_rgb(220, 48, 50), 1),
            ("berry", "きのみ", egui::Color32::from_rgb(32, 212, 102), if specialty == "きのみ" { 2 } else { 3 }),
            ("food", "食材", egui::Color32::from_rgb(245, 183, 72), if specialty == "食材" { 2 } else { 4 }),
            ("skill", "スキル", egui::Color32::from_rgb(70, 159, 253), if specialty == "スキル" { 2 } else { 5 }),
        ];
        columns.sort_by_key(|c| c.3);
        if !self.show_secondary {
            columns.retain(|c| c.3 <= 2);
        }
        let mut lvs: Vec<EvalLv> = vec![EvalLv::Max];
        lvs.extend(env.config.select_evaluate.enabled_levels().into_iter().map(EvalLv::Lv));
        let lv_label = |lv: EvalLv| match lv {
            EvalLv::Max => "最大".to_string(),
            EvalLv::Lv(n) => format!("Lv{n}"),
        };

        // ボックス内の最高(同種族・同種族同食材)
        let box_best = |lv: EvalLv, after: &str, key: &str, same_food: bool| -> Option<f64> {
            let list = self.box_results.as_ref()?;
            list.iter()
                .filter(|r| !same_food || r.pokemon.box_pokemon.as_ref().is_some_and(|b| b.food_list.iter().enumerate().all(|(i, f)| self.pokemon.food_list.get(i) == Some(f))))
                .filter_map(|r| r.evaluate_result.as_ref()?.get(lv, after)?.get(key).map(|i| i.score))
                .filter(|s| !s.is_nan())
                .fold(None, |acc: Option<f64>, s| Some(acc.map_or(s, |a| a.max(s))))
        };
        let own_score = |lv: EvalLv, after: &str, key: &str| -> Option<f64> { er.get(lv, after)?.get(key).map(|i| i.score).filter(|s| !s.is_nan()) };
        let pct = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{}%", fmt_fixed(v * 100.0, 1)));

        if self.graph {
            ui.label(egui::RichText::new("本個体と、ボックス内の同種族・同種族同食材の最高厳選度を比較します。").size(11.0).color(style::muted()));
            for after in &base.after_list {
                ui.label(egui::RichText::new(format!("進化先：{after}")).strong());
                ui.horizontal_wrapped(|ui| {
                    for (key, name, color, _) in &columns {
                        let series: [Vec<Option<f64>>; 3] = [
                            lvs.iter().map(|&lv| own_score(lv, after, key)).collect(),
                            lvs.iter().map(|&lv| box_best(lv, after, key, false)).collect(),
                            lvs.iter().map(|&lv| box_best(lv, after, key, true)).collect(),
                        ];
                        ui.vertical(|ui| {
                            ui.label(*name);
                            let labels: Vec<String> = lvs.iter().map(|&lv| lv_label(lv)).collect();
                            mini_chart(ui, &labels, &series, *color);
                        });
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("━ 本個体");
                ui.label("┅ 同種族1位");
                ui.label("┈ 同種族・同食材1位");
            });
        } else {
            egui::ScrollArea::horizontal().id_salt("edit_select_table").show(ui, |ui| {
                egui::Grid::new("edit_select_grid").striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                    ui.label("");
                    ui.label("");
                    for (_, name, ..) in &columns {
                        ui.label(egui::RichText::new(*name).strong());
                        for _ in 1..lvs.len() {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                    ui.label(egui::RichText::new("最終進化").strong());
                    ui.label("");
                    for _ in &columns {
                        for &lv in &lvs {
                            ui.label(egui::RichText::new(lv_label(lv)).strong());
                        }
                    }
                    ui.end_row();
                    for after in &base.after_list {
                        for (row, title) in ["本個体", "同種族", "同種族・同食材"].iter().enumerate() {
                            ui.label(if row == 0 { after.as_str() } else { "" });
                            ui.label(*title);
                            for (key, ..) in &columns {
                                for &lv in &lvs {
                                    let v = match row {
                                        0 => own_score(lv, after, key),
                                        1 => box_best(lv, after, key, false),
                                        _ => box_best(lv, after, key, true),
                                    };
                                    let best = row == 0
                                        && v.is_some()
                                        && er.best.get(&lv).and_then(|e| e.get(key)).map(|i| i.score) == v;
                                    let mut t = egui::RichText::new(pct(v));
                                    if best {
                                        t = t.strong().color(egui::Color32::from_rgb(0xd0, 0x30, 0x30));
                                    }
                                    ui.label(t);
                                }
                            }
                            ui.end_row();
                        }
                    }
                });
            });
        }
    }

    fn candy_section(&mut self, ui: &mut egui::Ui, env: &EditEnv) {
        let m = master();
        egui::Grid::new("edit_candy_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("次のレベルまでのEXP");
            let mut v = self.pokemon.next_exp.unwrap_or(0.0);
            if ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0e7)).changed() {
                self.pokemon.next_exp = (v > 0.0).then_some(v);
            }
            ui.end_row();
            ui.label("目標レベル");
            ui.horizontal(|ui| {
                let mut v = self.pokemon.training.unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0)).changed() {
                    self.pokemon.training = (v > 0.0).then_some(v);
                }
                ui.label("Lv");
            });
            ui.end_row();
            ui.label("所持アメ");
            ui.horizontal(|ui| {
                if let Some(base) = m.pokemon(&self.pokemon.name) {
                    let seed = base.seed.clone().unwrap_or_else(|| base.candy_name.clone());
                    let current = self.candy_bag.as_ref().filter(|(n, _)| *n == seed).map(|(_, v)| *v).or_else(|| env.config.candy.bag.get(&seed).copied()).unwrap_or(0.0);
                    let mut v = current;
                    if ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0e7)).changed() {
                        self.candy_bag = Some((seed, v));
                    }
                }
                ui.label("個");
            });
            ui.end_row();
        });

        // 必要アメ数(編集中の個体の目標レベル・EXP で計算)
        let mut candy = env.config.candy.clone();
        if let (Some(base), Some((name, v))) = (m.pokemon(&self.pokemon.name), &self.candy_bag) {
            let seed = base.seed.clone().unwrap_or_else(|| base.candy_name.clone());
            if *name == seed {
                candy.bag.insert(seed, *v);
            }
        }
        let nature = m.nature(&self.pokemon.nature);
        let r = calc_require_info(&self.pokemon, nature, &candy);
        let sum = |a: Option<f64>, b: Option<f64>| a.map(|a| a + b.unwrap_or(0.0));
        let rows = [
            ("通常時".to_string(), r.normal_candy_num, r.normal_candy_shard),
            (format!("アメブ(EXPx{}/ゆめかけx{})", candy.boost_multiply, candy.boost_shard), r.boost_candy_num, r.boost_candy_shard),
            ("バランス(通常分)".to_string(), r.best_normal_candy_num, r.best_normal_candy_shard),
            ("バランス(アメブ分)".to_string(), r.best_boost_candy_num, r.best_boost_candy_shard),
            ("バランス(合計)".to_string(), sum(r.best_normal_candy_num, r.best_boost_candy_num), sum(r.best_normal_candy_shard, r.best_boost_candy_shard)),
        ];
        let show = |v: Option<f64>| v.map_or(String::new(), |v| super::fmt_num(v));
        ui.add_space(4.0);
        egui::Grid::new("edit_candy_table").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            style::th(ui, "種類");
            style::th(ui, "必要アメ数");
            style::th(ui, "必要ゆめのかけら");
            ui.end_row();
            for (name, num, shard) in rows {
                ui.label(name);
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| ui.label(show(num)));
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| ui.label(show(shard)));
                ui.end_row();
            }
        });
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


/// 厳選度の折れ線グラフ(縦軸は 0〜(最大を20刻みに切り上げた値)%)。系列は 本個体・同種族1位・同種族同食材1位
fn mini_chart(ui: &mut egui::Ui, labels: &[String], series: &[Vec<Option<f64>>; 3], color: egui::Color32) {
    let size = egui::vec2(300.0, 150.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(36.0, 6.0), rect.max - egui::vec2(6.0, 20.0));
    let max_score = series.iter().flatten().flatten().copied().filter(|v| v.is_finite()).fold(0.0_f64, f64::max) * 100.0;
    let axis_max = ((max_score / 20.0).ceil() * 20.0).max(20.0);
    let muted = style::muted();
    let font = egui::FontId::proportional(10.0);
    let y_of = |v: f64| plot.bottom() - (v / axis_max) as f32 * plot.height();
    let mut k = 0.0;
    while k <= axis_max + 0.1 {
        let y = y_of(k);
        painter.hline(plot.x_range(), y, egui::Stroke::new(1.0, style::line_color()));
        painter.text(egui::pos2(plot.left() - 3.0, y), egui::Align2::RIGHT_CENTER, format!("{k:.0}"), font.clone(), muted);
        k += 20.0;
    }
    let n = labels.len().max(1);
    let x_of = |i: usize| plot.left() + plot.width() * (i as f32 + 0.5) / n as f32;
    for (i, l) in labels.iter().enumerate() {
        painter.text(egui::pos2(x_of(i), plot.bottom() + 3.0), egui::Align2::CENTER_TOP, l, font.clone(), muted);
    }
    let styles = [(color, 3.0), (egui::Color32::from_gray(0x55), 2.0), (egui::Color32::from_gray(0x99), 2.0)];
    for (data, (c, w)) in series.iter().zip(styles) {
        let mut run: Vec<egui::Pos2> = Vec::new();
        let flush = |run: &mut Vec<egui::Pos2>| {
            if run.len() > 1 {
                painter.add(egui::Shape::line(std::mem::take(run), egui::Stroke::new(w, c)));
            }
            run.clear();
        };
        for (i, v) in data.iter().enumerate() {
            match v {
                Some(v) if v.is_finite() => {
                    let p = egui::pos2(x_of(i), y_of(*v * 100.0));
                    painter.circle_filled(p, w, c);
                    run.push(p);
                }
                _ => flush(&mut run),
            }
        }
        flush(&mut run);
    }
}
