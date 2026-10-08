//! アプリ全体の状態と画面切り替え

use eframe::egui;
use pokesle_core::box_io::BoxFile;
use pokesle_core::config::Config;
use pokesle_core::evaluate_table::EvaluateTable;

use crate::storage;
use crate::views::{box_view::BoxView, check_list_view::CheckListView, credit_view, settings_view::SettingsView, team_view::TeamView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Box,
    Team,
    CheckList,
    Settings,
    Credit,
}

/// 各画面で共有するデータ
pub struct Shared {
    pub config: Config,
    pub box_file: BoxFile,
    pub evaluate_table: Option<EvaluateTable>,
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
    team_view: TeamView,
    check_list_view: CheckListView,
    settings_view: SettingsView,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        crate::fonts::install(&cc.egui_ctx);
        App {
            tab: Tab::Box,
            shared: Shared {
                config: storage::load_config(),
                box_file: storage::load_box(),
                evaluate_table: storage::load_evaluate_table(),
                status: String::new(),
            },
            box_view: BoxView::default(),
            team_view: TeamView::default(),
            check_list_view: CheckListView::default(),
            settings_view: SettingsView::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ポケスリシミュレーター");
                ui.separator();
                for (tab, label) in [(Tab::Box, "ボックス"), (Tab::Team, "チーム編成"), (Tab::CheckList, "チェックリスト"), (Tab::Settings, "設定"), (Tab::Credit, "クレジット")] {
                    ui.selectable_value(&mut self.tab, tab, label);
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("ボックス {}匹", self.shared.box_file.list.len()));
                ui.separator();
                let table = match (&self.shared.evaluate_table, self.shared.usable_evaluate_table()) {
                    (_, Some(_)) => "厳選テーブル: 作成済み",
                    (Some(_), None) => "厳選テーブル: 設定変更により再作成が必要",
                    (None, None) => "厳選テーブル: 未作成",
                };
                ui.label(table);
                if !self.shared.status.is_empty() {
                    ui.separator();
                    ui.label(&self.shared.status);
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Box => self.box_view.ui(ui, &mut self.shared),
            Tab::Team => self.team_view.ui(ui, &mut self.shared),
            Tab::CheckList => self.check_list_view.ui(ui, &mut self.shared),
            Tab::Settings => self.settings_view.ui(ui, &mut self.shared),
            Tab::Credit => credit_view::ui(ui),
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
