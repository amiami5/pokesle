//! アプリ全体の状態と画面切り替え

use eframe::egui;
use pokesle_core::box_io::BoxFile;
use pokesle_core::config::Config;
use pokesle_core::evaluate_table::EvaluateTable;

use crate::storage;
use crate::views::style;
use crate::views::{box_view::BoxView, check_list_view::CheckListView, credit_view, evaluate_view::EvaluateView, team_view::TeamView, ui_setting_view::{self, UiSettingView}};

/// 画面(本家のルーティング)
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Box,
    CheckList,
    Summary,
    Team,
    FoodStock,
    Setting,
    EvaluateTable,
    TmpEvaluate,
    Data,
    Faq,
    History,
    Credit,
    UiSetting,
}

/// ヘッダーのリンク(画面, 表示名, 実装済みか)
type NavItem = (Tab, &'static str, bool);

/// ヘッダーのグループ(本家 app.vue)。(見出し, リンク)
const NAV: [(&str, &[NavItem]); 4] = [
    ("ボックス", &[(Tab::Box, "ボックス", true), (Tab::CheckList, "チェック\nリスト", true), (Tab::Summary, "サマリー", false)]),
    ("シミュレーション", &[(Tab::Team, "チーム編成", true), (Tab::FoodStock, "食材準備", false)]),
    ("厳選", &[(Tab::Setting, "基準生成", true), (Tab::EvaluateTable, "基準確認", false), (Tab::TmpEvaluate, "簡易診断", false)]),
    ("その他", &[(Tab::Data, "データ", false), (Tab::Faq, "FAQ", false), (Tab::History, "更新履歴", false), (Tab::Credit, "参考資料\nクレジット", true), (Tab::UiSetting, "設定", true)]),
];

/// 各画面で共有するデータ
pub struct Shared {
    pub config: Config,
    pub box_file: BoxFile,
    pub evaluate_table: Option<EvaluateTable>,
    pub ui_setting: storage::UiSetting,
    pub status: String,
}

impl Shared {
    pub fn save_config(&mut self) {
        if let Err(e) = storage::save_config(&self.config) {
            self.status = format!("設定を保存できませんでした: {e}");
        }
    }

    pub fn save_box(&mut self) {
        self.box_file.reindex();
        self.box_file.time = Some(now_iso());
        if let Err(e) = storage::save_box(&self.box_file) {
            self.status = format!("ボックスを保存できませんでした: {e}");
        }
    }

    /// 現在の設定で使える厳選テーブル
    pub fn usable_evaluate_table(&self) -> Option<&EvaluateTable> {
        self.evaluate_table.as_ref().filter(|t| t.is_enable(&self.config))
    }
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // UTC の ISO 8601(日付計算は簡易実装)
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// 1970-01-01 からの日数を年月日へ(Howard Hinnant の civil_from_days)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub struct App {
    tab: Tab,
    shared: Shared,
    box_view: BoxView,
    check_list_view: CheckListView,
    team_view: TeamView,
    evaluate_view: EvaluateView,
    ui_setting_view: UiSettingView,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let ui_setting = storage::load_ui_setting();
        ui_setting_view::apply(&cc.egui_ctx, &ui_setting);
        // 表示倍率は設定画面だけで変える(Ctrl+/- で変わると設定とずれるため)
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        style::apply(&cc.egui_ctx);
        // 見た目の確認用(デバッグ版のみ): POKESLE_DEBUG_POPUP=field などで設定ポップアップを開いて起動する。
        // 入れ子のポップアップは fix,resource のようにカンマ区切りで指定する
        #[cfg(debug_assertions)]
        if let Ok(ids) = std::env::var("POKESLE_DEBUG_POPUP") {
            for id in ids.split(',') {
                style::open_popup(&cc.egui_ctx, id);
            }
        }
        #[cfg(debug_assertions)]
        let tab = match std::env::var("POKESLE_DEBUG_TAB").as_deref() {
            Ok("team") => Tab::Team,
            Ok("checklist") => Tab::CheckList,
            Ok("setting") => Tab::Setting,
            Ok("ui") => Tab::UiSetting,
            _ => Tab::Box,
        };
        #[cfg(not(debug_assertions))]
        let tab = Tab::Box;
        App {
            tab,
            shared: Shared {
                config: storage::load_config(),
                box_file: storage::load_box(),
                evaluate_table: storage::load_evaluate_table(),
                ui_setting,
                status: String::new(),
            },
            box_view: BoxView::default(),
            check_list_view: CheckListView::default(),
            team_view: TeamView::default(),
            evaluate_view: EvaluateView::default(),
            ui_setting_view: UiSettingView::default(),
        }
    }

    /// 本家 app.vue の header
    fn header(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        style::hgradient(
            ui.painter(),
            rect,
            &[(0.0, egui::Color32::from_rgb(0x3a, 0x46, 0x5d)), (0.68, egui::Color32::from_rgb(0x52, 0x60, 0x79)), (1.0, egui::Color32::from_rgb(0x5c, 0x6b, 0x86))],
        );
        // 厳選テーブルが使えないときは、チェックリスト・基準確認を無効にし、基準生成に「!」を付ける
        let require_refresh = self.shared.usable_evaluate_table().is_none();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add_space(10.0);
            ui.label(egui::RichText::new("ポケスリシミュ").size(22.0).strong().color(egui::Color32::WHITE));
            ui.add_space(14.0);
            for (title, items) in NAV {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new(title).size(11.0).color(egui::Color32::from_rgb(0xd8, 0xdd, 0xe7)));
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        for &(tab, label, implemented) in items {
                            let disabled = !implemented || (require_refresh && matches!(tab, Tab::CheckList | Tab::EvaluateTable));
                            let caution = require_refresh && tab == Tab::Setting;
                            if nav_item(ui, label, self.tab == tab, disabled, caution, implemented).clicked() {
                                self.tab = tab;
                            }
                        }
                    });
                });
            }
        });
    }
}

/// ヘッダーのリンク1つ(白い太字、選択中は下に緑の線)
fn nav_item(ui: &mut egui::Ui, label: &str, active: bool, disabled: bool, caution: bool, implemented: bool) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let alpha = if disabled { 0.5 } else { 1.0 };
    let color = egui::Color32::WHITE.gamma_multiply(alpha);
    let galley = ui.fonts_mut(|f| f.layout_job(egui::text::LayoutJob::simple(label.to_string(), font, color, f32::INFINITY)));
    let badge = if caution { 24.0 } else { 0.0 };
    let pad = egui::vec2(5.0, 5.0);
    let height = 38.0_f32.max(galley.size().y + pad.y * 2.0);
    let size = egui::vec2(galley.size().x + pad.x * 2.0 + badge, height);
    let sense = if disabled { egui::Sense::hover() } else { egui::Sense::click() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let p = ui.painter();
    if !disabled && (active || response.hovered()) {
        p.rect_filled(rect, 0.0, egui::Color32::from_white_alpha(31));
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 3.0), rect.right_bottom()), 0.0, egui::Color32::from_rgb(0x7a, 0xc6, 0xb4));
    }
    // 本家は太字。少しずらして重ね描きする
    let pos = egui::pos2(rect.left() + pad.x, rect.center().y - galley.size().y / 2.0);
    p.galley(pos + egui::vec2(0.5, 0.0), galley.clone(), color);
    p.galley(pos, galley, color);
    if caution {
        let c = egui::pos2(rect.right() - pad.x - 10.0, rect.center().y);
        p.circle(c, 10.0, style::ACCENT, egui::Stroke::new(2.0, egui::Color32::WHITE));
        p.text(c, egui::Align2::CENTER_CENTER, "!", egui::FontId::proportional(12.0), egui::Color32::WHITE);
    }
    if !implemented {
        return response.on_hover_text("未実装");
    }
    if disabled {
        return response.on_hover_text("厳選テーブルの作成が必要です(基準生成)");
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header").frame(egui::Frame::new()).show(ui, |ui| self.header(ui));
        if !self.shared.status.is_empty() {
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.shared.status);
                    if ui.small_button("×").clicked() {
                        self.shared.status.clear();
                    }
                });
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(style::SURFACE_SUBTLE).inner_margin(10)).show(ui, |ui| match self.tab {
            Tab::Box => self.box_view.ui(ui, &mut self.shared),
            Tab::CheckList => self.check_list_view.ui(ui, &mut self.shared),
            Tab::Team => {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.team_view.ui(ui, &mut self.shared));
            }
            Tab::Setting => self.evaluate_view.ui(ui, &mut self.shared),
            Tab::Credit => credit_view::ui(ui),
            Tab::UiSetting => self.ui_setting_view.ui(ui, &mut self.shared.ui_setting, &mut self.shared.status),
            _ => {
                ui.label("未実装です");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
        assert_eq!(super::civil_from_days(20734), (2026, 10, 8));
    }
}
