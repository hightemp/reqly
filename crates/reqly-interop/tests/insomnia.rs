mod common;

use reqly_core::model::{Auth, BodyKind, KeyValueKind};
use reqly_interop::{Format, ImportedItem, ReportKind, Source, detect, import};

fn load(path: &str) -> reqly_interop::ImportBundle {
    let text = common::fixture(path);
    let sources = [Source::new(path, text)];
    assert_eq!(detect(&sources), Some(Format::Insomnia), "{path}");
    import(&sources, None).unwrap()
}

fn find<'a>(items: &'a [ImportedItem], name: &str) -> &'a ImportedItem {
    for i in items {
        if i.name() == name {
            return i;
        }
        if let ImportedItem::Folder { children, .. } = i
            && let Some(found) = children.iter().find(|c| c.name() == name)
        {
            return found;
        }
    }
    panic!("{name} not found")
}

fn check_full(bundle: &reqly_interop::ImportBundle) {
    assert_eq!(bundle.collections.len(), 1);
    let c = &bundle.collections[0];
    assert_eq!(c.name, "Example API");
    let names: Vec<_> = c.items.iter().map(ImportedItem::name).collect();
    assert_eq!(
        names,
        [
            "Auth",
            "Users",
            "Search form",
            "GraphQL viewer",
            "Upload binary",
            "SOAP call"
        ]
    );
    assert_eq!(c.variables["base_url"], "https://api.example.com");
    let envs: Vec<_> = c
        .environments
        .iter()
        .map(|e| (e.name.as_str(), e.color.as_deref()))
        .collect();
    assert_eq!(
        envs,
        [("Production", Some("#ff0000")), ("Local", Some("#00ff00"))]
    );
    assert_eq!(c.environments[0].data["limits"]["rps"], 10);

    let ImportedItem::Folder {
        data: users,
        children,
        ..
    } = find(&c.items, "Users")
    else {
        panic!()
    };
    assert_eq!(users.variables["page_size"], 20);
    assert_eq!(users.headers[0].name, "X-Team");
    assert!(matches!(&users.auth, Auth::Bearer { token, .. } if token.contains("'req_login'")));
    assert!(users.scripts.pre_request.contains("folder"));
    let child_names: Vec<_> = children.iter().map(ImportedItem::name).collect();
    assert_eq!(child_names, ["List users", "Get user", "Upload avatar"]);

    let ImportedItem::Request { data: list, .. } = find(&c.items, "List users") else {
        panic!()
    };
    assert_eq!(list.params.len(), 3);
    assert!(!list.params[2].enabled);
    assert_eq!(list.auth, Auth::Inherit);

    let ImportedItem::Request { data: get, .. } = find(&c.items, "Get user") else {
        panic!()
    };
    assert_eq!(get.path_params[0].name, "id");
    assert_eq!(get.settings.follow_redirects, Some(false));
    assert_eq!(get.settings.encode_url, Some(false));
    assert_eq!(get.settings.send_cookies, Some(false));
    assert!(matches!(&get.auth, Auth::Basic { username, .. } if username == "admin"));

    let ImportedItem::Request { data: avatar, .. } = find(&c.items, "Upload avatar") else {
        panic!()
    };
    assert_eq!(avatar.body.kind, BodyKind::Multipart);
    assert_eq!(avatar.body.fields[1].kind, KeyValueKind::File);
    assert_eq!(avatar.body.fields[1].value, "/home/me/avatar.png");
    assert!(!avatar.body.fields[2].enabled);

    let ImportedItem::Request { data: login, .. } = find(&c.items, "Login") else {
        panic!()
    };
    assert_eq!(login.body.kind, BodyKind::Json);
    assert!(
        login
            .scripts
            .after_response
            .contains("insomnia.environment")
    );
    assert_eq!(find(&c.items, "Login").id(), Some("req_login"));

    let ImportedItem::Request { data: binary, .. } = find(&c.items, "Upload binary") else {
        panic!()
    };
    assert_eq!(binary.body.kind, BodyKind::Binary);

    // Cookies.
    assert_eq!(bundle.cookies.len(), 2);
    assert!(!bundle.cookies[0].include_subdomains);
    assert_eq!(bundle.cookies[0].expires, Some(1_893_456_000));
    assert!(bundle.cookies[1].include_subdomains);

    // Report: digest + oauth2 auth, WebSocket + gRPC, scripts, GraphQL.
    let codes: Vec<_> = bundle
        .report
        .entries
        .iter()
        .map(|e| (e.kind, e.code.as_str()))
        .collect();
    for expected in [
        (ReportKind::Unsupported, "auth_unsupported"),
        (ReportKind::Unsupported, "protocol_unsupported"),
        (ReportKind::Unsupported, "script_kept"),
        (ReportKind::Converted, "graphql_as_json"),
    ] {
        assert!(codes.contains(&expected), "{expected:?} in {codes:?}");
    }
    assert_eq!(
        codes.iter().filter(|c| c.1 == "auth_unsupported").count(),
        2
    );
    assert_eq!(
        codes
            .iter()
            .filter(|c| c.1 == "protocol_unsupported")
            .count(),
        2
    );
    assert!(!codes.iter().any(|c| c.1 == "tag_unsupported"), "{codes:?}");
    let digest = bundle
        .report
        .entries
        .iter()
        .find(|e| e.code == "auth_unsupported")
        .unwrap();
    assert_eq!(digest.path, "Example API / Search form");
    assert!(digest.raw.is_some());
}

#[test]
fn v4_json_full() {
    let bundle = load("insomnia/v4-full.json");
    check_full(&bundle);
    insta::assert_json_snapshot!("v4-report", bundle.report);
}

#[test]
fn v5_yaml_full_matches_v4() {
    let v4 = load("insomnia/v4-full.json");
    let v5 = load("insomnia/v5-full.yaml");
    check_full(&v5);
    assert_eq!(
        common::without_row_ids(v5.collections.clone()),
        common::without_row_ids(v4.collections.clone())
    );
    assert_eq!(v5.cookies, v4.cookies);
}

#[test]
fn v4_yaml_minimal() {
    let bundle = load("third-party/insomnia-4/basic-input.yaml");
    assert_eq!(bundle.collections[0].name, "Insomnia");
}

#[test]
fn unknown_tags_are_reported() {
    let text = r#"{"_type":"export","__export_format":4,"resources":[
      {"_id":"wrk_1","_type":"workspace","name":"W","parentId":null},
      {"_id":"req_1","_type":"request","parentId":"wrk_1","name":"R","method":"GET",
       "url":"https://x/{% customplugin 'a' %}"}]}"#;
    let bundle = import(&[Source::new("x.json", text)], None).unwrap();
    let entry = bundle
        .report
        .entries
        .iter()
        .find(|e| e.code == "tag_unsupported")
        .unwrap();
    assert!(entry.message.contains("customplugin"));
}

#[test]
fn global_environment_workspace() {
    let text = "type: environment.insomnia.rest/5.0\nname: Shared\nmeta:\n  id: wrk_env\nenvironments:\n  name: Base\n  data:\n    a: 1\n  subEnvironments:\n    - name: Dev\n      data:\n        a: 2\n";
    let bundle = import(&[Source::new("env.yaml", text)], None).unwrap();
    assert!(bundle.collections.is_empty());
    let names: Vec<_> = bundle
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["Base", "Dev"]);
}

fn normalized(
    mut collections: Vec<reqly_interop::ImportedCollection>,
) -> Vec<reqly_interop::ImportedCollection> {
    fn strip(items: &mut [ImportedItem]) {
        for i in items {
            match i {
                ImportedItem::Folder { id, children, .. } => {
                    *id = None;
                    strip(children);
                }
                ImportedItem::Request { id, data, .. } => {
                    *id = None;
                    // Ids inside response tags differ (prefixed); compare without them.
                    if let Auth::Bearer { token, .. } = &mut data.auth {
                        *token = token.split(", ").next().unwrap_or("").to_string();
                    }
                }
            }
        }
    }
    for c in &mut collections {
        c.variable_secrets.clear();
        for e in &mut c.environments {
            e.secrets.clear();
        }
        strip(&mut c.items);
    }
    common::without_row_ids(collections)
}

#[test]
fn round_trip_through_insomnia_formats() {
    use reqly_interop::{ExportFormat, export};
    let data = common::sample();
    // What Insomnia cannot express: request variables and folder network settings.
    let mut expected = data.collections.clone();
    fn lossy(items: &mut [ImportedItem]) {
        for i in items {
            match i {
                ImportedItem::Folder { data, children, .. } => {
                    data.settings = Default::default();
                    lossy(children);
                }
                ImportedItem::Request { data, .. } => {
                    data.variables.clear();
                    for f in &mut data.body.fields {
                        f.value = f.value.replace("{{ $uuid }}", "{% uuid %}");
                    }
                }
            }
        }
    }
    lossy(&mut expected[0].items);
    for format in [ExportFormat::Insomnia4, ExportFormat::Insomnia5] {
        let text = export(&data, format, 1_700_000_000_000, "test", "x")
            .unwrap()
            .remove(0)
            .content;
        let sources = [Source::new("x", text.clone())];
        assert_eq!(detect(&sources), Some(Format::Insomnia), "{format:?}");
        let bundle = import(&sources, None).unwrap();
        assert_eq!(
            normalized(bundle.collections.clone()),
            normalized(expected.clone()),
            "{format:?}"
        );
        assert_eq!(bundle.cookies, data.cookies, "{format:?}");
        // The response tag points at the exported Login request.
        let login_id = find(&bundle.collections[0].items, "Login")
            .id()
            .unwrap()
            .to_string();
        let ImportedItem::Request { data: list, .. } =
            find(&bundle.collections[0].items, "List users")
        else {
            panic!()
        };
        assert!(
            matches!(&list.auth, Auth::Bearer { token, .. } if token.contains(&login_id)),
            "{format:?}"
        );
        if format == ExportFormat::Insomnia4 {
            insta::assert_snapshot!("export-v4", text);
        } else {
            insta::assert_snapshot!("export-v5", text);
        }
    }
}

#[test]
fn v5_needs_a_single_collection() {
    let mut data = common::sample();
    data.collections.push(data.collections[0].clone());
    let err = reqly_interop::export(&data, reqly_interop::ExportFormat::Insomnia5, 0, "t", "x")
        .unwrap_err();
    assert_eq!(err.code, "export.single_collection");
}
