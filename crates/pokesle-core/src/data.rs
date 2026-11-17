//! マスターデータ。
//!
//! `data/master.json` は本家(upstream/)の `src/data/` を Node で読み込んで書き出したもの
//! (`tools/upstream-bridge/export-data.mjs`)。派生値も本家の計算結果をそのまま使う。

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

static MASTER_JSON: &str = include_str!("../data/master.json");
static MASTER: OnceLock<Master> = OnceLock::new();

/// マスターデータ全体を返す。初回呼び出し時に読み込む。
pub fn master() -> &'static Master {
    MASTER.get_or_init(|| Master::load(MASTER_JSON).expect("master.json の読み込みに失敗しました"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Specialty {
    #[serde(rename = "きのみ")]
    Berry,
    #[serde(rename = "食材")]
    Food,
    #[serde(rename = "スキル")]
    Skill,
    #[serde(rename = "オール")]
    All,
}

impl Specialty {
    pub fn name(self) -> &'static str {
        match self {
            Specialty::Berry => "きのみ",
            Specialty::Food => "食材",
            Specialty::Skill => "スキル",
            Specialty::All => "オール",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "きのみ" => Some(Specialty::Berry),
            "食材" => Some(Specialty::Food),
            "スキル" => Some(Specialty::Skill),
            "オール" => Some(Specialty::All),
            _ => None,
        }
    }

    /// きのみとくい・オールなら true
    pub fn is_berry_like(self) -> bool {
        matches!(self, Specialty::Berry | Specialty::All)
    }

    /// 食材とくい・オールなら true
    pub fn is_food_like(self) -> bool {
        matches!(self, Specialty::Food | Specialty::All)
    }

    /// スキルとくい・オールなら true
    pub fn is_skill_like(self) -> bool {
        matches!(self, Specialty::Skill | Specialty::All)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Berry {
    pub name: String,
    pub energy: f64,
    #[serde(rename = "type")]
    pub type_name: String,
    pub type_color: String,
    pub color_order: i32,
    pub img: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Food {
    pub name: String,
    pub energy: f64,
    pub img: Option<String>,
    pub best_rate: f64,
    pub best_cooking: Option<String>,
    pub best_type_rate: HashMap<String, f64>,
    pub max_energy_by_type: HashMap<String, f64>,
    pub max_energy: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoodStats {
    pub max_energy: f64,
    pub average_energy: f64,
    pub average_max_cooked_energy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum CookingType {
    #[serde(rename = "カレー")]
    Curry,
    #[serde(rename = "サラダ")]
    Salad,
    #[serde(rename = "デザート")]
    Dessert,
}

impl CookingType {
    pub const ALL: [CookingType; 3] = [CookingType::Curry, CookingType::Salad, CookingType::Dessert];

    pub fn name(self) -> &'static str {
        match self {
            CookingType::Curry => "カレー",
            CookingType::Salad => "サラダ",
            CookingType::Dessert => "デザート",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "カレー" => Some(CookingType::Curry),
            "サラダ" => Some(CookingType::Salad),
            "デザート" => Some(CookingType::Dessert),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookingFood {
    pub name: String,
    pub num: f64,
    #[serde(skip)]
    pub food: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cooking {
    #[serde(rename = "type")]
    pub cooking_type: CookingType,
    pub name: String,
    pub energy: f64,
    pub food_list: Vec<CookingFood>,
    pub food_num: f64,
    pub raw_energy: f64,
    pub rate: f64,
    pub max_energy: f64,
    pub add_energy: f64,
    pub max_add_energy: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeLv {
    pub lv: usize,
    pub bonus: f64,
    pub total_exp: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookingStats {
    pub recipe_lvs: Vec<RecipeLv>,
    pub max_recipe_lv: usize,
    pub max_recipe_bonus: f64,
    pub pot_max: f64,
    pub max_food_num: f64,
    pub max_energy: f64,
    pub cooking_power_up_energy: f64,
    pub cooking_power_up_energy_average: f64,
    pub cooking_power_up_energy_map: HashMap<String, f64>,
}

impl CookingStats {
    /// レシピレベルのボーナス倍率。範囲外は本家同様 0。
    pub fn recipe_bonus(&self, lv: usize) -> f64 {
        lv.checked_sub(1)
            .and_then(|i| self.recipe_lvs.get(i))
            .map(|x| x.bonus)
            .unwrap_or(0.0)
    }
}

/// 計算処理で個別に扱うメインスキル。新しいスキルは本家の `pokemon-simulator.ts` の
/// switch 文に合わせてここへ追加する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    EnergyChargeS,
    EnergyChargeSRandom,
    Takuwaeru,
    EnergyChargeM,
    BerryBurst,
    Bakenokawa,
    Nightmare,
    Ryuseigun,
    PsychoBreak,
    FoodGetS,
    Plus,
    Present,
    BuildUp,
    FoodSelectS,
    Kyouun,
    Kairikibasami,
    CookingPowerUpS,
    Minus,
    CookingChanceS,
    MagicalFlame,
    HelpSupportS,
    HelpBoost,
    GenkiChargeS,
    Tsukinohikari,
    GenkiYellS,
    Hoppesurisuri,
    Iyashinohadou,
    GenkiAllS,
    BerryJuice,
    Mikazukinoinori,
    ShardGetS,
    ShardGetSRandom,
    Hadoudan,
    Henshin,
    Monomane,
    Metronome,
    /// 未対応のスキル。計算時に本家同様エラーにする。
    Unknown,
}

impl SkillKind {
    pub fn from_name(name: &str) -> SkillKind {
        use SkillKind::*;
        match name {
            "エナジーチャージS" => EnergyChargeS,
            "エナジーチャージS(ランダム)" => EnergyChargeSRandom,
            "たくわえる(エナジーチャージS)" => Takuwaeru,
            "エナジーチャージM" => EnergyChargeM,
            "きのみバースト" => BerryBurst,
            "ばけのかわ(きのみバースト)" => Bakenokawa,
            "ナイトメア(エナジーチャージM)" => Nightmare,
            "りゅうせいぐん(きのみバースト)" => Ryuseigun,
            "サイコブレイク(きのみゾーン)" => PsychoBreak,
            "食材ゲットS" => FoodGetS,
            "プラス(食材ゲットS)" => Plus,
            "プレゼント(食材ゲットS)" => Present,
            "ビルドアップ(料理アシストS)" => BuildUp,
            "食材セレクトS" => FoodSelectS,
            "きょううん(食材セレクトS)" => Kyouun,
            "かいりきバサミ(食材セレクトS)" => Kairikibasami,
            "料理パワーアップS" => CookingPowerUpS,
            "マイナス(料理パワーアップS)" => Minus,
            "料理チャンスS" => CookingChanceS,
            "マジカルフレイム(料理アドバンスS)" => MagicalFlame,
            "おてつだいサポートS" => HelpSupportS,
            "おてつだいブースト" => HelpBoost,
            "げんきチャージS" => GenkiChargeS,
            "つきのひかり(げんきチャージS)" => Tsukinohikari,
            "げんきエールS" => GenkiYellS,
            "ほっぺすりすり(げんきエールS)" => Hoppesurisuri,
            "いやしのはどう(げんきエールS)" => Iyashinohadou,
            "げんきオールS" => GenkiAllS,
            "きのみジュース(げんきオールS)" => BerryJuice,
            "みかづきのいのり(げんきオールS)" => Mikazukinoinori,
            "ゆめのかけらゲットS" => ShardGetS,
            "ゆめのかけらゲットS(ランダム)" => ShardGetSRandom,
            "はどうだん(ゆめのかけらゲットS)" => Hadoudan,
            "へんしん(スキルコピー)" => Henshin,
            "ものまね(スキルコピー)" => Monomane,
            "ゆびをふる" => Metronome,
            _ => Unknown,
        }
    }

    pub fn is_skill_copy(self) -> bool {
        matches!(self, SkillKind::Henshin | SkillKind::Monomane)
    }
}

/// `{ self, other }` の組
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SelfOther {
    pub self_: f64,
    pub other: f64,
}

/// スキル効果の `team` は、きのみバースト系では `{self, other}` の配列、
/// おてつだいブーストでは数値配列になっている。
#[derive(Debug, Clone, PartialEq)]
pub enum TeamEffect {
    Pairs(Vec<SelfOther>),
    Nums(Vec<f64>),
}

/// スキル効果1レベル分。本家では数値・オブジェクト・null が混在しているため、
/// 使われうるキーをすべて Option で持つ。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillEffect {
    /// 効果が数値そのものの場合
    pub value: Option<f64>,
    pub base: Option<f64>,
    pub expect: Option<f64>,
    pub self_: Option<f64>,
    pub other: Option<f64>,
    pub energy: Option<f64>,
    pub team: Option<TeamEffect>,
    pub bonus: Option<f64>,
    pub zone: Option<f64>,
    pub main: Option<f64>,
    pub sub: Option<f64>,
    pub food: Option<f64>,
    pub shard: Option<f64>,
    pub fix: Option<f64>,
    pub max: Option<f64>,
    pub help1: Option<f64>,
    pub help2: Option<f64>,
}

impl SkillEffect {
    fn from_value(v: &Value) -> Result<SkillEffect, String> {
        let mut e = SkillEffect::default();
        match v {
            Value::Null => {}
            Value::Number(n) => e.value = n.as_f64(),
            Value::Object(map) => {
                for (k, v) in map {
                    let num = || v.as_f64().ok_or_else(|| format!("スキル効果 {k} が数値ではありません"));
                    match k.as_str() {
                        "base" => e.base = Some(num()?),
                        "expect" => e.expect = Some(num()?),
                        "self" => e.self_ = Some(num()?),
                        "other" => e.other = Some(num()?),
                        "energy" => e.energy = Some(num()?),
                        "bonus" => e.bonus = Some(num()?),
                        "zone" => e.zone = Some(num()?),
                        "main" => e.main = Some(num()?),
                        "sub" => e.sub = Some(num()?),
                        "food" => e.food = Some(num()?),
                        "shard" => e.shard = Some(num()?),
                        "fix" => e.fix = Some(num()?),
                        "max" => e.max = Some(num()?),
                        "help1" => e.help1 = Some(num()?),
                        "help2" => e.help2 = Some(num()?),
                        "team" => {
                            let list = v.as_array().ok_or("team が配列ではありません")?;
                            if list.iter().all(|x| x.is_number()) {
                                e.team = Some(TeamEffect::Nums(list.iter().filter_map(|x| x.as_f64()).collect()));
                            } else {
                                let mut pairs = Vec::new();
                                for item in list {
                                    pairs.push(SelfOther {
                                        self_: item.get("self").and_then(Value::as_f64).unwrap_or(0.0),
                                        other: item.get("other").and_then(Value::as_f64).unwrap_or(0.0),
                                    });
                                }
                                e.team = Some(TeamEffect::Pairs(pairs));
                            }
                        }
                        _ => return Err(format!("未知のスキル効果キー: {k}")),
                    }
                }
            }
            _ => return Err(format!("未知のスキル効果: {v}")),
        }
        Ok(e)
    }

    pub fn value(&self) -> f64 {
        self.value.unwrap_or(f64::NAN)
    }

    pub fn team_pair(&self, index: usize) -> SelfOther {
        match &self.team {
            Some(TeamEffect::Pairs(list)) => list.get(index).copied().unwrap_or(SelfOther { self_: f64::NAN, other: f64::NAN }),
            _ => SelfOther { self_: f64::NAN, other: f64::NAN },
        }
    }

    pub fn team_last_pair(&self) -> SelfOther {
        match &self.team {
            Some(TeamEffect::Pairs(list)) if !list.is_empty() => list[list.len() - 1],
            _ => SelfOther { self_: f64::NAN, other: f64::NAN },
        }
    }

    pub fn team_num(&self, index: usize) -> f64 {
        match &self.team {
            Some(TeamEffect::Nums(list)) => list.get(index).copied().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub index: usize,
    pub name: String,
    pub kind: SkillKind,
    pub category: String,
    pub effect: Vec<SkillEffect>,
    pub metronome: Option<bool>,
    pub copyable: Option<bool>,
    pub team: bool,
    pub success: Option<f64>,
    pub evaluate_energy: Option<Vec<f64>>,
    pub genki: bool,
    pub shard: bool,
    pub energy_only: bool,
    /// 獲得する食材の固定リスト(食材のインデックス)
    pub food_list: Option<Vec<usize>>,
    pub skill_specialty_only: bool,
    pub self_heal: bool,
}

impl Skill {
    /// 本家の `skill.effect.length`
    pub fn max_lv(&self) -> usize {
        self.effect.len()
    }

    /// 本家の `copyable` は未指定なら undefined(偽)
    pub fn is_copyable(&self) -> bool {
        self.copyable.unwrap_or(false)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSkill {
    name: String,
    category: String,
    effect: Vec<Value>,
    metronome: Option<bool>,
    copyable: Option<bool>,
    #[serde(default)]
    team: bool,
    success: Option<f64>,
    evaluate_energy: Option<Vec<f64>>,
    #[serde(default)]
    genki: bool,
    #[serde(default)]
    shard: bool,
    #[serde(default)]
    energy_only: bool,
    food_list: Option<Vec<String>>,
    #[serde(default)]
    skill_specialty_only: bool,
    #[serde(default)]
    self_heal: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillCategory {
    pub id: String,
    pub name: String,
}

/// 計算処理で個別に扱うサブスキル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubSkillKind {
    BerryNumS,
    GenkiBonus,
    ShardBonus,
    ResearchExpBonus,
    SleepExpBonus,
    HelpBonus,
    SkillLvM,
    SkillRateM,
    FoodRateM,
    SkillLvS,
    SpeedM,
    BagL,
    BagM,
    SkillRateS,
    FoodRateS,
    SpeedS,
    BagS,
    Unknown,
}

impl SubSkillKind {
    pub fn from_name(name: &str) -> SubSkillKind {
        use SubSkillKind::*;
        match name {
            "きのみの数S" => BerryNumS,
            "げんき回復ボーナス" => GenkiBonus,
            "ゆめのかけらボーナス" => ShardBonus,
            "リサーチEXPボーナス" => ResearchExpBonus,
            "睡眠EXPボーナス" => SleepExpBonus,
            "おてつだいボーナス" => HelpBonus,
            "スキルレベルアップM" => SkillLvM,
            "スキル確率アップM" => SkillRateM,
            "食材確率アップM" => FoodRateM,
            "スキルレベルアップS" => SkillLvS,
            "おてつだいスピードM" => SpeedM,
            "最大所持数アップL" => BagL,
            "最大所持数アップM" => BagM,
            "スキル確率アップS" => SkillRateS,
            "食材確率アップS" => FoodRateS,
            "おてつだいスピードS" => SpeedS,
            "最大所持数アップS" => BagS,
            _ => Unknown,
        }
    }

    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubSkill {
    pub id: u32,
    pub name: String,
    pub rarity: u32,
    pub short: String,
    pub katakana: String,
    pub next: Option<String>,
    pub input_sort: i32,
    #[serde(skip, default = "unknown_sub_skill")]
    pub kind: SubSkillKind,
}

fn unknown_sub_skill() -> SubSkillKind {
    SubSkillKind::Unknown
}

/// 性格で上下する能力
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NatureStat {
    Speed,
    Genki,
    FoodRate,
    SkillRate,
    Exp,
    Unknown,
}

impl NatureStat {
    fn from_name(name: &str) -> NatureStat {
        match name {
            "手伝いスピード" => NatureStat::Speed,
            "げんき回復量" => NatureStat::Genki,
            "食材お手伝い確率" => NatureStat::FoodRate,
            "メインスキル発生確率" => NatureStat::SkillRate,
            "EXP獲得量" => NatureStat::Exp,
            _ => NatureStat::Unknown,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nature {
    pub name: String,
    pub good: Option<String>,
    pub weak: Option<String>,
    pub good_short: Option<String>,
    pub weak_short: Option<String>,
    #[serde(skip)]
    pub good_stat: Option<NatureStat>,
    #[serde(skip)]
    pub weak_stat: Option<NatureStat>,
}

impl Nature {
    /// 上昇補正なら `good`、下降補正なら `weak`、それ以外は `neutral`
    pub fn pick(nature: Option<&Nature>, stat: NatureStat, good: f64, weak: f64, neutral: f64) -> f64 {
        match nature {
            Some(n) if n.good_stat == Some(stat) => good,
            Some(n) if n.weak_stat == Some(stat) => weak,
            _ => neutral,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExBuff {
    pub speed: Option<f64>,
    #[serde(rename = "skillLv")]
    pub skill_lv: Option<f64>,
    pub bag: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExBuffs {
    #[serde(rename = "match")]
    pub match_: ExBuff,
    pub not_match: ExBuff,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub berry_list: Option<Vec<String>>,
    pub berry_option_list: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub ex: bool,
    pub ex_buff: Option<ExBuffs>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpRow {
    pub total: f64,
    /// 最終レベルは null
    pub shard: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightCapPikachuRow {
    pub time: f64,
    pub berry: f64,
    pub food_num_list: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PokemonFood {
    pub name: String,
    pub num_list: Vec<Option<f64>>,
    #[serde(skip)]
    pub food: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evolve {
    pub before: Option<String>,
    pub lv: Option<f64>,
    pub candy: Option<f64>,
    pub item_list: Vec<String>,
    pub sleep: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PokemonField {
    pub name: String,
    #[serde(rename = "type")]
    pub sleep_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pokemon {
    pub name: String,
    pub no: u32,
    pub order: usize,
    pub specialty: Specialty,
    #[serde(rename = "berry")]
    pub berry_name: String,
    #[serde(rename = "skill")]
    pub skill_name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub food_list: Vec<PokemonFood>,
    pub food_name_list: Vec<String>,
    pub help: f64,
    pub bag: f64,
    pub food_rate: f64,
    pub skill_rate: f64,
    pub legend: bool,
    pub kaihou: bool,
    pub exp: f64,
    pub candy_name: String,
    pub evolve: Evolve,
    pub field_list: Vec<PokemonField>,
    pub fp: Option<f64>,
    pub evolve_lv: i32,
    pub remain_evolve_lv: i32,
    pub after_list: Vec<String>,
    pub evolve_candy_map: HashMap<String, Option<f64>>,
    pub require_sleep: HashMap<String, bool>,
    pub seed: Option<String>,
    #[serde(default)]
    pub is_last: bool,
    #[serde(skip)]
    pub index: usize,
    #[serde(skip)]
    pub berry: usize,
    #[serde(skip)]
    pub skill: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    pub history: u64,
    pub evaluate: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMaster {
    upstream_commit: String,
    version: VersionInfo,
    berries: Vec<Berry>,
    berry_type_order: Vec<String>,
    foods: Vec<Food>,
    food_stats: FoodStats,
    cookings: Vec<Cooking>,
    cooking_stats: CookingStats,
    skill_categories: Vec<SkillCategory>,
    skills: Vec<RawSkill>,
    metronome_target: Vec<String>,
    sub_skills: Vec<SubSkill>,
    natures: Vec<Nature>,
    fields: Vec<Field>,
    exp: Vec<ExpRow>,
    night_cap_pikachu: Vec<NightCapPikachuRow>,
    pokemon: Vec<Pokemon>,
}

#[derive(Debug)]
pub struct Master {
    pub upstream_commit: String,
    pub version: VersionInfo,
    pub berries: Vec<Berry>,
    pub berry_type_order: Vec<String>,
    pub foods: Vec<Food>,
    pub food_stats: FoodStats,
    pub cookings: Vec<Cooking>,
    pub cooking_stats: CookingStats,
    pub skill_categories: Vec<SkillCategory>,
    pub skills: Vec<Skill>,
    /// ゆびをふるの対象スキル(スキルのインデックス)
    pub metronome_target: Vec<usize>,
    pub sub_skills: Vec<SubSkill>,
    pub natures: Vec<Nature>,
    pub fields: Vec<Field>,
    pub exp: Vec<ExpRow>,
    pub night_cap_pikachu: Vec<NightCapPikachuRow>,
    pub pokemon: Vec<Pokemon>,

    berry_map: HashMap<String, usize>,
    food_map: HashMap<String, usize>,
    cooking_map: HashMap<String, usize>,
    skill_map: HashMap<String, usize>,
    sub_skill_map: HashMap<String, usize>,
    sub_skill_id_map: HashMap<u32, usize>,
    nature_map: HashMap<String, usize>,
    field_map: HashMap<String, usize>,
    pokemon_map: HashMap<String, usize>,
}

fn index_map<T>(list: &[T], key: impl Fn(&T) -> &str) -> HashMap<String, usize> {
    list.iter().enumerate().map(|(i, x)| (key(x).to_string(), i)).collect()
}

impl Master {
    pub fn load(json: &str) -> Result<Master, String> {
        let raw: RawMaster = serde_json::from_str(json).map_err(|e| e.to_string())?;

        let berry_map = index_map(&raw.berries, |x| &x.name);
        let food_map = index_map(&raw.foods, |x| &x.name);
        let food_index = |name: &str| food_map.get(name).copied().ok_or_else(|| format!("未知の食材: {name}"));

        let mut cookings = raw.cookings;
        for cooking in &mut cookings {
            for f in &mut cooking.food_list {
                f.food = food_index(&f.name)?;
            }
        }
        let cooking_map = index_map(&cookings, |x| &x.name);

        let mut skills = Vec::with_capacity(raw.skills.len());
        for (index, s) in raw.skills.into_iter().enumerate() {
            let effect = s.effect.iter().map(SkillEffect::from_value).collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("{}: {e}", s.name))?;
            let food_list = match s.food_list {
                Some(list) => Some(list.iter().map(|n| food_index(n)).collect::<Result<Vec<_>, _>>()?),
                None => None,
            };
            skills.push(Skill {
                index,
                kind: SkillKind::from_name(&s.name),
                name: s.name,
                category: s.category,
                effect,
                metronome: s.metronome,
                copyable: s.copyable,
                team: s.team,
                success: s.success,
                evaluate_energy: s.evaluate_energy,
                genki: s.genki,
                shard: s.shard,
                energy_only: s.energy_only,
                food_list,
                skill_specialty_only: s.skill_specialty_only,
                self_heal: s.self_heal,
            });
        }
        let skill_map = index_map(&skills, |x| &x.name);
        let metronome_target = raw.metronome_target.iter()
            .map(|n| skill_map.get(n).copied().ok_or_else(|| format!("未知のスキル: {n}")))
            .collect::<Result<Vec<_>, _>>()?;

        let mut sub_skills = raw.sub_skills;
        for s in &mut sub_skills {
            s.kind = SubSkillKind::from_name(&s.name);
        }
        let sub_skill_map = index_map(&sub_skills, |x| &x.name);
        let sub_skill_id_map = sub_skills.iter().enumerate().map(|(i, x)| (x.id, i)).collect();

        let mut natures = raw.natures;
        for n in &mut natures {
            n.good_stat = n.good.as_deref().map(NatureStat::from_name);
            n.weak_stat = n.weak.as_deref().map(NatureStat::from_name);
        }
        let nature_map = index_map(&natures, |x| &x.name);
        let field_map = index_map(&raw.fields, |x| &x.name);

        let mut pokemon = raw.pokemon;
        for (i, p) in pokemon.iter_mut().enumerate() {
            p.index = i;
            p.berry = *berry_map.get(&p.berry_name).ok_or_else(|| format!("{}: 未知のきのみ", p.name))?;
            p.skill = *skill_map.get(&p.skill_name).ok_or_else(|| format!("{}: 未知のスキル", p.name))?;
            for f in &mut p.food_list {
                f.food = food_index(&f.name)?;
            }
        }
        let pokemon_map = index_map(&pokemon, |x| &x.name);

        Ok(Master {
            upstream_commit: raw.upstream_commit,
            version: raw.version,
            berries: raw.berries,
            berry_type_order: raw.berry_type_order,
            foods: raw.foods,
            food_stats: raw.food_stats,
            cookings,
            cooking_stats: raw.cooking_stats,
            skill_categories: raw.skill_categories,
            skills,
            metronome_target,
            sub_skills,
            natures,
            fields: raw.fields,
            exp: raw.exp,
            night_cap_pikachu: raw.night_cap_pikachu,
            pokemon,
            berry_map,
            food_map,
            cooking_map,
            skill_map,
            sub_skill_map,
            sub_skill_id_map,
            nature_map,
            field_map,
            pokemon_map,
        })
    }

    pub fn berry(&self, name: &str) -> Option<&Berry> {
        self.berry_map.get(name).map(|&i| &self.berries[i])
    }
    pub fn food_index(&self, name: &str) -> Option<usize> {
        self.food_map.get(name).copied()
    }
    pub fn food(&self, name: &str) -> Option<&Food> {
        self.food_index(name).map(|i| &self.foods[i])
    }
    pub fn cooking(&self, name: &str) -> Option<&Cooking> {
        self.cooking_map.get(name).map(|&i| &self.cookings[i])
    }
    pub fn skill(&self, name: &str) -> Option<&Skill> {
        self.skill_map.get(name).map(|&i| &self.skills[i])
    }
    pub fn sub_skill(&self, name: &str) -> Option<&SubSkill> {
        self.sub_skill_map.get(name).map(|&i| &self.sub_skills[i])
    }
    pub fn sub_skill_by_id(&self, id: u32) -> Option<&SubSkill> {
        self.sub_skill_id_map.get(&id).map(|&i| &self.sub_skills[i])
    }
    pub fn nature(&self, name: &str) -> Option<&Nature> {
        self.nature_map.get(name).map(|&i| &self.natures[i])
    }
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.field_map.get(name).map(|&i| &self.fields[i])
    }
    pub fn pokemon(&self, name: &str) -> Option<&Pokemon> {
        self.pokemon_map.get(name).map(|&i| &self.pokemon[i])
    }

    pub fn pokemon_berry(&self, p: &Pokemon) -> &Berry {
        &self.berries[p.berry]
    }
    pub fn pokemon_skill(&self, p: &Pokemon) -> &Skill {
        &self.skills[p.skill]
    }

    /// 銀種を使った場合のサブスキル(本家の `SubSkill.useSilverSeed`)
    pub fn use_silver_seed(&self, sub_skill_list: &[String], setting: Option<&HashMap<String, bool>>) -> Vec<String> {
        let mut tmp = sub_skill_list.join("/");
        let enabled = |name: &str| setting.is_none_or(|s| s.get(name).copied().unwrap_or(false));
        let mut step = |from: &str, to: &str| {
            if tmp.contains(from) && enabled(from) && !tmp.contains(to) {
                tmp = tmp.replacen(from, to, 1);
            }
        };
        step("最大所持数アップM", "最大所持数アップL");
        step("最大所持数アップS", "最大所持数アップM");
        step("最大所持数アップM", "最大所持数アップL");
        step("スキルレベルアップS", "スキルレベルアップM");
        step("スキル確率アップS", "スキル確率アップM");
        step("食材確率アップS", "食材確率アップM");
        step("おてつだいスピードS", "おてつだいスピードM");
        tmp.split('/').map(str::to_string).collect()
    }
}

/// きのみ1個のエナジー(レベル補正後)
pub fn berry_energy_at(base: f64, lv: f64) -> f64 {
    (base + lv - 1.0).max(base * 1.025_f64.powf(lv - 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_master() {
        let m = master();
        assert!(m.pokemon.len() > 200);
        let skills: Vec<&str> = m.skills.iter().filter(|s| s.kind == SkillKind::Unknown).map(|s| s.name.as_str()).collect();
        assert!(skills.is_empty(), "未対応のスキル(SkillKind と計算処理の追加が必要): {skills:?}");
        let subs: Vec<&str> = m.sub_skills.iter().filter(|s| s.kind == SubSkillKind::Unknown).map(|s| s.name.as_str()).collect();
        assert!(subs.is_empty(), "未対応のサブスキル(SubSkillKind と計算処理の追加が必要): {subs:?}");
        let natures: Vec<&str> = m
            .natures
            .iter()
            .filter(|n| n.good_stat == Some(NatureStat::Unknown) || n.weak_stat == Some(NatureStat::Unknown))
            .map(|n| n.name.as_str())
            .collect();
        assert!(natures.is_empty(), "未対応の性格補正(NatureStat の追加が必要): {natures:?}");
    }
}
