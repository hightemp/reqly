//! Fractional indexing keys used to order siblings (tree nodes, tabs, rows).
//!
//! A key is a non-empty base-62 string that never ends with the zero digit.
//! Keys compare lexicographically by bytes, so a new key can always be
//! generated between any two existing keys without renumbering siblings —
//! a property needed for sync and real-time collaboration.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::error::{CoreError, CoreResult};

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[ts(export, type = "string")]
pub struct SortKey(String);

impl SortKey {
    pub fn parse(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.bytes().all(|b| digit_index(b).is_some())
            && !value.ends_with('0');
        if valid {
            Ok(Self(value))
        } else {
            Err(CoreError::new("sort_key.invalid", "invalid sort key").with_param("value", value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns a key strictly between `prev` and `next`.
    /// `None` stands for the open start or end of the list.
    pub fn between(prev: Option<&SortKey>, next: Option<&SortKey>) -> CoreResult<Self> {
        if let (Some(a), Some(b)) = (prev, next)
            && a >= b
        {
            return Err(
                CoreError::new("sort_key.unordered", "prev must be less than next")
                    .with_param("prev", a.as_str())
                    .with_param("next", b.as_str()),
            );
        }
        let mid = match (prev, next) {
            // Appending and prepending step the last digit instead of halving,
            // so long runs of appends grow keys by one char per ~60 items.
            (Some(a), None) => increment(a.0.as_bytes()),
            (None, Some(b)) => decrement(b.0.as_bytes()),
            (a, b) => midpoint(
                a.map_or(&[][..], |k| k.0.as_bytes()),
                b.map(|k| k.0.as_bytes()),
            ),
        };
        Ok(Self(String::from_utf8(mid).expect("digits are ascii")))
    }

    /// Returns `n` ascending keys strictly between `prev` and `next`,
    /// spread evenly so that key lengths stay short.
    pub fn n_between(
        prev: Option<&SortKey>,
        next: Option<&SortKey>,
        n: usize,
    ) -> CoreResult<Vec<Self>> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let mid = Self::between(prev, next)?;
        let left = n / 2;
        let mut keys = Self::n_between(prev, Some(&mid), left)?;
        keys.push(mid.clone());
        keys.extend(Self::n_between(Some(&mid), next, n - left - 1)?);
        Ok(keys)
    }
}

fn digit_index(byte: u8) -> Option<usize> {
    DIGITS.iter().position(|&d| d == byte)
}

/// Smallest-effort key greater than `a`.
fn increment(a: &[u8]) -> Vec<u8> {
    let last = digit_index(a[a.len() - 1]).expect("validated key");
    let mut out = a.to_vec();
    if last + 1 < DIGITS.len() {
        *out.last_mut().expect("non-empty") = DIGITS[last + 1];
    } else {
        out.push(DIGITS[1]);
    }
    out
}

/// Smallest-effort key less than `b`.
fn decrement(b: &[u8]) -> Vec<u8> {
    let last = digit_index(b[b.len() - 1]).expect("validated key");
    let mut out = b.to_vec();
    if last > 1 {
        *out.last_mut().expect("non-empty") = DIGITS[last - 1];
    } else {
        *out.last_mut().expect("non-empty") = DIGITS[0];
        out.push(DIGITS[DIGITS.len() - 1]);
    }
    out
}

/// Midpoint of two digit strings, `a < b`, where an empty `a` is the start
/// and `None` for `b` is the end. Neither input ends with the zero digit,
/// and neither does the result.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        let common = b
            .iter()
            .enumerate()
            .take_while(|&(i, &d)| a.get(i).copied().unwrap_or(DIGITS[0]) == d)
            .count();
        if common > 0 {
            let mut out = b[..common].to_vec();
            out.extend(midpoint(
                a.get(common..).unwrap_or_default(),
                Some(&b[common..]),
            ));
            return out;
        }
    }

    let digit_a = a
        .first()
        .map_or(0, |&d| digit_index(d).expect("validated key"));
    let digit_b = b.map_or(DIGITS.len(), |b| digit_index(b[0]).expect("validated key"));

    if digit_b - digit_a > 1 {
        let mid = (digit_a + digit_b).div_ceil(2);
        vec![DIGITS[mid]]
    } else if let Some(b) = b.filter(|b| b.len() > 1) {
        vec![b[0]]
    } else {
        let mut out = vec![DIGITS[digit_a]];
        out.extend(midpoint(a.get(1..).unwrap_or_default(), None));
        out
    }
}

impl fmt::Display for SortKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for SortKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SortKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<String> for SortKey {
    type Error = CoreError;

    fn try_from(value: String) -> CoreResult<Self> {
        Self::parse(value)
    }
}

impl From<SortKey> for String {
    fn from(key: SortKey) -> Self {
        key.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn key(s: &str) -> SortKey {
        SortKey::parse(s).unwrap()
    }

    #[test]
    fn parse_rejects_invalid_keys() {
        assert!(SortKey::parse("").is_err());
        assert!(SortKey::parse("a0").is_err(), "trailing zero digit");
        assert!(SortKey::parse("a-b").is_err(), "non base-62 char");
        assert!(SortKey::parse("V").is_ok());
    }

    #[test]
    fn between_open_bounds_gives_middle_digit() {
        let k = SortKey::between(None, None).unwrap();
        assert_eq!(k.as_str(), "V");
    }

    #[test]
    fn between_appends_and_prepends() {
        let a = key("V");
        let after = SortKey::between(Some(&a), None).unwrap();
        let before = SortKey::between(None, Some(&a)).unwrap();
        assert!(before < a && a < after);
    }

    #[test]
    fn between_adjacent_digits_extends_length() {
        let a = key("V");
        let b = key("W");
        let m = SortKey::between(Some(&a), Some(&b)).unwrap();
        assert!(a < m && m < b);
        assert_eq!(m.as_str().len(), 2);
    }

    #[test]
    fn between_rejects_unordered_bounds() {
        let a = key("V");
        assert!(SortKey::between(Some(&a), Some(&a)).is_err());
        assert!(SortKey::between(Some(&key("W")), Some(&a)).is_err());
    }

    #[test]
    fn n_between_is_sorted_and_bounded() {
        let a = key("A");
        let b = key("B");
        let keys = SortKey::n_between(Some(&a), Some(&b), 50).unwrap();
        assert_eq!(keys.len(), 50);
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert!(a < keys[0] && keys[49] < b);
    }

    #[test]
    fn n_between_zero_is_empty() {
        assert!(SortKey::n_between(None, None, 0).unwrap().is_empty());
    }

    #[test]
    fn repeated_append_keeps_keys_short() {
        let mut last: Option<SortKey> = None;
        for _ in 0..1000 {
            last = Some(SortKey::between(last.as_ref(), None).unwrap());
        }
        assert!(last.unwrap().as_str().len() <= 40);
    }

    fn arb_key() -> impl Strategy<Value = SortKey> {
        "[0-9A-Za-z]{0,6}[1-9A-Za-z]".prop_map(|s| SortKey::parse(s).unwrap())
    }

    proptest! {
        #[test]
        fn between_is_strictly_inside(a in arb_key(), b in arb_key()) {
            prop_assume!(a != b);
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            let m = SortKey::between(Some(&lo), Some(&hi)).unwrap();
            prop_assert!(lo < m && m < hi, "{lo} < {m} < {hi}");
            prop_assert!(SortKey::parse(m.as_str()).is_ok());
        }

        #[test]
        fn between_open_ends(a in arb_key()) {
            let after = SortKey::between(Some(&a), None).unwrap();
            let before = SortKey::between(None, Some(&a)).unwrap();
            prop_assert!(before < a && a < after);
        }

        #[test]
        fn serde_round_trip(a in arb_key()) {
            let json = serde_json::to_string(&a).unwrap();
            prop_assert_eq!(serde_json::from_str::<SortKey>(&json).unwrap(), a);
        }
    }
}
