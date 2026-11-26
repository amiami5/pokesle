//! A/B比較画面(1〜2個体の簡易診断・比較を行う)

use eframe::egui;
use pokesle_core::assist;
use pokesle_core::data::master;
use pokesle_core::sim_pokemon::BoxPokemon;
use pokesle_core::team_sim::{simulate_tmp_compare, simulate_tmp_evaluate};

use super::box_view::{keyword_match, sub_skill_chip};
use super::pokemon_edit::{EditResult, PokemonEdit};
use super::tmp_evaluate_view::{
    Evaluated, Evolution, LV_LIST, MintChartDisplay, SERIES, box_result_to_evaluated, chart, diagnostic_table, diff_info, food_options, food_requirement_rate, rate_table, reset_foods, tmp_setting_tab,
};
use super::{combo, fmt_fixed, food_icon, nature_label, style};
use crate::app::Shared;
use crate::job::Job;

const COMPARE_COL_W: [f32; 7] = [150.0, 56.0, 68.0, 68.0, 68.0, 68.0, 68.0];
const COMPARE_TABLE_W: f32 = 546.0;
const BOX_PICKER_POPUP_ID: &str = "compare_box_picker";

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerTarget {
    A,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerSort {
    IndexAsc,
    IndexDesc,
    LvDesc,
    LvAsc,
    NameAsc,
    NameDesc,
}

struct BoxPicker {
    target: PickerTarget,
    query: String,
    only_favorite: bool,
    same_tree_only: bool,
    sort: PickerSort,
    focus_search: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Compare,
    Setting,
}

#[derive(Clone)]
struct SideForm {
    pokemon: BoxPokemon,
    search: String,
    assist_text: String,
    use_mint: bool,
    selected_box_idx: Option<usize>,
}

impl SideForm {
    fn default_pokemon() -> Self {
        let m = master();
        let base = &m.pokemon[0];
        let mut p = BoxPokemon {
            name: base.name.clone(),
            lv: 1.0,
            skill_lv: Some(1.0),
            food_list: Vec::new(),
            sub_skill_list: vec![String::new(); 5],
            nature: "がんばりや".into(),
            ..Default::default()
        };
        reset_foods(&mut p);
        Self { search: p.name.clone(), pokemon: p, assist_text: String::new(), use_mint: false, selected_box_idx: None }
    }

    fn empty() -> Self {
        Self {
            pokemon: BoxPokemon {
                lv: 1.0,
                skill_lv: Some(1.0),
                food_list: vec![String::new(); 3],
                sub_skill_list: vec![String::new(); 5],
                nature: "がんばりや".into(),
                ..Default::default()
            },
            search: String::new(),
            assist_text: String::new(),
            use_mint: false,
            selected_box_idx: None,
        }
    }

    fn is_configured(&self) -> bool {
        !self.pokemon.name.is_empty() && master().pokemon(&self.pokemon.name).is_some()
    }

    fn copy_from(&mut self, other: &Self) {
        self.pokemon = other.pokemon.clone();
        self.search = other.search.clone();
        self.assist_text = other.assist_text.clone();
        self.use_mint = other.use_mint;
        self.selected_box_idx = other.selected_box_idx;
    }

    fn clear(&mut self) {
        *self = Self::empty();
    }

    fn load_box_pokemon(&mut self, idx: usize, bp: &BoxPokemon) {
        self.pokemon = bp.clone();
        self.search = bp.name.clone();
        self.assist_text = assist::to_text(&self.pokemon);
        self.selected_box_idx = Some(idx);
        if !master().nature(&self.pokemon.nature).is_some_and(|n| n.good.is_some()) {
            self.use_mint = false;
        }
    }
}

enum ViewResult {
    Single {
        tag: &'static str,
        eval: Evaluated,
    },
    Compare {
        a: Evaluated,
        b: Evaluated,
        evo_a_idx: usize,
        evo_b_idx: usize,
    },
}

pub struct CompareView {
    tab: Tab,
    form_a: SideForm,
    form_b: SideForm,
    job: Option<Job<ViewResult>>,
    result: Option<ViewResult>,
    chart_series: usize,
    error: String,
    edit: Option<PokemonEdit>,
    box_picker: Option<BoxPicker>,
}

impl Default for CompareView {
    fn default() -> Self {
        Self {
            tab: Tab::Compare,
            form_a: SideForm::default_pokemon(),
            form_b: SideForm::empty(),
            job: None,
            result: None,
            chart_series: 0,
            error: String::new(),
            edit: None,
            box_picker: None,
        }
    }
}

impl CompareView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(res) => self.result = Some(res),
                Err(e) => self.error = e,
            }
        }

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(1060.0);
            style::alert(
                ui,
                "個体A・個体Bを設定して、Lv30〜80の厳選度や実際の値（きのみ・食材・スキル）を診断・比較します。\n片方だけ入力した場合は単体診断、両方入力した場合はA/B比較の結果が表示されます。",
            );
            ui.add_space(6.0);
            let labels = vec!["A/B比較".to_string(), "設定".to_string()];
            if let Some(i) = style::tab_list(ui, &labels, if self.tab == Tab::Compare { 0 } else { 1 }) {
                self.tab = if i == 0 { Tab::Compare } else { Tab::Setting };
            }
            ui.add_space(6.0);
            match self.tab {
                Tab::Compare => self.compare_tab(ui, shared),
                Tab::Setting => tmp_setting_tab(ui, shared),
            }
        });

        if let Some(edit) = &mut self.edit {
            match edit.ui(ui.ctx()) {
                EditResult::None => {}
                EditResult::Cancel => self.edit = None,
                EditResult::Save(_, pokemon) => {
                    shared.box_file.list.push(*pokemon);
                    shared.save_box();
                    self.edit = None;
                }
            }
        }

        self.box_picker_window(ui.ctx(), shared);
    }

    fn open_box_picker(&mut self, ctx: &egui::Context, target: PickerTarget) {
        self.box_picker = Some(BoxPicker {
            target,
            query: String::new(),
            only_favorite: false,
            same_tree_only: false,
            sort: PickerSort::LvDesc,
            focus_search: true,
        });
        style::open_popup(ctx, BOX_PICKER_POPUP_ID);
    }

    fn box_picker_window(&mut self, ctx: &egui::Context, shared: &Shared) {
        if self.box_picker.is_none() {
            return;
        }
        if !style::is_popup_open(ctx, BOX_PICKER_POPUP_ID) {
            self.box_picker = None;
            return;
        }
        let target = self.box_picker.as_ref().expect("checked").target;
        let title = match target {
            PickerTarget::A => "個体Aをボックスから選択",
            PickerTarget::B => "個体Bをボックスから選択",
        };
        let ref_form = match target {
            PickerTarget::A => if self.form_b.is_configured() { &self.form_b } else { &self.form_a },
            PickerTarget::B => if self.form_a.is_configured() { &self.form_a } else { &self.form_b },
        };
        let ref_name = ref_form.is_configured().then_some(ref_form.pokemon.name.as_str());
        let ref_seed = ref_name.and_then(seed_key);
        let selected_idx = match target {
            PickerTarget::A => self.form_a.selected_box_idx,
            PickerTarget::B => self.form_b.selected_box_idx,
        };

        let mut chosen: Option<usize> = None;
        let mut close = false;
        let box_list = &shared.box_file.list;

        style::popup_sized(ctx, BOX_PICKER_POPUP_ID, title, egui::vec2(920.0, 580.0), |ui| {
            let Some(picker) = &mut self.box_picker else { return };
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
                return;
            }

            let mut enter_pressed = false;
            ui.horizontal_wrapped(|ui| {
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut picker.query)
                        .hint_text("検索 (名前・メモ・食材(AAA等)・サブスキル・せいかく / 空白:AND, |:OR)")
                        .desired_width(360.0),
                );
                if picker.focus_search {
                    search_resp.request_focus();
                    picker.focus_search = false;
                }
                if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    enter_pressed = true;
                }
                if !picker.query.is_empty() && ui.small_button("×").on_hover_text("検索をクリア").clicked() {
                    picker.query.clear();
                }
                ui.checkbox(&mut picker.only_favorite, "★のみ");
                if let (Some(name), Some(_)) = (ref_name, ref_seed) {
                    ui.checkbox(&mut picker.same_tree_only, format!("同系統のみ ({name})"));
                }
            });
            ui.add_space(4.0);

            let query = picker.query.trim();
            let mut filtered: Vec<usize> = box_list
                .iter()
                .enumerate()
                .filter(|(_, bp)| {
                    if picker.only_favorite && !bp.favorite {
                        return false;
                    }
                    if picker.same_tree_only
                        && let Some(seed) = ref_seed
                        && seed_key(&bp.name) != Some(seed)
                    {
                        return false;
                    }
                    query.is_empty() || keyword_match(bp, query)
                })
                .map(|(i, _)| i)
                .collect();

            filtered.sort_by(|&a, &b| {
                let pa = &box_list[a];
                let pb = &box_list[b];
                match picker.sort {
                    PickerSort::IndexAsc => pa.index.cmp(&pb.index),
                    PickerSort::IndexDesc => pb.index.cmp(&pa.index),
                    PickerSort::LvDesc => pb.lv.total_cmp(&pa.lv).then_with(|| pa.index.cmp(&pb.index)),
                    PickerSort::LvAsc => pa.lv.total_cmp(&pb.lv).then_with(|| pa.index.cmp(&pb.index)),
                    PickerSort::NameAsc => pa.name.cmp(&pb.name).then_with(|| pb.lv.total_cmp(&pa.lv)).then_with(|| pa.index.cmp(&pb.index)),
                    PickerSort::NameDesc => pb.name.cmp(&pa.name).then_with(|| pb.lv.total_cmp(&pa.lv)).then_with(|| pa.index.cmp(&pb.index)),
                }
            });

            if enter_pressed && filtered.len() == 1 {
                chosen = Some(filtered[0]);
            }

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} / {} 匹表示（行をクリックして読込）", filtered.len(), box_list.len())).color(style::muted()));
            });
            ui.add_space(4.0);

            if filtered.is_empty() {
                ui.label("条件に一致するポケモンがボックスにいません。");
                return;
            }

            // ヘッダ（「No.」「ポケモン」「Lv」のクリックでソート切替）
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let no_label = match picker.sort {
                    PickerSort::IndexAsc => "No. ▲",
                    PickerSort::IndexDesc => "No. ▼",
                    _ => "No.",
                };
                if ui
                    .add_sized([54.0, 18.0], egui::Label::new(egui::RichText::new(no_label).strong()).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("クリックでNo.順にソート")
                    .clicked()
                {
                    picker.sort = if picker.sort == PickerSort::IndexAsc { PickerSort::IndexDesc } else { PickerSort::IndexAsc };
                }
                let name_label = match picker.sort {
                    PickerSort::NameAsc => "ポケモン ▲",
                    PickerSort::NameDesc => "ポケモン ▼",
                    _ => "ポケモン",
                };
                if ui
                    .add_sized([124.0, 18.0], egui::Label::new(egui::RichText::new(name_label).strong()).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("クリックでポケモン名順にソート")
                    .clicked()
                {
                    picker.sort = if picker.sort == PickerSort::NameAsc { PickerSort::NameDesc } else { PickerSort::NameAsc };
                }
                let lv_label = match picker.sort {
                    PickerSort::LvDesc => "Lv ▼",
                    PickerSort::LvAsc => "Lv ▲",
                    _ => "Lv",
                };
                if ui
                    .add_sized([48.0, 18.0], egui::Label::new(egui::RichText::new(lv_label).strong()).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("クリックでLv順にソート")
                    .clicked()
                {
                    picker.sort = if picker.sort == PickerSort::LvDesc { PickerSort::LvAsc } else { PickerSort::LvDesc };
                }
                ui.add_sized([74.0, 18.0], egui::Label::new(egui::RichText::new("食材").strong()));
                ui.add_sized([300.0, 18.0], egui::Label::new(egui::RichText::new("サブスキル").strong()));
                ui.add_sized([108.0, 18.0], egui::Label::new(egui::RichText::new("せいかく").strong()));
                ui.label(egui::RichText::new("メモ").strong());
            });
            ui.separator();

            let row_h = 28.0;
            let list_h = (ui.available_height() - 8.0).max(240.0);
            let hover_bg = style::pick(0xeaf2ff, 0x2b3648);
            let selected_bg = style::pick(0xdce9ff, 0x324563);

            egui::ScrollArea::vertical().id_salt("compare_box_picker_rows").max_height(list_h).auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                filtered.len(),
                |ui, range| {
                    for pos in range {
                        let idx = filtered[pos];
                        let bp = &box_list[idx];
                        let is_selected = selected_idx == Some(idx);
                        let row_id = ui.make_persistent_id(("cmp_picker_row", idx));
                        let was_hovered = ui.ctx().data(|d| d.get_temp::<bool>(row_id)).unwrap_or(false);

                        let frame_fill = if is_selected {
                            selected_bg
                        } else if was_hovered {
                            hover_bg
                        } else if pos % 2 == 1 {
                            style::surface()
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let resp = egui::Frame::new().fill(frame_fill).corner_radius(3).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                            ui.set_height(row_h - 4.0);
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let no_text = if bp.favorite { format!("{} ★", bp.index + 1) } else { format!("{}", bp.index + 1) };
                                ui.add_sized([54.0, 22.0], egui::Label::new(no_text));
                                let name_text = if bp.shiny { format!("{} (色)", bp.name) } else { bp.name.clone() };
                                ui.add_sized([124.0, 22.0], egui::Label::new(egui::RichText::new(name_text).strong()).truncate());
                                ui.add_sized([48.0, 22.0], egui::Label::new(format!("Lv{}", bp.lv as u32)));
                                ui.allocate_ui_with_layout(egui::vec2(74.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    food_icon::food_list(ui, bp);
                                });
                                ui.allocate_ui_with_layout(egui::vec2(300.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    for slot in 0..5 {
                                        let s = bp.sub_skill_list.get(slot).map(|x| x.as_str()).unwrap_or("");
                                        sub_skill_chip(ui, s, slot, bp);
                                    }
                                });
                                ui.allocate_ui_with_layout(egui::vec2(108.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    nature_label(ui, &bp.nature);
                                });
                                let memo = bp.memo.as_deref().unwrap_or("");
                                if !memo.is_empty() {
                                    ui.add(egui::Label::new(memo).truncate()).on_hover_text(memo);
                                }
                            });
                        });

                        let click = ui.interact(resp.response.rect, row_id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hovered = click.hovered();
                        ui.ctx().data_mut(|d| d.insert_temp(row_id, hovered));
                        if click.clicked() {
                            chosen = Some(idx);
                        }
                    }
                },
            );
        });

        if let Some(idx) = chosen
            && let Some(bp) = shared.box_file.list.get(idx)
        {
            match target {
                PickerTarget::A => self.form_a.load_box_pokemon(idx, bp),
                PickerTarget::B => self.form_b.load_box_pokemon(idx, bp),
            }
            close = true;
        }
        if close {
            style::close_popup(ctx, BOX_PICKER_POPUP_ID);
            self.box_picker = None;
        }
    }

    fn start_run(&mut self, ctx: &egui::Context, shared: &Shared) {
        let has_a = self.form_a.is_configured();
        let has_b = self.form_b.is_configured();
        if !has_a && !has_b {
            self.error = "個体Aまたは個体Bのポケモンを選択してください".into();
            return;
        }
        self.result = None;
        self.error.clear();
        let config = shared.sim_config();
        let m = master();
        let pa = self.form_a.pokemon.clone();
        let pb = self.form_b.pokemon.clone();
        let mint_a = self.form_a.use_mint && m.nature(&pa.nature).is_some_and(|n| n.good.is_some());
        let mint_b = self.form_b.use_mint && m.nature(&pb.nature).is_some_and(|n| n.good.is_some());

        self.job = Some(Job::spawn(ctx, move |p| match (has_a, has_b) {
            (true, true) => {
                let (ra, rb) = simulate_tmp_compare(&config, &pa, mint_a, &pb, mint_b, &p)?;
                let ea = box_result_to_evaluated(pa, ra, mint_a)?;
                let eb = box_result_to_evaluated(pb, rb, mint_b)?;
                let evo_a_idx = ea.evolutions.len().saturating_sub(1);
                let evo_b_idx = eb.evolutions.len().saturating_sub(1);
                Ok(ViewResult::Compare { a: ea, b: eb, evo_a_idx, evo_b_idx })
            }
            (true, false) => {
                let ra = simulate_tmp_evaluate(&config, &pa, mint_a, &p)?;
                let ea = box_result_to_evaluated(pa, ra, mint_a)?;
                Ok(ViewResult::Single { tag: "個体A", eval: ea })
            }
            (false, true) => {
                let rb = simulate_tmp_evaluate(&config, &pb, mint_b, &p)?;
                let eb = box_result_to_evaluated(pb, rb, mint_b)?;
                Ok(ViewResult::Single { tag: "個体B", eval: eb })
            }
            (false, false) => unreachable!(),
        }));
    }

    fn compare_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        let color_a = style::pick(0x3d73c9, 0x6ea0f0);
        let color_b = style::pick(0xbf6575, 0xe87c8f);

        let mut open_picker: Option<PickerTarget> = None;
        let avail = ui.available_width();
        if avail >= 920.0 {
            let card_w = (avail - 12.0) / 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.vertical(|ui| {
                    ui.set_width(card_w);
                    if side_form_ui(ui, "cmp_a", "個体A", color_a, &mut self.form_a, &shared.box_file.list) {
                        open_picker = Some(PickerTarget::A);
                    }
                });
                ui.vertical(|ui| {
                    ui.set_width(card_w);
                    if side_form_ui(ui, "cmp_b", "個体B", color_b, &mut self.form_b, &shared.box_file.list) {
                        open_picker = Some(PickerTarget::B);
                    }
                });
            });
        } else {
            if side_form_ui(ui, "cmp_a", "個体A", color_a, &mut self.form_a, &shared.box_file.list) {
                open_picker = Some(PickerTarget::A);
            }
            ui.add_space(8.0);
            if side_form_ui(ui, "cmp_b", "個体B", color_b, &mut self.form_b, &shared.box_file.list) {
                open_picker = Some(PickerTarget::B);
            }
        }
        if let Some(target) = open_picker {
            self.open_box_picker(ui.ctx(), target);
        }
        ui.add_space(6.0);

        let running = self.job.is_some();
        let has_a = self.form_a.is_configured();
        let has_b = self.form_b.is_configured();
        let btn_label = match (has_a, has_b) {
            (true, true) => "A/B比較する",
            (true, false) => "個体Aを診断する",
            (false, true) => "個体Bを診断する",
            (false, false) => "診断 / 比較する",
        };
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal_wrapped(|ui| {
                if style::form_button(ui, btn_label, style::ButtonKind::Execute).clicked() {
                    self.start_run(ui.ctx(), shared);
                }
                ui.add_space(8.0);
                if style::form_button(ui, "A → B コピー", style::ButtonKind::Normal).clicked() {
                    self.form_b.copy_from(&self.form_a);
                }
                if style::form_button(ui, "B → A コピー", style::ButtonKind::Normal).clicked() {
                    self.form_a.copy_from(&self.form_b);
                }
                if style::form_button(ui, "A ⇄ B 入替", style::ButtonKind::Normal).clicked() {
                    std::mem::swap(&mut self.form_a, &mut self.form_b);
                }
            });
        });
        if let Some(job) = &self.job {
            job.show_progress(ui);
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }

        let Some(res) = &mut self.result else { return };
        ui.add_space(10.0);

        match res {
            ViewResult::Single { tag, eval } => {
                ui.label(egui::RichText::new(format!("{tag} の診断結果（個体Bを入力するとA/B比較ができます）")).strong());
                ui.add_space(6.0);
                let all_sp = shared.ui_setting.all_specialty;
                let has_mint = eval.has_mint;
                let display = if has_mint { MintChartDisplay::After } else { MintChartDisplay::Before };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    for evo in &eval.evolutions {
                        ui.vertical(|ui| {
                            ui.set_width(400.0);
                            ui.label(egui::RichText::new(&evo.name).size(16.0).strong());
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new("厳選度").size(13.0).strong().color(style::muted()));
                            chart(ui, evo, has_mint, display, all_sp);
                            ui.add_space(6.0);
                            rate_table(ui, evo, has_mint);
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("実際の値").size(13.0).strong().color(style::muted()));
                            diagnostic_table(ui, evo, has_mint);
                        });
                    }
                });
                ui.add_space(8.0);
                if style::form_button(ui, &format!("{tag}をボックスに追加"), style::ButtonKind::Execute).clicked() {
                    self.edit = Some(PokemonEdit::new(None, Some(eval.pokemon.clone())));
                }
            }
            ViewResult::Compare { a, b, evo_a_idx, evo_b_idx } => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("比較対象：").strong());
                    evo_selector(ui, "cmp_evo_a", "個体A", color_a, a, evo_a_idx);
                    ui.add_space(12.0);
                    evo_selector(ui, "cmp_evo_b", "個体B", color_b, b, evo_b_idx);
                });
                ui.add_space(6.0);

                let Some(evo_a) = a.evolutions.get(*evo_a_idx) else { return };
                let Some(evo_b) = b.evolutions.get(*evo_b_idx) else { return };

                ui.horizontal_wrapped(|ui| {
                    ui.label("グラフ指標：");
                    for (i, (_, label, ..)) in SERIES.iter().enumerate() {
                        ui.radio_value(&mut self.chart_series, i, *label);
                    }
                });
                ui.add_space(4.0);

                let label_a = side_display_name("A", evo_a, a.has_mint);
                let label_b = side_display_name("B", evo_b, b.has_mint);

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    ui.vertical(|ui| {
                        ui.set_width(420.0);
                        ui.label(egui::RichText::new("厳選度比較グラフ").size(13.0).strong().color(style::muted()));
                        compare_chart(ui, evo_a, a.has_mint, &label_a, color_a, evo_b, b.has_mint, &label_b, color_b, self.chart_series);
                        style::small(
                            ui,
                            "厳選度は、全サブスキル・全せいかくの組合せのうち、どの程度優れているかを示す値です。100%が理論値です。",
                            420.0,
                        );
                    });

                    ui.vertical(|ui| {
                        ui.set_width(COMPARE_TABLE_W);
                        ui.label(egui::RichText::new("厳選度 比較表").size(13.0).strong().color(style::muted()));
                        compare_rate_table(ui, evo_a, a.has_mint, color_a, evo_b, b.has_mint, color_b);
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("実際の値 比較表").size(13.0).strong().color(style::muted()));
                        compare_diagnostic_table(ui, evo_a, a.has_mint, color_a, evo_b, b.has_mint, color_b);
                        style::small(
                            ui,
                            "差(A−B)がプラス（赤）は個体Aが上、マイナス（青）は個体Bが上です。食材下の数値は現環境の必要最大数に対する割合です。",
                            COMPARE_TABLE_W,
                        );
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if style::form_button(ui, "個体Aをボックスに追加", style::ButtonKind::Execute).clicked() {
                        self.edit = Some(PokemonEdit::new(None, Some(a.pokemon.clone())));
                    }
                    if style::form_button(ui, "個体Bをボックスに追加", style::ButtonKind::Execute).clicked() {
                        self.edit = Some(PokemonEdit::new(None, Some(b.pokemon.clone())));
                    }
                });
            }
        }
    }
}

fn side_display_name(tag: &str, evo: &Evolution, has_mint: bool) -> String {
    if has_mint { format!("{tag}: {}（ミント後）", evo.name) } else { format!("{tag}: {}", evo.name) }
}

fn evo_selector(ui: &mut egui::Ui, id: &str, title: &str, color: egui::Color32, eval: &Evaluated, idx: &mut usize) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().color(color));
        let current = eval.evolutions.get(*idx).map(|e| e.name.as_str()).unwrap_or("-");
        if eval.evolutions.len() <= 1 {
            ui.label(current);
        } else {
            egui::ComboBox::from_id_salt(id).selected_text(current).width(150.0).show_ui(ui, |ui| {
                for (i, evo) in eval.evolutions.iter().enumerate() {
                    if ui.selectable_label(*idx == i, &evo.name).clicked() {
                        *idx = i;
                    }
                }
            });
        }
        if eval.has_mint {
            ui.label(egui::RichText::new("(ミント後)").size(11.0).color(style::muted()));
        }
    });
}

fn seed_key(name: &str) -> Option<&'static str> {
    master().pokemon(name).map(|p| p.seed.as_deref().unwrap_or(p.name.as_str()))
}

fn side_form_ui(
    ui: &mut egui::Ui,
    id_prefix: &str,
    title: &str,
    accent: egui::Color32,
    form: &mut SideForm,
    box_list: &[BoxPokemon],
) -> bool {
    let m = master();
    let mut open_picker = false;
    egui::Frame::new()
        .fill(style::surface())
        .stroke(egui::Stroke::new(1.0, style::line_color()))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(title).size(15.0).strong().color(accent));
                ui.add_space(6.0);
                if !box_list.is_empty() {
                    let btn_label = form
                        .selected_box_idx
                        .and_then(|i| box_list.get(i))
                        .map(|p| format!("ボックス: No.{} {} Lv{}", p.index + 1, p.name, p.lv as u32))
                        .unwrap_or_else(|| "ボックスから読込…".to_string());
                    if ui.button(btn_label).on_hover_text("ボックスから検索して読み込みます").clicked() {
                        open_picker = true;
                    }
                }
                if ui.small_button("クリア").clicked() {
                    form.clear();
                }
            });
            ui.add_space(4.0);

            egui::Grid::new((id_prefix, "grid")).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                style::th(ui, "ポケモン");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.search).hint_text("名前で検索").desired_width(240.0));
                    let query = form.search.trim().to_string();
                    if !query.is_empty() && query != form.pokemon.name {
                        let hits: Vec<_> = m.pokemon.iter().filter(|p| p.name.contains(&query)).take(10).collect();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            if hits.is_empty() {
                                ui.label("該当なし");
                            }
                            for p in hits {
                                if ui.selectable_label(false, format!("{} ({}・{})", p.name, p.specialty.name(), m.pokemon_skill(p).name)).clicked() {
                                    form.pokemon.name = p.name.clone();
                                    form.search = p.name.clone();
                                    form.selected_box_idx = None;
                                    reset_foods(&mut form.pokemon);
                                }
                            }
                        });
                    } else if form.is_configured() {
                        ui.label(egui::RichText::new(&form.pokemon.name).strong());
                    } else {
                        ui.label(egui::RichText::new("未設定").color(style::muted()));
                    }
                });
                ui.end_row();

                style::th(ui, "食材");
                ui.horizontal_wrapped(|ui| {
                    for slot in 0..3 {
                        let options = food_options(&form.pokemon.name, slot);
                        if form.pokemon.food_list.len() <= slot {
                            form.pokemon.food_list.resize(3, String::new());
                        }
                        if !options.is_empty() && !options.contains(&form.pokemon.food_list[slot]) {
                            form.pokemon.food_list[slot] = options.first().cloned().unwrap_or_default();
                        }
                        combo(ui, (id_prefix, "food", slot), &mut form.pokemon.food_list[slot], options, 108.0);
                    }
                });
                ui.end_row();

                style::th(ui, "サブスキル");
                ui.vertical(|ui| {
                    form.pokemon.sub_skill_list.resize(5, String::new());
                    for (slot, lv) in [10, 25, 50, 75, 100].iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Lv{lv}"));
                            let mut options = vec![String::new()];
                            options.extend(m.sub_skills.iter().map(|s| s.name.clone()));
                            combo(ui, (id_prefix, "sub", slot), &mut form.pokemon.sub_skill_list[slot], options, 195.0);
                        });
                    }
                });
                ui.end_row();

                style::th(ui, "せいかく");
                ui.horizontal_wrapped(|ui| {
                    if combo(ui, (id_prefix, "nature"), &mut form.pokemon.nature, m.natures.iter().map(|n| n.name.clone()), 150.0)
                        && !m.nature(&form.pokemon.nature).is_some_and(|n| n.good.is_some())
                    {
                        form.use_mint = false;
                    }
                    let can_mint = m.nature(&form.pokemon.nature).is_some_and(|n| n.good.is_some());
                    ui.add_enabled_ui(can_mint, |ui| {
                        ui.checkbox(&mut form.use_mint, "ミント使用");
                    });
                });
                ui.end_row();

                style::th(ui, "入力アシスト");
                ui.vertical(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut form.assist_text)
                            .hint_text("名前・食材・サブスキル・せいかくを1行ずつ入力でも反映")
                            .desired_rows(2)
                            .desired_width(260.0),
                    );
                    if resp.changed() {
                        let converted = assist::convert_romaji(&form.assist_text);
                        if converted != form.assist_text {
                            form.assist_text = converted;
                        }
                        let prev_name = form.pokemon.name.clone();
                        assist::infer(&form.assist_text, &mut form.pokemon);
                        if !form.pokemon.name.is_empty() && form.pokemon.name != prev_name {
                            form.search = form.pokemon.name.clone();
                        }
                        if !m.nature(&form.pokemon.nature).is_some_and(|n| n.good.is_some()) {
                            form.use_mint = false;
                        }
                        form.selected_box_idx = None;
                    }
                });
                ui.end_row();
            });
        });
    open_picker
}

fn effective_rates(evo: &Evolution, has_mint: bool, series_idx: usize) -> Vec<Option<f64>> {
    let src = if has_mint { &evo.mint_rates } else { &evo.rates };
    src.iter().map(|r| r[series_idx]).collect()
}

fn effective_berry(evo: &Evolution, has_mint: bool) -> &[Option<f64>] {
    if has_mint { &evo.mint_berry_energy } else { &evo.berry_energy }
}

fn effective_skill(evo: &Evolution, has_mint: bool) -> &[Option<f64>] {
    if has_mint { &evo.mint_skill_activation_num } else { &evo.skill_activation_num }
}

fn effective_food(evo: &Evolution, has_mint: bool, food_name: &str) -> Vec<Option<f64>> {
    let Some((_, before, after)) = evo.foods.iter().find(|(n, ..)| n == food_name) else {
        return vec![None; LV_LIST.len()];
    };
    if has_mint { after.clone() } else { before.clone() }
}

#[allow(clippy::too_many_arguments)]
fn compare_chart(
    ui: &mut egui::Ui,
    evo_a: &Evolution,
    mint_a: bool,
    label_a: &str,
    color_a: egui::Color32,
    evo_b: &Evolution,
    mint_b: bool,
    label_b: &str,
    color_b: egui::Color32,
    series_idx: usize,
) {
    let size = egui::vec2(420.0, 250.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(40.0, 12.0), rect.max - egui::vec2(10.0, 48.0));
    let grid = style::line_color();
    let muted = style::muted();
    let font = egui::FontId::proportional(11.0);

    for i in 0..=5 {
        let v = i as f32 * 20.0;
        let y = plot.bottom() - plot.height() * v / 100.0;
        p.hline(plot.x_range(), y, egui::Stroke::new(1.0, grid));
        p.text(egui::pos2(plot.left() - 4.0, y), egui::Align2::RIGHT_CENTER, format!("{v:.0}%"), font.clone(), muted);
    }
    let x_of = |i: usize| plot.left() + plot.width() * (i as f32 + 0.5) / LV_LIST.len() as f32;
    for (i, lv) in LV_LIST.iter().enumerate() {
        p.text(egui::pos2(x_of(i), plot.bottom() + 4.0), egui::Align2::CENTER_TOP, format!("Lv{lv}"), font.clone(), muted);
    }

    let to_points = |rates: &[Option<f64>]| -> Vec<egui::Pos2> {
        rates
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let v = (r.unwrap_or(0.0) * 100.0).clamp(0.0, 100.0) as f32;
                egui::pos2(x_of(i), plot.bottom() - plot.height() * v / 100.0)
            })
            .collect()
    };

    let pts_a = to_points(&effective_rates(evo_a, mint_a, series_idx));
    let pts_b = to_points(&effective_rates(evo_b, mint_b, series_idx));
    p.add(egui::Shape::line(pts_a.clone(), egui::Stroke::new(2.2, color_a)));
    for pt in pts_a {
        p.circle_filled(pt, 3.5, color_a);
    }
    p.extend(egui::Shape::dashed_line(&pts_b, egui::Stroke::new(2.2, color_b), 7.0, 3.5));
    for pt in pts_b {
        p.circle_filled(pt, 3.5, color_b);
    }

    // 凡例
    let ly1 = rect.bottom() - 26.0;
    let ly2 = rect.bottom() - 12.0;
    p.line_segment([egui::pos2(rect.left() + 12.0, ly1 + 5.0), egui::pos2(rect.left() + 30.0, ly1 + 5.0)], egui::Stroke::new(2.2, color_a));
    p.text(egui::pos2(rect.left() + 36.0, ly1), egui::Align2::LEFT_TOP, format!("{label_a}（実線）"), font.clone(), muted);
    p.extend(egui::Shape::dashed_line(
        &[egui::pos2(rect.left() + 12.0, ly2 + 5.0), egui::pos2(rect.left() + 30.0, ly2 + 5.0)],
        egui::Stroke::new(2.2, color_b),
        5.0,
        2.5,
    ));
    p.text(egui::pos2(rect.left() + 36.0, ly2), egui::Align2::LEFT_TOP, format!("{label_b}（破線）"), font, muted);
}

#[derive(Clone, Copy)]
enum RowBg {
    Header,
    Normal,
    Subtle,
    Diff,
}

fn cmp_table_row(ui: &mut egui::Ui, bg: RowBg, cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + '_>>) {
    let left = ui.cursor().min.x;
    let top = ui.cursor().min.y;
    let pad = 4.0;
    let bg_slot = ui.painter().add(egui::Shape::Noop);
    let mut x = left;
    let mut bottom = top;
    for (col, add) in cells.into_iter().enumerate() {
        let w = COMPARE_COL_W[col];
        let rect = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, 0.0));
        let align = if col <= 1 { egui::Align::Min } else { egui::Align::Max };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(pad, 0.0)).translate(egui::vec2(0.0, pad)).with_max_y(f32::INFINITY))
                .layout(egui::Layout::top_down(align)),
        );
        child.set_width(w - pad * 2.0);
        child.spacing_mut().item_spacing.y = 1.0;
        add(&mut child);
        bottom = bottom.max(child.min_rect().bottom() + pad);
        x += w;
    }
    let fill = match bg {
        RowBg::Header => style::primary_soft(),
        RowBg::Normal => style::surface(),
        RowBg::Subtle => style::surface_subtle(),
        RowBg::Diff => style::mint_soft(),
    };
    let row_rect = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + COMPARE_TABLE_W, bottom));
    let shapes = vec![
        egui::Shape::rect_filled(row_rect, 0.0, fill),
        egui::Shape::hline(row_rect.x_range(), bottom, egui::Stroke::new(1.0, style::line_color())),
    ];
    ui.painter().set(bg_slot, egui::Shape::Vec(shapes));
    ui.allocate_rect(row_rect, egui::Sense::hover());
}

fn cmp_header_cells<'a>() -> Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> {
    let mut cells: Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> = vec![
        Box::new(|ui| {
            style::th(ui, "項目");
        }),
        Box::new(|ui| {
            style::th(ui, "個体");
        }),
    ];
    for lv in LV_LIST {
        cells.push(Box::new(move |ui| {
            style::th(ui, &format!("Lv{lv}"));
        }));
    }
    cells
}

fn compare_rate_table(
    ui: &mut egui::Ui,
    evo_a: &Evolution,
    mint_a: bool,
    color_a: egui::Color32,
    evo_b: &Evolution,
    mint_b: bool,
    color_b: egui::Color32,
) {
    let old = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 0.0);
    cmp_table_row(ui, RowBg::Header, cmp_header_cells());
    for (si, (_, label, ..)) in SERIES.iter().enumerate() {
        let va = effective_rates(evo_a, mint_a, si);
        let vb = effective_rates(evo_b, mint_b, si);
        emit_metric_rows(
            ui,
            Box::new(move |ui| {
                ui.label(*label);
            }),
            &va,
            &vb,
            color_a,
            color_b,
            1,
            "%",
            true,
            None,
        );
    }
    ui.spacing_mut().item_spacing.y = old;
}

fn compare_diagnostic_table(
    ui: &mut egui::Ui,
    evo_a: &Evolution,
    mint_a: bool,
    color_a: egui::Color32,
    evo_b: &Evolution,
    mint_b: bool,
    color_b: egui::Color32,
) {
    let old = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 0.0);
    cmp_table_row(ui, RowBg::Header, cmp_header_cells());

    // きのみエナジー
    emit_metric_rows(
        ui,
        Box::new(|ui| {
            ui.label("きのみエナジー");
        }),
        effective_berry(evo_a, mint_a),
        effective_berry(evo_b, mint_b),
        color_a,
        color_b,
        1,
        "",
        false,
        None,
    );

    // 食材の和集合
    let mut all_foods: Vec<String> = Vec::new();
    for (f, ..) in evo_a.foods.iter().chain(evo_b.foods.iter()) {
        if !all_foods.contains(f) {
            all_foods.push(f.clone());
        }
    }
    for food in &all_foods {
        let va = effective_food(evo_a, mint_a, food);
        let vb = effective_food(evo_b, mint_b, food);
        let food_name = food.clone();
        let food_for_req = food.clone();
        emit_metric_rows(
            ui,
            Box::new(move |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    food_icon::food_icon(ui, &food_name, 18.0);
                    ui.label(&food_name);
                });
            }),
            &va,
            &vb,
            color_a,
            color_b,
            1,
            "",
            false,
            Some(food_for_req),
        );
    }

    // スキル発動回数
    emit_metric_rows(
        ui,
        Box::new(|ui| {
            ui.label("スキル発動回数");
        }),
        effective_skill(evo_a, mint_a),
        effective_skill(evo_b, mint_b),
        color_a,
        color_b,
        2,
        "",
        false,
        None,
    );

    ui.spacing_mut().item_spacing.y = old;
}

#[allow(clippy::too_many_arguments)]
fn emit_metric_rows<'a>(
    ui: &mut egui::Ui,
    title_cell: Box<dyn FnOnce(&mut egui::Ui) + 'a>,
    va: &[Option<f64>],
    vb: &[Option<f64>],
    color_a: egui::Color32,
    color_b: egui::Color32,
    digits: usize,
    suffix: &str,
    is_percent_rate: bool,
    food_name: Option<String>,
) {
    let scale = if is_percent_rate { 100.0 } else { 1.0 };
    let fmt_val = |v: Option<f64>| -> String { v.map_or("－".to_string(), |x| format!("{}{suffix}", fmt_fixed(x * scale, digits))) };

    // 個体A行
    let mut cells_a: Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> = vec![
        title_cell,
        Box::new(move |ui| {
            ui.label(egui::RichText::new("個体A").strong().color(color_a));
        }),
    ];
    for &v in va {
        let s = fmt_val(v);
        let req = food_name.as_deref().map(|fnm| food_requirement_rate(v, fnm)).unwrap_or_default();
        cells_a.push(Box::new(move |ui| {
            ui.label(s);
            if !req.is_empty() {
                ui.label(egui::RichText::new(req).size(10.5).color(style::muted()));
            }
        }));
    }
    cmp_table_row(ui, RowBg::Normal, cells_a);

    // 個体B行
    let mut cells_b: Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> = vec![
        Box::new(|_| {}),
        Box::new(move |ui| {
            ui.label(egui::RichText::new("個体B").strong().color(color_b));
        }),
    ];
    for &v in vb {
        let s = fmt_val(v);
        let req = food_name.as_deref().map(|fnm| food_requirement_rate(v, fnm)).unwrap_or_default();
        cells_b.push(Box::new(move |ui| {
            ui.label(s);
            if !req.is_empty() {
                ui.label(egui::RichText::new(req).size(10.5).color(style::muted()));
            }
        }));
    }
    cmp_table_row(ui, RowBg::Subtle, cells_b);

    // 差分行 (A - B)
    let mut cells_d: Vec<Box<dyn FnOnce(&mut egui::Ui) + 'a>> = vec![
        Box::new(|_| {}),
        Box::new(|ui| {
            ui.label(egui::RichText::new("差 (A−B)").size(10.5).color(style::muted()));
        }),
    ];
    for i in 0..LV_LIST.len() {
        let a = va.get(i).copied().flatten().map(|x| x * scale);
        let b = vb.get(i).copied().flatten().map(|x| x * scale);
        let diff = match (a, b) {
            (Some(a), Some(b)) => diff_info(Some(b), Some(a), digits, suffix),
            (None, Some(b)) if food_name.is_some() => diff_info(Some(b), Some(0.0), digits, suffix),
            (Some(a), None) if food_name.is_some() => diff_info(Some(0.0), Some(a), digits, suffix),
            _ => None,
        };
        cells_d.push(Box::new(move |ui| {
            if let Some((text, color)) = diff {
                ui.label(egui::RichText::new(text).size(11.0).strong().color(color));
            } else {
                ui.label(egui::RichText::new("－").size(10.5).color(style::muted()));
            }
        }));
    }
    cmp_table_row(ui, RowBg::Diff, cells_d);
}
