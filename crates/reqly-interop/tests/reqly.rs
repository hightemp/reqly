mod common;

use pretty_assertions::assert_eq;
use reqly_interop::reqly::{ExportOptions, Syntax, export, import, migrate};
use reqly_interop::{Format, Source, detect};
use serde_json::json;

#[test]
fn json_and_yaml_round_trip() {
    let data = common::sample();
    for syntax in [Syntax::Json, Syntax::Yaml] {
        let options = ExportOptions {
            syntax,
            exported_at: Some(1),
            source: "reqly test".into(),
        };
        let text = export(&data, &options).unwrap();
        assert_eq!(
            detect(&[Source::new("x", text.clone())]),
            Some(Format::Reqly)
        );
        let bundle = import(&text).unwrap();
        assert_eq!(
            common::without_row_ids(bundle.collections.clone()),
            common::without_row_ids(data.collections.clone())
        );
        assert_eq!(bundle.environments, data.environments);
        assert_eq!(bundle.cookies, data.cookies);
        assert!(bundle.report.entries.is_empty());
        // export → import → export is stable.
        let again = reqly_interop::ExportData {
            workspace: data.workspace.clone(),
            collections: bundle.collections,
            environments: bundle.environments,
            cookies: bundle.cookies,
            history: bundle.history,
        };
        assert_eq!(export(&again, &options).unwrap(), text);
    }
}

#[test]
fn secrets_can_be_stripped() {
    let mut data = common::sample();
    data.strip_secrets();
    assert_eq!(data.collections[0].variables["key"], "");
    assert_eq!(
        data.collections[0].variables["base_url"],
        "https://api.example.com"
    );
}

#[test]
fn json_export_snapshot() {
    let text = export(
        &common::sample(),
        &ExportOptions {
            exported_at: Some(1_700_000_000_000),
            source: "reqly 0.3.0".into(),
            ..ExportOptions::default()
        },
    )
    .unwrap();
    insta::assert_snapshot!(text);
}

#[test]
fn migrations() {
    let v1 = json!({ "format": "reqly", "formatVersion": "1.0.0", "collections": [] });
    assert_eq!(
        migrate(v1).unwrap()["formatVersion"],
        reqly_interop::reqly::FORMAT_VERSION
    );
    let missing = json!({ "format": "reqly", "collections": [] });
    assert!(migrate(missing).is_ok());
    let future = json!({ "format": "reqly", "formatVersion": "99.0.0" });
    assert_eq!(migrate(future).unwrap_err().code, "import.newer_format");
}

#[test]
fn rejects_other_documents() {
    assert_eq!(import("{\"a\": 1}").unwrap_err().code, "import.invalid");
    assert_eq!(import("not json").unwrap_err().code, "import.invalid");
}
