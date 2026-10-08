//! 育成に必要なアメ・ゆめのかけら(本家 `data/exp.ts` の `calcRequireInfo`)
//!
//! 計算結果を一致させるため、本家の仕様もそのまま再現している:
//! - 所持アメは `candy.bag[種ポケモン名]` を見る(アメ名ではない)。未設定なら比較は常に偽
//! - 最適ブーストの探索ではポケモンごとの経験値倍率を掛けない

use crate::config::CandyConfig;
use crate::data::{master, Nature};
use crate::js;
use crate::sim_pokemon::BoxPokemon;

/// 目標Lvまでに必要なアメ・ゆめのかけら。None は本家の null
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequireInfo {
    pub normal_candy_num: Option<f64>,
    pub normal_candy_shard: Option<f64>,
    pub boost_candy_num: Option<f64>,
    pub boost_candy_shard: Option<f64>,
    pub best_boost_candy_num: Option<f64>,
    pub best_boost_candy_shard: Option<f64>,
    pub best_normal_candy_num: Option<f64>,
    pub best_normal_candy_shard: Option<f64>,
}

/// JS の真偽判定(null/0/NaN が偽)
fn truthy(x: Option<f64>) -> bool {
    x.is_some_and(|x| x != 0.0 && !x.is_nan())
}

/// null + x(null は 0 として足す)
fn add(a: &mut Option<f64>, x: f64) {
    *a = Some(a.unwrap_or(0.0) + x);
}

/// 本家 `Exp.calcRequireInfo(pokemon, nature, config)`
pub fn calc_require_info(pokemon: &BoxPokemon, nature: Option<&Nature>, candy: &CandyConfig) -> RequireInfo {
    let m = master();
    let list = &m.exp;
    let len = list.len();
    let mut result = RequireInfo::default();

    let Some(training) = pokemon.training else { return result };
    if !(pokemon.lv < training && pokemon.lv < len as f64 && 1.0 < training) {
        return result;
    }
    let Some(info) = m.pokemon(&pokemon.name) else { return result };
    let mut lv = pokemon.lv.min(len as f64) as usize;
    if lv == 0 {
        return result;
    }
    let total = |i: usize| list.get(i).map_or(f64::NAN, |x| x.total);
    let shard = |i: usize| list.get(i).and_then(|x| x.shard).unwrap_or(0.0);

    // 次のレベルまでの経験値
    let mut next_total = js::round(total(lv) * info.exp);
    // 今ある経験値(次までの経験値があるならそれを使って計算、そうでないなら現Lvの経験値)
    let mut normal_total = match pokemon.next_exp {
        Some(n) if n != 0.0 && !n.is_nan() => next_total - n,
        _ => js::round(total(lv - 1) * info.exp),
    };
    let mut boost_total = normal_total;
    let mut min_boost_total = normal_total;

    // アメ1個あたりの経験値
    let good = nature.and_then(|n| n.good.as_deref()) == Some("EXP獲得量");
    let weak = nature.and_then(|n| n.weak.as_deref()) == Some("EXP獲得量");
    let pick = |g: f64, w: f64, n: f64| if good { g } else if weak { w } else { n };
    let candy_exp25 = pick(47.0, 33.0, 40.0);
    let candy_exp30 = pick(41.0, 29.0, 35.0);
    let candy_exp99 = pick(30.0, 21.0, 25.0);

    let bm = candy.boost_multiply;
    let bag = info.seed.as_ref().and_then(|s| candy.bag.get(s).copied());

    // アメのEXPが変わる区間のすべてをブーストするパターンから、すべてブーストしないパターンまで走査
    let mut min_boost_candy_num = 0.0;
    let mut tmp_min_boost_candy_num: Option<f64> = None;
    loop {
        let mut tmp_lv = lv as f64;
        let mut tmp_exp = normal_total;
        let mut use_candy = 0.0;
        let mut use_boost = 0.0;
        for (exp, border) in [(candy_exp25, 25.0), (candy_exp30, 30.0), (candy_exp99, len as f64)] {
            if border <= tmp_lv {
                continue;
            }
            if tmp_lv < training {
                let target_lv = js::min(training, border);
                let target_total = total(target_lv as usize - 1);
                if let Some(tmp_min) = tmp_min_boost_candy_num
                    && use_boost < tmp_min
                {
                    let this_candy = js::min(((target_total - tmp_exp) / (exp * bm)).ceil(), tmp_min - use_boost);
                    tmp_exp += this_candy * exp * bm;
                    use_candy += this_candy;
                    use_boost += this_candy;
                }
                let this_candy = ((target_total - tmp_exp) / exp).ceil();
                tmp_exp += this_candy * exp;
                tmp_lv = target_lv;
                use_candy += this_candy;
            } else {
                break;
            }
        }

        match tmp_min_boost_candy_num {
            None => {
                if bag.is_some_and(|b| use_candy > b) {
                    // 最小ブースト数: 足りない分を補えるだけ、もしくは必要アメ数/倍率の小さい方
                    let b = bag.unwrap_or(0.0);
                    tmp_min_boost_candy_num = Some(js::min(js::min(((use_candy - b) / (bm - 1.0)).ceil(), (use_candy / bm).ceil()), b));
                } else {
                    result.best_boost_candy_num = None;
                    result.best_normal_candy_num = Some(use_candy);
                    break;
                }
            }
            Some(tmp_min) => {
                if bag.is_some_and(|b| use_candy <= b) {
                    result.best_boost_candy_num = Some(tmp_min);
                    result.best_normal_candy_num = Some(use_candy - tmp_min);
                    min_boost_candy_num = tmp_min;
                    tmp_min_boost_candy_num = Some(tmp_min - 1.0);
                } else {
                    break;
                }
            }
        }
    }

    let can_min_boost = truthy(result.best_boost_candy_num) && truthy(result.best_normal_candy_num);
    if can_min_boost {
        result.best_boost_candy_shard = Some(0.0);
        result.best_normal_candy_shard = Some(0.0);
    } else {
        result.best_boost_candy_num = None;
        result.best_boost_candy_shard = None;
        result.best_normal_candy_num = None;
        result.best_normal_candy_shard = None;
    }

    result.normal_candy_num = Some(0.0);
    result.normal_candy_shard = Some(0.0);

    while (lv as f64) < training && truthy(Some(next_total)) {
        let candy_exp = if lv < 25 {
            candy_exp25
        } else if lv < 30 {
            candy_exp30
        } else {
            candy_exp99
        };
        let boosted_candy_exp = candy_exp * bm;
        let lv_shard = shard(lv - 1);

        let require_candy_num = ((next_total - normal_total) / candy_exp).ceil();
        add(&mut result.normal_candy_num, require_candy_num);
        add(&mut result.normal_candy_shard, lv_shard * require_candy_num);
        normal_total += require_candy_num * candy_exp;

        let boost_require_candy_num = ((next_total - boost_total) / boosted_candy_exp).ceil();
        add(&mut result.boost_candy_num, boost_require_candy_num);
        add(&mut result.boost_candy_shard, lv_shard * boost_require_candy_num * candy.boost_shard);
        boost_total += boost_require_candy_num * boosted_candy_exp;

        if can_min_boost {
            if min_boost_candy_num > 0.0 {
                let n = js::min(((next_total - min_boost_total) / boosted_candy_exp).ceil(), min_boost_candy_num);
                min_boost_candy_num -= n;
                add(&mut result.best_boost_candy_shard, lv_shard * n * candy.boost_shard);
                min_boost_total += n * boosted_candy_exp;
            }
            if next_total > min_boost_total {
                let n = ((next_total - min_boost_total) / candy_exp).ceil();
                add(&mut result.best_normal_candy_shard, lv_shard * n);
                min_boost_total += n * candy_exp;
            }
        }

        lv += 1;
        next_total = js::round(total(lv) * info.exp);
    }

    result
}
