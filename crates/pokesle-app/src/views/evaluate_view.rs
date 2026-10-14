//! 基準生成画面(本家 pages/setting.vue): 厳選基準の詳細設定(components/evaluate-setting.vue)と厳選テーブルの作成

use eframe::egui;
use pokesle_core::config::{Config, EvaluateConfig, EvaluateRule, EvaluateRuleSetting, PokemonTarget};
use pokesle_core::data::{master, Specialty};
use pokesle_core::evaluate_setting::TARGET_SPECIALTY_LIST;
use pokesle_core::evaluate_table::{simulate_evaluate_table, EvaluateTable};

use super::style::{self, help, js_num, number, percent_number, small, th, ButtonKind};

/// 見出し(上揃え)
fn lbl(ui: &mut egui::Ui, text: &str) {
    ui.vertical(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        th(ui, text);
    });
}
use super::pokemon_target;
use crate::app::Shared;
use crate::job::Job;
use crate::storage;

#[derive(Default)]
pub struct EvaluateView {
    job: Option<Job<EvaluateTable>>,
    done: bool,
    error: String,
    /// 編集中の設定(本家 editConfig)。保存ボタンを押すまで反映しない
    edit: Option<Config>,
    /// 前回描いたパス番号。続けて描いていなければ(別の画面から戻ったら)編集中の設定を作り直す
    last_pass: u64,
}

impl EvaluateView {
    pub fn ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared) {
        if let Some(job) = &mut self.job
            && let Some(r) = job.poll()
        {
            self.job = None;
            match r {
                Ok(table) => {
                    if let Err(e) = storage::save_evaluate_table(&table) {
                        self.error = format!("厳選テーブルを保存できませんでした: {e}");
                    }
                    shared.evaluate_table = Some(table);
                    self.done = true;
                }
                Err(e) => self.error = e,
            }
        }

        let pass = ui.ctx().cumulative_pass_nr();
        if self.edit.is_none() || pass != self.last_pass + 1 {
            self.edit = Some(shared.config.clone());
        }
        self.last_pass = pass;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(1200.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            match &shared.evaluate_table {
                Some(t) if t.version != master().version.evaluate => {
                    style::danger(ui, "初めての利用、もしくはシステムがアップデートされたため厳選テーブルがクリアされました。")
                }
                None => style::danger(ui, "初めての利用、もしくはシステムがアップデートされたため厳選テーブルがクリアされました。"),
                Some(_) if shared.usable_evaluate_table().is_none() => {
                    style::danger(ui, "睡眠時間、チェック回数が変更されたため厳選テーブルがクリアされました。")
                }
                _ => {}
            }
            style::alert(
                ui,
                "一部を除く全ポケモンの厳選テーブルを計算するページです。\n計算しておくと、ボックス画面などで厳選度が表示されるようになります。\n※ダークライやミュウは食材の組合せが512通りあり計算量が多いため対象外です。これらの特別なポケモンは簡易診断で見てください。",
            );

            let edit = self.edit.as_mut().expect("編集中の設定");
            evaluate_setting(ui, edit, false);

            if self.done {
                style::alert(ui, "厳選情報の計算が完了しました。ボックス画面から厳選情報を確認できます。");
            }
            if !self.error.is_empty() {
                style::danger(ui, &self.error);
            }
            match &self.job {
                Some(job) => job.show_progress(ui),
                None => {
                    if style::form_button(ui, "設定を保存して厳選情報を計算する", ButtonKind::Execute).clicked() {
                        shared.config = edit.clone();
                        shared.save_config();
                        self.done = false;
                        self.error.clear();
                        let config = shared.config.clone();
                        self.job = Some(Job::spawn(ui.ctx(), move |p| simulate_evaluate_table(&config, &p)));
                    }
                }
            }
        });
    }
}

/// 本家 ToggleArea(見出しをクリックで開閉、最初は開いている)
fn toggle_area(ui: &mut egui::Ui, id: &str, header: impl Into<egui::WidgetText>, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(header).id_salt(("evaluate_setting", id)).default_open(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        add(ui);
    });
}

/// 本家 SettingList(見出しと中身を横に並べた一覧)
fn setting_list(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(("evaluate_setting_list", id)).num_columns(2).spacing([16.0, 12.0]).min_col_width(110.0).show(ui, add);
}

/// 単位付きの数値入力
fn num_unit(ui: &mut egui::Ui, value: &mut f64, speed: f64, width: f32, unit: &str) -> bool {
    ui.horizontal(|ui| {
        let changed = number(ui, value, speed, width);
        if !unit.is_empty() {
            ui.label(unit);
        }
        changed
    })
    .inner
}

/// スキルをカテゴリごとにチェックボックスで並べる(本家の Skill.categoryList ループ)
fn skill_checkboxes(ui: &mut egui::Ui, mut checkbox: impl FnMut(&mut egui::Ui, &str)) {
    let m = master();
    for cat in &m.skill_categories {
        let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
        if skills.is_empty() {
            continue;
        }
        style::section_title(ui, &cat.name);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for s in skills {
                checkbox(ui, &s.name);
            }
        });
    }
}

/// Lv30以降で1レベル上がることで増える1日のきのみエナジー/経験値 の最大(本家 maxEnergyPerExp)
struct MaxEnergyPerExp {
    score: f64,
    lv: usize,
    require_exp: f64,
    energy: f64,
    before_energy: f64,
    name: String,
}

fn max_energy_per_exp() -> MaxEnergyPerExp {
    let m = master();
    let mut best: Option<MaxEnergyPerExp> = None;
    for p in m.pokemon.iter().filter(|p| p.is_last) {
        let berry = &m.berries[p.berry];
        let mut before_energy = 0.0;
        let mut max = MaxEnergyPerExp { score: 0.0, lv: 0, require_exp: 0.0, energy: 0.0, before_energy: 0.0, name: p.name.clone() };
        for lv in 1..=m.exp.len() {
            let l = lv as f64;
            let rate = if matches!(p.specialty, Specialty::Berry | Specialty::All) { 3.0 } else { 2.0 };
            let energy = (berry.energy + l - 1.0).max(berry.energy * 1.025_f64.powf(l - 1.0)) * rate * 86400.0 / p.help / 0.45 / 0.65 / 0.9 / (1.0 - (l - 1.0) * 0.002);
            if lv >= 30 {
                let require_exp = (m.exp[lv - 1].total - m.exp[lv - 2].total) * p.exp;
                let score = (energy - before_energy) / require_exp;
                if max.score < score {
                    max = MaxEnergyPerExp { score, lv, require_exp, energy, before_energy, name: p.name.clone() };
                }
            }
            before_energy = energy;
        }
        // 本家は score の降順で安定ソートした先頭
        if best.as_ref().is_none_or(|b| b.score < max.score) {
            best = Some(max);
        }
    }
    best.expect("最終進化のポケモンがいません")
}

/// 本家 EvaluateSetting(isTmpEvaluate = false)
/// 基準生成・簡易診断の設定。`tmp` は簡易診断(厳選レベルとサポートスキル評価用の欄を出さない)
pub fn evaluate_setting(ui: &mut egui::Ui, c: &mut Config, tmp: bool) {
    let m = master();
    let e = &mut c.select_evaluate;

    toggle_area(ui, "basic", "基本設定", |ui| {
        setting_list(ui, "basic", |ui| {
            lbl(ui, "睡眠時間");
            num_unit(ui, &mut c.sleep_time, 0.1, 60.0, "時間");
            ui.end_row();

            lbl(ui, "日中タップ回数");
            num_unit(ui, &mut c.check_freq, 1.0, 60.0, "回");
            ui.end_row();

            lbl(ui, "おてつだいボーナス評価");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([80.0, 22.0], egui::DragValue::new(&mut e.team_help_bonus).range(0.0..=4.0));
                    ui.label("匹");
                });
                small(ui, "自分以外の4匹におてつだいボーナスが何匹いるか指定します。\n3匹にすると、おてボ＋おてスピMが最も高く評価されます。", 400.0);
            });
            ui.end_row();

            lbl(ui, "銀種前提厳選");
            ui.vertical(|ui| {
                let mut list: Vec<_> = m.sub_skills.iter().filter(|x| x.next.is_some()).collect();
                list.sort_by(|a, b| b.name.encode_utf16().cmp(a.name.encode_utf16()));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for s in list {
                        let v = e.silver_seed.entry(s.name.clone()).or_insert(false);
                        ui.checkbox(v, &s.name);
                    }
                });
                small(ui, "銀種を使うサブスキルを設定してください。", 400.0);
            });
            ui.end_row();

            if !tmp {
                lbl(ui, "厳選レベル");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for lv in [10, 25, 30, 50, 60, 70, 80] {
                            let v = e.level_list.entry(lv).or_insert(false);
                            ui.checkbox(v, lv.to_string());
                        }
                    });
                    small(ui, "どのLv時点の評価で厳選するか設定します。", 400.0);
                });
                ui.end_row();
            }
        });
    });

    toggle_area(ui, "exp", "経験値関連補正", |ui| {
        ui.label("経験値に関するサブスキル・せいかく・スキルはエナジーには直接影響を与えないため、あなたのプレイスタイルに応じて補正値を設定できます。");
        ui.horizontal_top(|ui| {
            egui::Grid::new("exp_setting").striped(true).num_columns(3).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label("");
                lbl(ui, "加算");
                lbl(ui, "倍率");
                ui.end_row();
                for (title, v) in [("睡眠EXPボーナス", &mut e.sub_skill.suimin_exp_bonus), ("EXP獲得量▲▲", &mut e.nature.exp_up), ("EXP獲得量▼▼", &mut e.nature.exp_down)] {
                    lbl(ui, title);
                    ui.horizontal(|ui| {
                        ui.label("＋");
                        number(ui, &mut v.add, 1.0, 50.0);
                    });
                    ui.horizontal(|ui| {
                        percent_number(ui, &mut v.rate, 50.0);
                        ui.label("%");
                    });
                    ui.end_row();
                }
            });
            ui.add_space(10.0);
            setting_list(ui, "exp_energy", |ui| {
                lbl(ui, "経験値のエナジー換算");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized([50.0, 22.0], egui::DragValue::new(&mut e.energy_per_candy).range(0.0..=100.0));
                        ui.label("エナジー / 25経験値");
                    });
                    let x = max_energy_per_exp();
                    ui.horizontal(|ui| {
                        small(ui, &format!("アメを獲得するスキルのエナジー計算に利用されます。\n参考値:{:.0} エナジー / 日 / アメ", x.score * 25.0), 400.0);
                        help(
                            ui,
                            &format!(
                                "参考情報\nLv30以降で1レベル上がることで増加する1日のきのみエナジーの理論値/経験値の最大値は {:.2} です。\n※Lvが低いうちはきのみエナジーの計算式が異なるので30以降で計算\n\n\
                                 上記値になるケース\n{}\nLv{}: {}エナジー\nLv{}: {}エナジー\n (必要経験値: {})\n\n\
                                 よって、アメ1個あたりの数値に直すと {:.0}\nその育てたポケモンを1週間運用する場合は {:.0}\nその育てたポケモンを30日運用する場合は {:.0}\n\n\
                                 理論値で計算しているのでその点については高めに出ている一方、サブスキルの解放は考慮していない値のため、概ねこの値を設定しておけばそれっぽい数値になるかと思います。\n育てたポケモンを平均何日運用するかで調整してください。",
                                x.score,
                                x.name,
                                x.lv - 1,
                                pokesle_core::js::round(x.before_energy),
                                x.lv,
                                pokesle_core::js::round(x.energy),
                                js_num(x.require_exp),
                                x.score * 25.0,
                                x.score * 25.0 * 7.0,
                                x.score * 25.0 * 30.0,
                            ),
                        );
                    });
                });
                ui.end_row();
            });
        });
    });

    toggle_area(ui, "calc", "計算設定", |ui| {
        setting_list(ui, "calc", |ui| {
            lbl(ui, "エナジー/ゆめのかけら");
            ui.vertical(|ui| {
                number(ui, &mut e.shard_energy_rate, 1.0, 80.0);
                ui.horizontal(|ui| {
                    small(ui, "ゆめのかけら1個を得る\nのに必要なエナジー", 200.0);
                    help(ui, SHARD_ENERGY_RATE_HELP);
                });
            });
            ui.end_row();

            lbl(ui, "ゆめのかけらゲット評価");
            ui.vertical(|ui| {
                number(ui, &mut e.shard_energy, 1.0, 80.0);
                ui.horizontal(|ui| {
                    small(ui, "1個あたり何エナジー\nとして換算するか", 200.0);
                    help(ui, SHARD_ENERGY_HELP);
                });
            });
            ui.end_row();

            lbl(ui, "ゆめのかけらボーナス評価");
            ui.vertical(|ui| {
                num_unit(ui, &mut e.shard_bonus, 1.0, 80.0, "%");
                small(ui, "0%:エナジー換算しない\n100%:エナジー6%アップとして評価", 200.0);
            });
            ui.end_row();

            if !tmp {
                lbl(ui, "サポートスキル評価用");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("厳選ライン：");
                        num_unit(ui, &mut e.support_border, 1.0, 40.0, "%");
                    });
                    ui.horizontal(|ui| {
                        ui.label("上位：");
                        num_unit(ui, &mut e.support_rank_num, 1.0, 40.0, "%");
                    });
                    small(ui, "サポートスキル評価時に参照する他ポケモンの厳選度と上位何%を使用するか", 200.0);
                });
                ui.end_row();
            }

            lbl(ui, "料理パワーアップ評価方法");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut e.cooking_power_up_type, 0.0, "3種のうち最大");
                    ui.radio_value(&mut e.cooking_power_up_type, 1.0, "3種の平均");
                });
                ui.horizontal(|ui| {
                    ui.label("上記の");
                    num_unit(ui, &mut e.cooking_power_up_rate, 1.0, 40.0, "%");
                });
                let st = &m.cooking_stats;
                ui.horizontal(|ui| {
                    small(ui, &format!("なべ拡張の1個分を何エナジーとして評価するか。現在の平均は {:.1} エナジーです。", st.cooking_power_up_energy_average), 240.0);
                    let map = |k: &str| st.cooking_power_up_energy_map.get(k).copied().unwrap_or(f64::NAN);
                    help(
                        ui,
                        &format!(
                            "考え方\nいいキャンプチケットを使用しない場合は、料理パワーアップを発動することでより良い料理が作れるようになります。\n10個分拡大して10000エナジー高い料理が作れる場合、1個分の拡大あたり1000エナジーの価値があると考えられます。\nその考えに基づき、現在のなべの最大値で作れる料理と最も良い料理のエナジー差と、食材数の差から価値を計算しています。\n\n\
                             この時、料理3種それぞれで差が異なるので、そのどれを使用するかを設定できます。\n現環境では以下の通りです。\nカレー：{:.1}エナジー\nサラダ：{:.1}エナジー\nデザート：{:.1}エナジー\n平均：{:.1}エナジー\n\n\
                             また、食材が十分用意された状態では料理パワーアップの価値は上記の通り計算できますが、これから食材も集める場合は料理パワーアップだけでは意味がないので、食材担当と併用する運用が基本の場合は100%ではなく50%などの値にする必要があります。",
                            map("カレー"),
                            map("サラダ"),
                            map("デザート"),
                            st.cooking_power_up_energy_average
                        ),
                    );
                });
            });
            ui.end_row();

            lbl(ui, "仮定ヒーラー");
            ui.vertical(|ui| {
                number(ui, &mut e.healer, 1.0, 80.0);
                small(ui, "性格のげんき補正評価用の\n1日の回復量", 200.0);
            });
            ui.end_row();

            lbl(ui, "げんき計算");
            ui.vertical(|ui| {
                ui.checkbox(&mut e.genki_full_if_self_heal, "げんきチャージ持ちは\n常時げんき100%とする");
                small(ui, "ONにするとせいかくのげんき補正が無視されます", 300.0);
            });
            ui.end_row();

            lbl(ui, "おやすみリボン");
            num_unit(ui, &mut e.pokemon_sleep_time, 1.0, 50.0, "時間");
            ui.end_row();
        });
    });

    let expect_header = egui::RichText::new("下振れ計算設定");
    toggle_area(ui, "expect", expect_header, |ui| {
        ui.horizontal(|ui| {
            ui.label("下振れすると困る要素にチェックをつけておくと、その点において下振れしにくいポケモンがより高い評価になります。");
            help(ui, EXPECT_HELP);
        });
        setting_list(ui, "expect", |ui| {
            lbl(ui, "下振れ考慮ボーダー");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let v = e.expect_type.entry("border".into()).or_insert(0.0);
                    ui.add_sized([50.0, 22.0], egui::DragValue::new(v).range(0.0..=100.0));
                    ui.label("%");
                });
                small(ui, "設定した確率で下回らない値を評価に使います。", 150.0);
            });
            ui.end_row();

            lbl(ui, "食材評価");
            ui.vertical(|ui| {
                let v = e.expect_type.entry("food".into()).or_insert(0.0);
                ui.radio_value(v, 0.0, "通常期待値");
                ui.radio_value(v, 1.0, "下振れ考慮");
            });
            ui.end_row();

            lbl(ui, "下振れ考慮するスキル");
            ui.vertical(|ui| {
                skill_checkboxes(ui, |ui, name| {
                    let mut on = e.expect_type.get(name).copied() == Some(1.0);
                    if ui.checkbox(&mut on, name).changed() {
                        e.expect_type.insert(name.to_string(), if on { 1.0 } else { 0.0 });
                    }
                });
                small(ui, "未選択のスキルは通常期待値で評価します。", 400.0);
            });
            ui.end_row();
        });
    });

    toggle_area(ui, "specialty", "とくい毎設定", |ui| specialty_table(ui, e));

    toggle_area(ui, "skill_lv", "スキルレベル設定", |ui| {
        ui.label("とくい問わずスキルレベルを最大にしたいスキルを選択してください。");
        skill_checkboxes(ui, |ui, name| {
            let list = &mut e.max_skill_lv_skill_name_list;
            let mut on = list.iter().any(|x| x == name);
            if ui.checkbox(&mut on, name).changed() {
                if on {
                    list.push(name.to_string());
                } else if let Some(i) = list.iter().position(|x| x == name) {
                    list.remove(i);
                }
            }
        });
    });

    toggle_area(ui, "rule", "ポケモン詳細設定", |ui| rule_list(ui, e));

    let skill_energy_header = egui::RichText::new("スキルエナジー設定");
    toggle_area(ui, "skill_energy", skill_energy_header, |ui| {
        ui.horizontal(|ui| {
            ui.label("デフォルト値の理由");
            help(ui, SKILL_ENERGY_HELP);
        });
        setting_list(ui, "skill_energy", |ui| {
            for skill in m.skills.iter().filter(|s| s.evaluate_energy.is_some()) {
                let defaults = skill.evaluate_energy.as_deref().unwrap_or_default();
                lbl(ui, &format!("{}のエナジー換算", skill.name));
                ui.vertical(|ui| {
                    let list = e.skill_energy.entry(skill.name.clone()).or_default();
                    if list.len() < defaults.len() {
                        list.resize(defaults.len(), None);
                    }
                    for (i, energy) in defaults.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Lv{}：", i + 1));
                            style::opt_number(ui, &format!("skill_energy_{}_{i}", skill.name), &mut list[i], false, 60.0, &js_num(*energy));
                        });
                    }
                });
                ui.end_row();
            }
        });
    });

    toggle_area(ui, "other", "その他設定", |ui| {
        setting_list(ui, "other", |ui| {
            lbl(ui, "スレッド数");
            ui.horizontal(|ui| {
                ui.add_sized([60.0, 22.0], egui::DragValue::new(&mut c.worker_num).range(1.0..=100.0));
                small(ui, "ワーカースレッドの数を指定します。", 300.0);
            });
            ui.end_row();
        });
    });
}

const SPECIALTY_LIST: [&str; 4] = ["きのみ", "食材", "スキル", "オール"];

/// とくい毎設定の表
fn specialty_table(ui: &mut egui::Ui, e: &mut EvaluateConfig) {
    egui::ScrollArea::horizontal().id_salt("specialty_table").show(ui, |ui| {
        egui::Grid::new("specialty_table").striped(true).num_columns(6).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("");
            for s in SPECIALTY_LIST {
                lbl(ui, s);
            }
            lbl(ui, "備考");
            ui.end_row();

            type Field = fn(&mut pokesle_core::config::SpecialtyEvaluateConfig) -> &mut f64;
            let rows: [(&str, Field, &str); 3] = [
                ("きのみ評価", |x| &mut x.berry_energy_rate, "100%:好みと合わせない前提\n200%:好みと合わせる前提\n240%:EXやイベントボーナスを想定"),
                ("食材評価", |x| &mut x.food_energy_rate, "0%:基礎エナジー\n100%:最良料理×最大レシピLv"),
                ("食材ゲット評価", |x| &mut x.food_get_rate, "0%:基礎エナジー\n100%:最良料理×最大レシピLv"),
            ];
            for (title, field, note) in rows {
                lbl(ui, title);
                for s in SPECIALTY_LIST {
                    match e.specialty.get_mut(s) {
                        Some(x) => {
                            num_unit(ui, field(x), 1.0, 40.0, "%");
                        }
                        None => {
                            ui.label("-");
                        }
                    }
                }
                small(ui, note, 200.0);
                ui.end_row();
            }

            lbl(ui, "スキルレベル設定");
            for s in SPECIALTY_LIST {
                match e.specialty.get_mut(s) {
                    Some(x) => {
                        ui.vertical(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            for (v, l) in [(1.0, "最大進化時Lv"), (2.0, "最大Lv")] {
                                if ui.radio(x.skill_lv_type == Some(v), l).clicked() {
                                    x.skill_lv_type = Some(v);
                                }
                            }
                        });
                    }
                    None => {
                        ui.label("-");
                    }
                }
            }
            small(ui, "「最大進化時Lv」は、例えばライチュウならLv3です。", 200.0);
            ui.end_row();
        });
    });
}

/// 本家 createEvaluateRuleSetting
fn new_rule_setting() -> EvaluateRuleSetting {
    EvaluateRuleSetting { setting_type: "berryEnergyRate".into(), skill_name: None, value: Some(100.0), skill_lv_type: Some(1.0), skill_lv: Some(1.0) }
}

/// 本家 createEvaluateRule
fn new_rule() -> EvaluateRule {
    EvaluateRule {
        target: PokemonTarget {
            target_type: Some("all".into()),
            all: true,
            specialties: TARGET_SPECIALTY_LIST.iter().map(|s| (s.to_string(), false)).collect(),
            skill_name_list: Vec::new(),
            friend_point: None,
            pokemon_name_list: Vec::new(),
        },
        setting_list: vec![new_rule_setting()],
    }
}

/// 上書き設定値の種類の表示名
fn rule_setting_label(s: &EvaluateRuleSetting) -> String {
    match s.setting_type.as_str() {
        "berryEnergyRate" => "きのみ評価".into(),
        "foodEnergyRate" => "食材評価".into(),
        "foodGetRate" => "食材ゲット評価".into(),
        "skillLv" => s.skill_name.clone().unwrap_or_default(),
        x => x.into(),
    }
}

/// ポケモン詳細設定(本家 evaluate-rule-list)
fn rule_list(ui: &mut egui::Ui, e: &mut EvaluateConfig) {
    let m = master();
    ui.label("とくい毎設定・スキルレベル設定を更に細かい条件で上書き出来ます。");
    let mut remove_rule = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
        for (index, rule) in e.rule_list.iter_mut().enumerate() {
            egui::Frame::new().stroke(egui::Stroke::new(1.0, style::line_color())).corner_radius(7).inner_margin(10).show(ui, |ui| {
                ui.set_width(450.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("設定 {}", index + 1)).strong().color(style::primary_strong()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if style::form_button(ui, "削除", ButtonKind::Normal).clicked() {
                                remove_rule = Some(index);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    lbl(ui, "対象");
                    pokemon_target::editor(ui, ("evaluate_rule", index), &mut rule.target);
                    ui.add_space(10.0);

                    let mut remove_setting = None;
                    for (si, setting) in rule.setting_list.iter_mut().enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                lbl(ui, "上書き設定値");
                                egui::ComboBox::from_id_salt(("evaluate_rule_setting", index, si)).width(180.0).selected_text(rule_setting_label(setting)).show_ui(ui, |ui| {
                                    for (ty, label) in [("berryEnergyRate", "きのみ評価"), ("foodEnergyRate", "食材評価"), ("foodGetRate", "食材ゲット評価")] {
                                        if ui.selectable_label(setting.setting_type == ty, label).clicked() {
                                            setting.setting_type = ty.into();
                                            setting.skill_name = None;
                                        }
                                    }
                                    for cat in &m.skill_categories {
                                        let skills: Vec<_> = m.skills.iter().filter(|s| s.category == cat.id).collect();
                                        if skills.is_empty() {
                                            continue;
                                        }
                                        ui.label(egui::RichText::new(&cat.name).strong().color(style::muted()));
                                        for s in skills {
                                            let selected = setting.setting_type == "skillLv" && setting.skill_name.as_deref() == Some(&s.name);
                                            if ui.selectable_label(selected, format!("　{}", s.name)).clicked() {
                                                setting.setting_type = "skillLv".into();
                                                setting.skill_name = Some(s.name.clone());
                                            }
                                        }
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.add_space(18.0);
                                if setting.setting_type != "skillLv" {
                                    ui.horizontal(|ui| {
                                        style::opt_number(ui, &format!("evaluate_rule_value_{index}_{si}"), &mut setting.value, false, 60.0, "");
                                        ui.label("%");
                                    });
                                } else {
                                    for (v, l) in [(1.0, "最大進化時Lv"), (2.0, "最大Lv")] {
                                        if ui.radio(setting.skill_lv_type == Some(v), l).clicked() {
                                            setting.skill_lv_type = Some(v);
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        if ui.radio(setting.skill_lv_type == Some(3.0), "指定レベル").clicked() {
                                            setting.skill_lv_type = Some(3.0);
                                        }
                                        ui.add_enabled_ui(setting.skill_lv_type == Some(3.0), |ui| {
                                            style::opt_number(ui, &format!("evaluate_rule_lv_{index}_{si}"), &mut setting.skill_lv, false, 50.0, "");
                                        });
                                    });
                                }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.add_space(0.0);
                                if style::form_button(ui, "削除", ButtonKind::Normal).clicked() {
                                    remove_setting = Some(si);
                                }
                            });
                        });
                    }
                    if let Some(si) = remove_setting {
                        rule.setting_list.remove(si);
                    }
                    if style::form_button(ui, "上書き設定値を追加", ButtonKind::Normal).clicked() {
                        rule.setting_list.push(new_rule_setting());
                    }
                });
            });
        }
    });
    if let Some(i) = remove_rule {
        e.rule_list.remove(i);
    }
    if style::form_button(ui, "設定を追加", ButtonKind::Normal).clicked() {
        e.rule_list.push(new_rule());
    }
}

const SHARD_ENERGY_RATE_HELP: &str = "結論\nEXフィールド＆リサーチランクカンストを見据えるなら 50\nそうでないなら 120\n睡眠スコアが100でない場合はその分増やしてください(睡眠スコアが50なら倍)\n\n\
計算\n基本的に約12000ねむけパワーにつき1ゆめのかけらが得られます。\n睡眠スコアが100と考えると、120エナジー/ゆめのかけら となります。\n\n\
リサーチランクがカンストしていると、およそ半分ほどのリサーチEXPもゆめのかけらになるので、80エナジー/ゆめのかけら となります。\n\n\
更にEXフィールドだと、ゆめのかけら・リサーチEXPは8割ほど多く手に入るため、50エナジー/ゆめのかけら となります。";

const SHARD_ENERGY_HELP: &str = "結論\nゆめのかけらゲットを編成したい日に応じて、エナジー/ゆめのかけら 設定値に対し、以下を掛け算した値\n月曜～：1/4\n火曜～：1/3.5\n水曜～：1/3\n木曜～：1/2.5\n金曜～：1/2\n土曜～：1/1.5\n日曜～：1/1\n\n\
計算\nエナジー/ゆめのかけら の設定値のデフォルトは50ですが、つまり月曜に50エナジーを稼ぐと、7回のリサーチにかかるので実質 50エナジー / 7ゆめのかけら になり、7エナジー / 1ゆめのかけら になります。\n\n\
ゆめのかけらゲットが真価を発揮するのはエナジーを稼いでも評価回数が少ない週の後半になります。\n日曜からなら1日だけなので、エナジー/ゆめのかけら 設定と同じ 50 を設定するのが良いでしょう。";

const EXPECT_HELP: &str = "下振れ考慮について\n1%で当たるものを100回試行した時の期待値は1%×100で1回ですが、1回以上当たる確率は63.4%程度しかありません。\nつまり、下振れして1回も当たらない確率が36.6%あるということです。\n試行回数がもっと多ければ期待値に収束していきますが、1日のおてつだい回数は多くて100回前後なので、頻繁に上振れ・下振れが発生します。\n特に料理関係は下振れすると困ることが多いので、この下振れ前提で評価させるようにすると、より安定した個体が評価されるようになります。\n(遅くても確率が高い方が高く評価されます)\n\n\
サブスキル・せいかく評価への影響\n「期待値」によって計算する場合、食材やスキルだけに限って考えると、確率アップとおてつだいスピードの短縮はほぼ等価です(実際は所持数溢れのためおてつだいスピードの方が若干評価が劣ります)。\n「下振れ考慮」によって計算する場合、確率が高い方が下振れが起きにくいので確率アップの方が若干高く評価されます。\n\n\
その他、通常期待値との違い\nとくい分野の厳選度はより実態に即したものに近づきます。\n総合スコアの厳選度は通常の期待値より食材・スキルが少なく見積もられることになるので、相対的にきのみの数Sなどが高く評価されることになります。\n食材とくいは収束しにくいABCだと食材の価値が若干低く見積もられ、相対的にきのみとスキルに関連するサブスキルが高く評価されることになります。";

const SKILL_ENERGY_HELP: &str = "スキルコピーやほっぺすりすりの対象はエナジーチャージM相当で評価しています。\nエナジーチャージMより強いスキルも一部ありますが、げんきオールや料理チャンスは積めば積むほど効果量が薄くなり、でんせつポケモンのスキルは1匹しか入れられないので、これらを基準にすると過剰に評価してしまいます。\nそのため、固定値で4匹固められる可能性のあるエナジーチャージMを基準にしています。\n\n\
ほっぺすりすりのデフォルト値\nエナジーチャージM持ちの中でもスキル確率の高いウソッキー(9%)に合わせ、補正が諸々かかった15%前提で計算しています。";
