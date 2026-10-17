mod common;

use reqly_core::model::{ApiKeyLocation, Auth, BodyKind, KeyValueKind};
use reqly_interop::{
    ExportFormat, Format, ImportedItem, ReportKind, Source, detect, export, import,
};

fn load(path: &str) -> reqly_interop::ImportBundle {
    let sources = [Source::new(path, common::fixture(path))];
    assert_eq!(detect(&sources), Some(Format::Postman), "{path}");
    import(&sources, None).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn request<'a>(items: &'a [ImportedItem], name: &str) -> &'a reqly_core::model::HttpRequest {
    find(items, name).unwrap_or_else(|| panic!("{name} not found"))
}

fn find<'a>(items: &'a [ImportedItem], name: &str) -> Option<&'a reqly_core::model::HttpRequest> {
    for i in items {
        match i {
            ImportedItem::Request { name: n, data, .. } if n == name => return Some(data),
            ImportedItem::Folder { children, .. } => {
                if let Some(found) = find(children, name) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn full_collection() {
    let bundle = load("postman/full-v2_1.json");
    let c = &bundle.collections[0];
    assert_eq!(c.name, "Shop API");
    assert_eq!(c.description, "Orders and **products**.");
    assert_eq!(c.variables["api.host"], "https://shop.example.com");
    assert_eq!(c.variable_secrets, ["token"]);
    assert!(!c.variables.contains_key("unused"));

    let list = request(&c.items, "List products");
    assert_eq!(list.url, "{{ _['api.host'] }}/v1/products");
    let params: Vec<_> = list
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str(), p.enabled))
        .collect();
    assert_eq!(
        params,
        [
            ("limit", "{{ _['page size'] }}", true),
            ("sort", "price", true),
            ("debug", "1", false)
        ]
    );
    assert_eq!(list.settings.follow_redirects, Some(false));
    assert_eq!(list.settings.encode_url, Some(false));
    assert!(list.scripts.after_response.contains("pm.test"));
    assert!(!list.headers[1].enabled);
    assert_eq!(list.auth, Auth::Inherit, "inherits the folder's api key");

    let ImportedItem::Folder { data: products, .. } = &c.items[0] else {
        panic!()
    };
    assert!(
        matches!(&products.auth, Auth::ApiKey { key, location: ApiKeyLocation::Query, .. } if key == "api_key")
    );

    let get = request(&c.items, "Get product");
    assert_eq!(get.path_params[0].name, "productId");
    assert_eq!(get.path_params[0].value, "42");
    assert_eq!(get.path_params[0].description, "Product id");

    let order = request(&c.items, "Create order");
    assert_eq!(order.body.kind, BodyKind::Json);
    assert!(order.body.text.contains("{{$randomEmail}}"));
    assert_eq!(order.description, "Creates an order.");
    assert!(matches!(&order.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
    assert!(order.scripts.pre_request.contains("pm.variables.set"));

    let gql = request(&c.items, "GraphQL search");
    assert_eq!(gql.body.kind, BodyKind::Json);
    assert!(gql.body.text.contains("\"query\""));
    assert_eq!(gql.url, "https://shop.example.com/graphql");

    let upload = request(&c.items, "Upload image");
    assert_eq!(upload.body.kind, BodyKind::Binary);

    // Collection basic auth goes to top-level items that inherit.
    assert!(
        matches!(&request(&c.items, "GraphQL search").auth, Auth::Basic { username, .. } if username == "admin")
    );
    assert_eq!(request(&c.items, "No auth").auth, Auth::None);
    assert!(
        matches!(&request(&c.items, "Signed call").auth, Auth::AwsSigV4 { access_key, region, .. } if access_key == "AKIA" && region == "eu-west-1")
    );

    let codes: Vec<_> = bundle
        .report
        .entries
        .iter()
        .map(|e| (e.kind, e.code.as_str(), e.path.as_str()))
        .collect();
    for expected in [
        (ReportKind::Unsupported, "collection_scripts", "Shop API"),
        (ReportKind::Converted, "collection_auth", "Shop API"),
        (
            ReportKind::Converted,
            "graphql_as_json",
            "Shop API / GraphQL search",
        ),
        (
            ReportKind::Info,
            "examples_skipped",
            "Shop API / Products / List products",
        ),
        (ReportKind::Info, "variable_disabled", "Shop API"),
        (
            ReportKind::Unsupported,
            "script_kept",
            "Shop API / Create order",
        ),
    ] {
        assert!(codes.contains(&expected), "{expected:?} in {codes:#?}");
    }
    insta::assert_json_snapshot!("full-report", bundle.report);
}

#[test]
fn third_party_fixtures() {
    let dir = format!(
        "{}/fixtures/third-party/postman",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    for name in names {
        let bundle = load(&format!("third-party/postman/{name}"));
        assert_eq!(bundle.collections.len(), 1, "{name}");
        assert!(bundle.collections[0].request_count() > 0, "{name}");
    }
    let v20 = load("third-party/postman/basic-auth-v2_0-input.json");
    assert!(matches!(
        &v20.collections[0].items[0],
        ImportedItem::Request { data, .. } if matches!(&data.auth, Auth::Basic { username, .. } if username == "basic-username")
    ));
    let first_auth = |name: &str| {
        let b = load(&format!("third-party/postman/{name}"));
        fn walk(items: &[ImportedItem]) -> Option<Auth> {
            items.iter().find_map(|i| match i {
                ImportedItem::Request { data, .. }
                    if !matches!(data.auth, Auth::Inherit | Auth::None) =>
                {
                    Some(data.auth.clone())
                }
                ImportedItem::Folder { children, .. } => walk(children),
                _ => None,
            })
        }
        assert!(
            !b.report
                .entries
                .iter()
                .any(|e| e.code == "auth_unsupported"),
            "{name}"
        );
        walk(&b.collections[0].items).unwrap()
    };
    assert_eq!(
        first_auth("digest-auth-v2_0-input.json"),
        Auth::Digest {
            username: "guest".into(),
            password: "guest".into()
        }
    );
    assert!(matches!(
        first_auth("aws-signature-auth-v2_1-input.json"),
        Auth::AwsSigV4 { region, service, .. } if region == "aws-region" && service == "aws-service-name"
    ));
    assert!(matches!(
        first_auth("oauth1_0-auth-v2_1-input.json"),
        Auth::OAuth1 { realm, location: reqly_core::model::OAuth1Location::Query, .. } if realm == "realm-value"
    ));
    let Auth::OAuth2(oauth) = first_auth("oauth2_0-auth-v2_1-input.json") else {
        panic!()
    };
    assert_eq!(oauth.token_prefix, "test");
    assert_eq!(oauth.audience, "test");
    let complex = load("third-party/postman/complex-v2_1-input.json");
    let first = request(&complex.collections[0].items, "{{base_url}}/api/users");
    assert_eq!(first.body.kind, BodyKind::Multipart);
    assert_eq!(first.body.fields[0].kind, KeyValueKind::File);
    assert!(!first.body.fields[2].enabled);
}

#[test]
fn environments_and_globals() {
    let env = load("third-party/postman-env/basic-input.json");
    assert_eq!(env.environments[0].name, "Production Env");
    assert_eq!(env.environments[0].data["foo"], "production");
    assert!(!env.environments[0].data.contains_key("bar"));

    let globals = load("postman/globals.json");
    assert_eq!(globals.environments[0].name, "My Workspace Globals");
    assert_eq!(globals.environments[0].secrets, ["password"]);

    // Collection + environment files together.
    let sources = [
        Source::new("c.json", common::fixture("postman/full-v2_1.json")),
        Source::new(
            "e.json",
            common::fixture("third-party/postman-env/basic-input.json"),
        ),
    ];
    let both = import(&sources, None).unwrap();
    assert_eq!(both.collections.len(), 1);
    assert_eq!(both.environments.len(), 1);
}

#[test]
fn round_trip() {
    let data = common::sample();
    let files = export(&data, ExportFormat::Postman, 0, "t", "Example API").unwrap();
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "example-api.postman_collection.json",
            "example-api.prod.postman_environment.json",
            "example-api.shared.postman_environment.json"
        ]
    );
    let sources: Vec<_> = files
        .iter()
        .map(|f| Source::new(&f.name, &f.content))
        .collect();
    let bundle = import(&sources, None).unwrap();
    let c = &bundle.collections[0];
    assert_eq!(c.variables, data.collections[0].variables);
    assert_eq!(c.variable_secrets, ["key"]);
    let list = request(&c.items, "List users");
    assert_eq!(list.url, "{{base_url}}/users");
    assert!(!list.params[1].enabled);
    let upload = request(&c.items, "Upload");
    assert_eq!(upload.body.kind, BodyKind::FormUrlencoded);
    assert_eq!(upload.body.fields[1].value, "{{$guid}}");
    let ImportedItem::Folder { data: folder, .. } = &c.items[1] else {
        panic!()
    };
    assert!(matches!(&folder.auth, Auth::Basic { .. }));
    assert_eq!(folder.variables["version"], "v2");
    let names: Vec<_> = bundle
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["prod", "Shared"]);
    insta::assert_snapshot!("export-collection", files[0].content);
}
