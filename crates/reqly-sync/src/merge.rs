//! Three-way merge of records (ADR-013).

use serde_json::{Map, Value};

use crate::record::{Kind, Record};
use crate::text;

/// The result of [`merge`].
#[derive(Debug, Clone, PartialEq)]
pub enum Merged {
    Record(Record),
    /// Deleted on one side, changed on the other: the user decides.
    Conflict,
}

/// Merges `ours` (the change being applied) and `theirs` (the other side),
/// both made from `base` (`None`: no common ancestor). Fields changed on one
/// side take that side; changed on both, text fields merge their edits and
/// anything else takes `ours`.
pub fn merge(base: Option<&Record>, ours: &Record, theirs: &Record) -> Merged {
    let empty = Map::new();
    let base_fields = base.map_or(&empty, |b| &b.fields);
    let base_deleted = base.is_some_and(|b| b.deleted);
    let ours_deleted_now = ours.deleted && !base_deleted;
    let theirs_deleted_now = theirs.deleted && !base_deleted;
    if base.is_some() {
        let ours_changed = ours.fields != *base_fields;
        let theirs_changed = theirs.fields != *base_fields;
        let ours_restored = !ours.deleted && base_deleted;
        let theirs_restored = !theirs.deleted && base_deleted;
        if (ours_deleted_now && !theirs.deleted && (theirs_changed || theirs_restored))
            || (theirs_deleted_now && !ours.deleted && (ours_changed || ours_restored))
        {
            return Merged::Conflict;
        }
    }
    let deleted = if ours.deleted != base_deleted || base.is_none() {
        ours.deleted
    } else {
        theirs.deleted
    };
    let fields = merge_map(ours.kind, "", base_fields, &ours.fields, &theirs.fields);
    Merged::Record(Record {
        deleted,
        fields,
        ..ours.clone()
    })
}

fn merge_map(
    kind: Kind,
    path: &str,
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
) -> Map<String, Value> {
    let mut out = Map::new();
    let keys = ours
        .keys()
        .chain(theirs.keys().filter(|k| !ours.contains_key(*k)));
    for key in keys {
        let child = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        if let Some(v) = merge_value(kind, &child, base.get(key), ours.get(key), theirs.get(key)) {
            out.insert(key.clone(), v);
        }
    }
    // Keys only in the base were removed on both sides (or on one side with
    // the other unchanged), which `merge_value` already decided as absent.
    out
}

fn merge_value(
    kind: Kind,
    path: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
) -> Option<Value> {
    if ours == theirs || theirs == base {
        return ours.cloned();
    }
    if ours == base {
        return theirs.cloned();
    }
    match (ours, theirs) {
        (Some(Value::Object(o)), Some(Value::Object(t))) => {
            let empty = Map::new();
            let b = match base {
                Some(Value::Object(b)) => b,
                _ => &empty,
            };
            Some(Value::Object(merge_map(kind, path, b, o, t)))
        }
        (Some(Value::String(o)), Some(Value::String(t))) if kind.text_fields().contains(&path) => {
            let b = base.and_then(Value::as_str).unwrap_or("");
            Some(Value::String(text::merge(b, o, t)))
        }
        _ => ours.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqly_core::Id;
    use serde_json::json;

    fn rec(fields: Value) -> Record {
        Record {
            kind: Kind::Node,
            id: Id::new(),
            workspace_id: Id::new(),
            deleted: false,
            fields: fields.as_object().unwrap().clone(),
        }
    }

    fn merged(base: &Record, ours: &Record, theirs: &Record) -> Record {
        match merge(Some(base), ours, theirs) {
            Merged::Record(r) => r,
            Merged::Conflict => panic!("unexpected conflict"),
        }
    }

    #[test]
    fn fields_changed_on_one_side_each() {
        let base = rec(json!({ "name": "a", "sortKey": "m", "data": { "method": "GET" } }));
        let ours = rec(json!({ "name": "b", "sortKey": "m", "data": { "method": "GET" } }));
        let theirs = rec(json!({ "name": "a", "sortKey": "n", "data": { "method": "POST" } }));
        assert_eq!(
            merged(&base, &ours, &theirs).fields,
            rec(json!({ "name": "b", "sortKey": "n", "data": { "method": "POST" } })).fields
        );
    }

    #[test]
    fn both_changed_ours_wins_except_text() {
        let base = rec(json!({ "name": "a", "data": { "url": "http://x/a", "headers": [] } }));
        let ours = rec(json!({ "name": "ours", "data": { "url": "https://x/a", "headers": [1] } }));
        let theirs =
            rec(json!({ "name": "theirs", "data": { "url": "http://x/a?q=1", "headers": [2] } }));
        assert_eq!(
            merged(&base, &ours, &theirs).fields,
            rec(json!({ "name": "ours", "data": { "url": "https://x/a?q=1", "headers": [1] } }))
                .fields
        );
    }

    #[test]
    fn nested_text_fields_merge() {
        let base = rec(
            json!({ "data": { "body": { "kind": "json", "text": "{\n  \"a\": 1,\n  \"b\": 2\n}" } } }),
        );
        let ours = rec(
            json!({ "data": { "body": { "kind": "json", "text": "{\n  \"a\": 10,\n  \"b\": 2\n}" } } }),
        );
        let theirs = rec(
            json!({ "data": { "body": { "kind": "json", "text": "{\n  \"a\": 1,\n  \"b\": 20\n}" } } }),
        );
        assert_eq!(
            merged(&base, &ours, &theirs).fields["data"]["body"]["text"],
            "{\n  \"a\": 10,\n  \"b\": 20\n}"
        );
    }

    #[test]
    fn non_text_strings_are_not_merged() {
        let base = rec(json!({ "name": "Users" }));
        let ours = rec(json!({ "name": "Users v2" }));
        let theirs = rec(json!({ "name": "All Users" }));
        assert_eq!(merged(&base, &ours, &theirs).fields["name"], "Users v2");
    }

    #[test]
    fn removed_keys() {
        let base = rec(json!({ "a": 1, "b": 2 }));
        let ours = rec(json!({ "a": 1 }));
        let theirs = rec(json!({ "a": 3, "b": 2 }));
        assert_eq!(
            merged(&base, &ours, &theirs).fields,
            rec(json!({ "a": 3 })).fields
        );
    }

    #[test]
    fn delete_against_change_is_a_conflict() {
        let base = rec(json!({ "name": "a" }));
        let mut deleted = base.clone();
        deleted.deleted = true;
        let changed = rec(json!({ "name": "b" }));
        assert_eq!(merge(Some(&base), &deleted, &changed), Merged::Conflict);
        assert_eq!(merge(Some(&base), &changed, &deleted), Merged::Conflict);
    }

    #[test]
    fn delete_against_nothing_deletes() {
        let base = rec(json!({ "name": "a" }));
        let mut deleted = base.clone();
        deleted.deleted = true;
        assert!(merged(&base, &deleted, &base).deleted);
        assert!(merged(&base, &base, &deleted).deleted);
        assert!(merged(&base, &deleted, &deleted).deleted);
    }

    #[test]
    fn restore_against_change_of_a_deleted_record() {
        let mut base = rec(json!({ "name": "a" }));
        base.deleted = true;
        let restored = rec(json!({ "name": "a" }));
        let mut renamed = rec(json!({ "name": "b" }));
        renamed.deleted = true;
        let r = merged(&base, &restored, &renamed);
        assert!(!r.deleted);
        assert_eq!(r.fields["name"], "b");
    }

    #[test]
    fn without_base_ours_wins() {
        let ours = rec(json!({ "name": "a", "x": 1 }));
        let theirs = rec(json!({ "name": "b", "y": 2 }));
        let Merged::Record(r) = merge(None, &ours, &theirs) else {
            panic!()
        };
        assert_eq!(r.fields, rec(json!({ "name": "a", "x": 1, "y": 2 })).fields);
    }
}
