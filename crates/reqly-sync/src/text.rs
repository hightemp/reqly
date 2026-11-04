//! Text merges with Yjs (`yrs`), shared with live collaboration (ADR-012).

use yrs::{
    ClientID, Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact,
    Update, updates::decoder::Decode,
};

/// A document with one text `t`. UTF-16 offsets, as Yjs in the browser.
pub fn new_doc(client_id: u64, text: &str) -> (Doc, TextRef) {
    let doc = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::with_client_id(ClientID::new(client_id))
    });
    let t = doc.get_or_insert_text("t");
    if !text.is_empty() {
        t.insert(&mut doc.transact_mut(), 0, text);
    }
    (doc, t)
}

/// Replaces `text` with `target` by the smallest single edit, so concurrent
/// edits elsewhere in the text survive. Returns the Yjs update, if any.
pub fn replace(doc: &Doc, text: &TextRef, target: &str) -> Option<Vec<u8>> {
    let current = text.get_string(&doc.transact());
    if current == target {
        return None;
    }
    let a: Vec<u16> = current.encode_utf16().collect();
    let b: Vec<u16> = target.encode_utf16().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let inserted = String::from_utf16_lossy(&b[prefix..b.len() - suffix]);
    let before = doc.transact().state_vector();
    {
        let mut txn = doc.transact_mut();
        let removed = (a.len() - suffix - prefix) as u32;
        if removed > 0 {
            text.remove_range(&mut txn, prefix as u32, removed);
        }
        if !inserted.is_empty() {
            text.insert(&mut txn, prefix as u32, &inserted);
        }
    }
    Some(doc.transact().encode_state_as_update_v1(&before))
}

/// Merges two edits of `base` as concurrent Yjs edits, so changes in
/// different places both survive. Deterministic: the same inputs always
/// give the same text.
pub fn merge(base: &str, ours: &str, theirs: &str) -> String {
    if ours == theirs || theirs == base {
        return ours.to_string();
    }
    if ours == base {
        return theirs.to_string();
    }
    let (origin, _) = new_doc(1, base);
    let state = origin
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let fork = |client| {
        let doc = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::with_client_id(ClientID::new(client))
        });
        let t = doc.get_or_insert_text("t");
        if let Ok(update) = Update::decode_v1(&state) {
            let _ = doc.transact_mut().apply_update(update);
        }
        (doc, t)
    };
    let (ours_doc, ours_text) = fork(2);
    let (theirs_doc, theirs_text) = fork(3);
    replace(&ours_doc, &ours_text, ours);
    if let Some(update) = replace(&theirs_doc, &theirs_text, theirs)
        && let Ok(update) = Update::decode_v1(&update)
    {
        let _ = ours_doc.transact_mut().apply_update(update);
    }
    ours_text.get_string(&ours_doc.transact())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_in_different_places_both_survive() {
        let base = "line one\nline two\nline three";
        let ours = "line ONE\nline two\nline three";
        let theirs = "line one\nline two\nline THREE";
        assert_eq!(merge(base, ours, theirs), "line ONE\nline two\nline THREE");
        assert_eq!(merge(base, theirs, ours), "line ONE\nline two\nline THREE");
    }

    #[test]
    fn one_sided_and_equal_edits() {
        assert_eq!(merge("a", "b", "a"), "b");
        assert_eq!(merge("a", "a", "c"), "c");
        assert_eq!(merge("a", "x", "x"), "x");
    }

    #[test]
    fn url_edits_merge() {
        let merged = merge(
            "https://api.test/users?page=1",
            "https://api.test/v2/users?page=1",
            "https://api.test/users?page=2",
        );
        assert_eq!(merged, "https://api.test/v2/users?page=2");
    }

    #[test]
    fn non_ascii_text() {
        assert_eq!(
            merge("привет мир", "Привет мир", "привет мир 🌍"),
            "Привет мир 🌍"
        );
    }

    #[test]
    fn replace_uses_utf16_offsets() {
        let (doc, t) = new_doc(7, "a🌍b");
        assert!(replace(&doc, &t, "a🌍cb").is_some());
        assert_eq!(t.get_string(&doc.transact()), "a🌍cb");
        assert!(replace(&doc, &t, "a🌍cb").is_none());
    }
}
