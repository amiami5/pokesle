//! 本家(JavaScript)と同じ結果を得るための小さな互換関数。

/// JS の `Math.round`(0.5 は正の無限大方向へ丸める)
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// JS の `Number.prototype.toFixed`。同距離の場合は大きい方へ丸める。
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return if x.is_nan() { "NaN".into() } else if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x.abs() >= 1e21 {
        return format!("{x}");
    }
    // 十分な桁数の正確な10進展開から、指定桁で四捨五入する
    let neg = x < 0.0;
    let s = format!("{:.*}", digits + 30, x.abs());
    let (int_part, frac_part) = s.split_once('.').unwrap_or((&s, ""));
    let mut digits_vec: Vec<u8> = int_part.bytes().chain(frac_part.bytes().take(digits)).map(|b| b - b'0').collect();
    let rest = &frac_part[digits.min(frac_part.len())..];
    let round_up = rest.as_bytes().first().is_some_and(|&b| b >= b'5');
    if round_up {
        let mut i = digits_vec.len();
        loop {
            if i == 0 {
                digits_vec.insert(0, 1);
                break;
            }
            i -= 1;
            if digits_vec[i] == 9 {
                digits_vec[i] = 0;
            } else {
                digits_vec[i] += 1;
                break;
            }
        }
    }
    let int_len = digits_vec.len() - digits;
    let mut out = String::new();
    let is_zero = digits_vec.iter().all(|&d| d == 0);
    if neg && !is_zero {
        out.push('-');
    }
    for d in &digits_vec[..int_len] {
        out.push((b'0' + d) as char);
    }
    if digits > 0 {
        out.push('.');
        for d in &digits_vec[int_len..] {
            out.push((b'0' + d) as char);
        }
    }
    out
}

/// JS の数値 → 文字列変換(キャッシュキー用)。往復可能な最短表現であれば一致する。
pub fn num_key(x: f64) -> u64 {
    // -0 と 0 は JS の文字列化では同じ "0"
    if x == 0.0 { 0 } else { x.to_bits() }
}

/// JS の `Math.max(...list)`(空なら -Infinity、NaN を含めば NaN)
pub fn max_of(list: impl IntoIterator<Item = f64>) -> f64 {
    let mut result = f64::NEG_INFINITY;
    for x in list {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > result {
            result = x;
        }
    }
    result
}

/// JS の `Math.min(a, b)`(NaN を含めば NaN)
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// JS の `Math.max(a, b)`(NaN を含めば NaN)
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

/// JS の `(a, b) => a - b` による比較(NaN は等しいものとして扱う)。安定ソート用。
pub fn cmp_num(a: f64, b: f64) -> std::cmp::Ordering {
    let d = a - b;
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_matches_js() {
        assert_eq!(to_fixed(0.03125, 4), "0.0313");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(0.12345678, 3), "0.123");
        assert_eq!(to_fixed(-0.00001, 3), "0.000");
        assert_eq!(to_fixed(99.9999, 2), "100.00");
    }

    #[test]
    fn round_matches_js() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
    }
}
