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

#[test]
fn exports_match_the_published_schema() {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/format/reqly.schema.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut data = common::sample();
    data.history.push(reqly_interop::ExportedResponse {
        request_id: "req-login".into(),
        record: json!({ "status": 200 }),
        body_base64: Some("e30=".into()),
    });
    let text = export(
        &data,
        &ExportOptions {
            exported_at: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&doc)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    // Imports from other formats export to valid documents too.
    let insomnia = reqly_interop::import(
        &[reqly_interop::Source::new(
            "x",
            common::fixture("insomnia/v4-full.json"),
        )],
        None,
    )
    .unwrap();
    let data = reqly_interop::ExportData {
        collections: insomnia.collections,
        cookies: insomnia.cookies,
        ..Default::default()
    };
    let doc: serde_json::Value =
        serde_json::from_str(&export(&data, &ExportOptions::default()).unwrap()).unwrap();
    assert!(
        validator.is_valid(&doc),
        "{:?}",
        validator
            .iter_errors(&doc)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );
}
