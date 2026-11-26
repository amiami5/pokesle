//! 下振れ基準の計算(本家 `models/utils/prob-border.js`)

use std::collections::HashMap;

use crate::js;

pub struct ProbBorder {
    reverse_border: f64,
    get_cache: HashMap<(u64, u64), f64>,
    skill_cache: HashMap<(u64, u64, u64, u64), f64>,
}

#[inline]
fn f64_key(x: f64) -> u64 {
    if x == 0.0 { 0.0_f64.to_bits() } else if x.is_nan() { f64::NAN.to_bits() } else { x.to_bits() }
}

#[inline]
fn p4_key(p: f64) -> u64 {
    if p.is_finite() {
        f64_key(js::round(p * 10000.0))
    } else {
        f64_key(p)
    }
}

impl ProbBorder {
    pub fn new(border: f64) -> ProbBorder {
        ProbBorder { reverse_border: 1.0 - border, get_cache: HashMap::new(), skill_cache: HashMap::new() }
    }

    /// 確率 `p` の試行を `n` 回行ったとき、成功回数が下振れ基準を満たす境界値
    pub fn get(&mut self, p: f64, n: f64) -> f64 {
        if self.reverse_border <= 0.0 {
            return 0.0;
        }
        if self.reverse_border >= 1.0 {
            return n;
        }

        let n = n.floor();
        let key = (p4_key(p), f64_key(n));
        if let Some(&v) = self.get_cache.get(&key) {
            return v;
        }

        let mut sum_p = 0.0;
        let rp = 1.0 - p;
        let mut i = 0.0;
        while i <= n {
            // i回成功する確率
            let mut this_p = p.powf(i) * rp.powf(n - i);
            let mut j = 0.0;
            while j < i {
                this_p *= (n - j) / (i - j);
                j += 1.0;
            }

            if sum_p + this_p >= self.reverse_border {
                let v = i + (self.reverse_border - sum_p) / this_p;
                self.get_cache.insert(key, v);
                return v;
            }
            sum_p += this_p;
            i += 1.0;
        }

        self.get_cache.insert(key, n);
        n
    }

    /// 1区間で最大 `limit` 回まで持ち越せるスキル発動を `n2` 区間繰り返したときの境界値
    pub fn skill(&mut self, p: f64, n1: f64, n2: f64, limit: f64) -> f64 {
        if self.reverse_border <= 0.0 {
            return 0.0;
        }
        if self.reverse_border >= 1.0 {
            return n2 * limit;
        }

        let n1 = n1.floor();
        let n2 = n2.floor();
        let limit = limit.floor();

        let key = (p4_key(p), f64_key(n1), f64_key(n2), f64_key(limit));
        if let Some(&v) = self.skill_cache.get(&key) {
            return v;
        }

        let limit_u = limit.max(0.0) as usize;
        let n2_u = n2.max(0.0) as usize;

        // 1. フェーズ1の確率分布 [P(0), P(1), ..., P(limit)]
        let mut phase1 = vec![0.0; limit_u + 1];
        let rp = 1.0 - p;
        let mut p_i = rp.powf(n1);
        phase1[0] = p_i;
        let mut sum_prob = p_i;
        for i in 1..limit_u {
            let fi = i as f64;
            p_i *= (n1 - fi + 1.0) / fi * (p / rp);
            phase1[i] = p_i;
            sum_prob += p_i;
        }
        phase1[limit_u] = 1.0 - sum_prob;

        // 2. n2回分の分布を畳み込み
        let max_success = n2_u * limit_u;
        let mut dist = vec![0.0; max_success + 1];
        dist[0] = 1.0;
        let mut current_max = 0;
        for _ in 0..n2_u {
            let mut next = vec![0.0; max_success + 1];
            let next_max = current_max + limit_u;
            for s in 0..=current_max {
                if dist[s] == 0.0 {
                    continue;
                }
                for add in 0..=limit_u {
                    next[s + add] += dist[s] * phase1[add];
                }
            }
            dist = next;
            current_max = next_max;
        }

        // 3. 累積確率が基準を超える位置
        let target = self.reverse_border;
        let mut cumulative = 0.0;
        for (i, &d) in dist.iter().enumerate().take(current_max + 1) {
            if cumulative + d >= target {
                let denom = if d == 0.0 { 1e-18 } else { d };
                let v = i as f64 + (target - cumulative) / denom;
                self.skill_cache.insert(key, v);
                return v;
            }
            cumulative += d;
        }
        let v = current_max as f64;
        self.skill_cache.insert(key, v);
        v
    }
}
