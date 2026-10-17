mod common;

use reqly_core::model::{BodyKind, KeyValueKind};
use reqly_interop::{ExportFormat, Format, ImportedItem, Source, detect, export, import};

fn load(path: &str) -> reqly_interop::ImportBundle {
    let sources = [Source::new(path, common::fixture(path))];
    import(&sources, detect(&sources)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn requests(items: &[ImportedItem]) -> Vec<&reqly_core::model::HttpRequest> {
    let mut out = Vec::new();
    for i in items {
        match i {
            ImportedItem::Request { data, .. } => out.push(data),
            ImportedItem::Folder { children, .. } => out.extend(requests(children)),
        }
    }
    out
}

#[test]
fn curl_fixtures() {
    let dir = format!("{}/fixtures/third-party/curl", env!("CARGO_MANIFEST_DIR"));
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let text = common::fixture(&format!("third-party/curl/{name}"));
        let sources = [Source::new(&name, text)];
        assert_eq!(detect(&sources), Some(Format::Curl), "{name}");
        if name == "no-url-input.sh" {
            assert_eq!(import(&sources, None).unwrap_err().code, "import.invalid");
            continue;
        }
        import(&sources, None).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let multi = load("third-party/curl/multi-input.sh");
    let reqs = requests(&multi.collections[0].items);
    assert_eq!(reqs.len(), 4);
    assert!(
        multi
            .report
            .entries
            .iter()
            .any(|e| e.code == "curl_skipped"),
        "echo is reported"
    );
    let ImportedItem::Folder { name, .. } = &multi.collections[0].items[0] else {
        panic!()
    };
    assert_eq!(name, "third-party/curl/multi-input.sh");
}

#[test]
fn har_fixtures() {
    let deep = load("third-party/har/deep-input.json");
    assert_eq!(deep.format, Format::Har);
    let reqs = requests(&deep.collections[0].items);
    assert!(!reqs.is_empty());
    assert_eq!(reqs[0].url, "https://insomnia.rest/foo/bar");
    assert_eq!(reqs[0].params[0].name, "foo");
    assert!(!reqs[0].headers.iter().any(|h| h.name == "Host"));
    assert!(deep.report.entries.iter().any(|e| e.code == "har_no_url"));

    let form = load("third-party/har/form-data-input.json");
    let reqs = requests(&form.collections[0].items);
    assert!(
        reqs.iter()
            .any(|r| r.body.kind == BodyKind::Multipart || r.body.kind == BodyKind::FormUrlencoded)
    );
    for name in [
        "minimal-input.json",
        "no-requests-input.json",
        "shallow-input.json",
    ] {
        load(&format!("third-party/har/{name}"));
    }
}

#[test]
fn har_round_trip() {
    let data = common::sample();
    let text = export(&data, ExportFormat::Har, 0, "t", "x")
        .unwrap()
        .remove(0)
        .content;
    let bundle = import(&[Source::new("x.har", text)], None).unwrap();
    assert_eq!(bundle.format, Format::Har);
    let reqs = requests(&bundle.collections[0].items);
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[1].url, "{{ base_url }}/users");
    assert_eq!(reqs[1].params.len(), 1, "disabled params are not sent");
    assert!(
        reqs[1].headers.iter().any(|h| h.name == "X-Team"),
        "folder headers are applied"
    );
    assert_eq!(reqs[2].body.kind, BodyKind::FormUrlencoded);
}

#[test]
fn curl_export_and_reimport() {
    let data = common::sample();
    let text = export(&data, ExportFormat::Curl, 0, "t", "Example API")
        .unwrap()
        .remove(0);
    assert_eq!(text.name, "example-api.sh");
    assert!(text.content.contains("# Users / List users"));
    let bundle = import(&[Source::new("cmds.sh", text.content)], None).unwrap();
    let reqs = requests(&bundle.collections[0].items);
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].method, "POST");
    assert!(reqs[2].body.kind == BodyKind::FormUrlencoded || reqs[2].body.kind == BodyKind::Text);
    let _ = KeyValueKind::Text;
}

#[test]
fn openapi_fixtures() {
    for dir in ["openapi3", "swagger2"] {
        let path = format!("{}/fixtures/third-party/{dir}", env!("CARGO_MANIFEST_DIR"));
        for entry in std::fs::read_dir(&path).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            let bundle = load(&format!("third-party/{dir}/{name}"));
            assert_eq!(bundle.format, Format::OpenApi, "{name}");
            assert!(bundle.collections[0].request_count() > 0, "{name}");
        }
    }
}

#[test]
fn openapi_petstore() {
    let bundle = load("third-party/openapi3/petstore-with-tags-input.json");
    let c = &bundle.collections[0];
    assert!(
        c.variables["base_url"]
            .as_str()
            .unwrap()
            .starts_with("http")
    );
    let folders: Vec<_> = c
        .items
        .iter()
        .filter(|i| matches!(i, ImportedItem::Folder { .. }))
        .map(ImportedItem::name)
        .collect();
    assert!(folders.contains(&"pet"), "{folders:?}");
    let reqs = requests(&c.items);
    let by_id = reqs
        .iter()
        .find(|r| r.url.ends_with("/pet/:petId"))
        .expect("path params converted");
    assert_eq!(by_id.path_params[0].name, "petId");
    assert!(by_id.url.starts_with("{{ base_url }}"));
    let add = reqs
        .iter()
        .find(|r| r.method == "POST" && r.url.ends_with("/pet"))
        .unwrap();
    assert_eq!(add.body.kind, BodyKind::Json);
    assert!(add.body.text.contains("\"name\""), "{}", add.body.text);
    insta::assert_json_snapshot!("petstore-report", bundle.report);
}

#[test]
fn openapi_server_variables_and_security() {
    let vars = load("third-party/openapi3/example-with-server-variables-input.yaml");
    let c = &vars.collections[0];
    assert!(
        c.variables["base_url"].as_str().unwrap().contains("{{ "),
        "{:?}",
        c.variables
    );
    assert!(c.variables.len() > 1);

    let sec = load("third-party/openapi3/endpoint-security-input.yaml");
    let reqs = requests(&sec.collections[0].items);
    assert!(reqs.iter().any(|r| !matches!(
        r.auth,
        reqly_core::model::Auth::Inherit | reqly_core::model::Auth::None
    )));

    let oauth = load("third-party/openapi3/oauth2-input.yaml");
    assert!(
        requests(&oauth.collections[0].items)
            .iter()
            .any(|r| matches!(&r.auth, reqly_core::model::Auth::OAuth2(c) if !c.token_url.is_empty() || !c.auth_url.is_empty()))
    );
}

#[test]
fn swagger_petstore() {
    let bundle = load("third-party/swagger2/petstore-input.json");
    let c = &bundle.collections[0];
    assert!(
        c.variables["base_url"]
            .as_str()
            .unwrap()
            .starts_with("http")
    );
    let reqs = requests(&c.items);
    let upload = reqs.iter().find(|r| r.url.contains("uploadImage")).unwrap();
    assert_eq!(upload.body.kind, BodyKind::Multipart);
    assert!(
        upload
            .body
            .fields
            .iter()
            .any(|f| f.kind == KeyValueKind::File)
    );
    let add = reqs
        .iter()
        .find(|r| r.method == "POST" && r.url.ends_with("/pet"))
        .unwrap();
    assert!(add.body.text.contains("\"name\""), "{}", add.body.text);
}
