//! PostgreSQL `numeric` binary format: base-10000 digits with weight, sign and display scale.

use super::reader::Reader;

const DIGIT_WIDTH: usize = 4;
const SIGN_NEGATIVE: u16 = 0x4000;
const SIGN_NAN: u16 = 0xC000;
const SIGN_POS_INF: u16 = 0xD000;
const SIGN_NEG_INF: u16 = 0xF000;
const MAX_DIGIT: u16 = 9999;

/// Exact decimal text, e.g. `-12.3400` for a value with display scale 4.
pub(super) fn decode(r: &mut Reader<'_>) -> Option<String> {
    let count = usize::try_from(r.i16()?).ok()?;
    let weight = i64::from(r.i16()?);
    let sign = r.u16()?;
    let scale = usize::from(r.u16()?);
    let digits = (0..count)
        .map(|_| r.u16().filter(|d| *d <= MAX_DIGIT))
        .collect::<Option<Vec<_>>>()?;
    match sign {
        SIGN_NAN => return Some("NaN".to_owned()),
        SIGN_POS_INF => return Some("Infinity".to_owned()),
        SIGN_NEG_INF => return Some("-Infinity".to_owned()),
        0 | SIGN_NEGATIVE => {}
        _ => return None,
    }
    let digit_at = |position: i64| -> u16 {
        usize::try_from(position)
            .ok()
            .and_then(|i| digits.get(i))
            .copied()
            .unwrap_or(0)
    };
    let integer = if weight < 0 {
        "0".to_owned()
    } else {
        let mut text = digit_at(0).to_string();
        for position in 1..=weight {
            text.push_str(&format!("{:04}", digit_at(position)));
        }
        text
    };
    let mut fraction = String::new();
    let mut position = weight + 1;
    while fraction.len() < scale {
        fraction.push_str(&format!(
            "{:0width$}",
            digit_at(position),
            width = DIGIT_WIDTH
        ));
        position += 1;
    }
    fraction.truncate(scale);
    let negative = sign == SIGN_NEGATIVE && digits.iter().any(|d| *d != 0);
    let sign = if negative { "-" } else { "" };
    Some(if scale == 0 {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction}")
    })
}
