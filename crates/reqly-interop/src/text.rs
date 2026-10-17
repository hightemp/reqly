//! Small helpers shared by importers.

use reqly_core::model::KeyValue;

/// A key/value row, disabled when `disabled` is true.
pub fn kv(name: impl Into<String>, value: impl Into<String>, disabled: bool) -> KeyValue {
    let row = KeyValue::new(name, value);
    if disabled { row.disabled() } else { row }
}

/// Content-Type header value of `headers`, if any.
pub fn content_type(headers: &[KeyValue]) -> Option<&str> {
    headers
        .iter()
        .find(|h| h.enabled && h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.as_str())
}
