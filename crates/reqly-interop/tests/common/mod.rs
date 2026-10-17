//! Shared fixtures for interop tests.
#![allow(dead_code)]

use reqly_core::model::{
    ApiKeyLocation, Auth, Body, BodyKind, FolderData, HttpRequest, KeyValue, RequestSettings,
};
use reqly_interop::{
    ExportData, ImportedCollection, ImportedCookie, ImportedEnvironment, ImportedItem,
};
use serde_json::json;

pub fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!("{}/fixtures/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A collection exercising every feature the formats can express.
pub fn sample() -> ExportData {
    let mut login = HttpRequest {
        method: "POST".into(),
        url: "{{ base_url }}/auth/login".into(),
        headers: vec![KeyValue::new("Content-Type", "application/json")],
        body: Body {
            kind: BodyKind::Json,
            text: "{\"user\": \"{{ user }}\"}".into(),
            ..Body::default()
        },
        description: "Logs in.".into(),
        ..HttpRequest::default()
    };
    login.variables.insert("user".into(), json!("admin"));
    let list = HttpRequest {
        method: "GET".into(),
        url: "{{ base_url }}/users".into(),
        params: vec![
            KeyValue::new("page", "1"),
            KeyValue::new("debug", "true").disabled(),
        ],
        auth: Auth::Bearer {
            token: "{% response 'body', 'req-login', '$.token', 'no-history' %}".into(),
            prefix: String::new(),
        },
        ..HttpRequest::default()
    };
    let form = HttpRequest {
        method: "POST".into(),
        url: "{{ base_url }}/upload".into(),
        body: Body {
            kind: BodyKind::FormUrlencoded,
            fields: vec![KeyValue::new("a", "1"), KeyValue::new("b", "{{ $uuid }}")],
            ..Body::default()
        },
        auth: Auth::ApiKey {
            key: "X-Key".into(),
            value: "{{ key }}".into(),
            location: ApiKeyLocation::Header,
        },
        ..HttpRequest::default()
    };
    let folder = FolderData {
        headers: vec![KeyValue::new("X-Team", "core")],
        auth: Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        settings: RequestSettings {
            timeout_ms: Some(5000),
            ..RequestSettings::default()
        },
        description: "User endpoints".into(),
        variables: json!({ "version": "v2" }).as_object().unwrap().clone(),
        ..FolderData::default()
    };
    let mut variables = serde_json::Map::new();
    variables.insert("base_url".into(), json!("https://api.example.com"));
    variables.insert("key".into(), json!("secret-key"));
    ExportData {
        workspace: "Main".into(),
        collections: vec![ImportedCollection {
            name: "Example API".into(),
            description: "Sample".into(),
            settings: RequestSettings::default(),
            variables,
            variable_secrets: vec!["key".into()],
            environments: vec![ImportedEnvironment {
                name: "prod".into(),
                color: Some("#e5484d".into()),
                data: json!({ "base_url": "https://prod.example.com", "limits": { "rps": 10 } })
                    .as_object()
                    .unwrap()
                    .clone(),
                secrets: vec![],
            }],
            items: vec![
                ImportedItem::request("Login", login).with_id("req-login"),
                ImportedItem::folder(
                    "Users",
                    folder,
                    vec![
                        ImportedItem::request("List users", list).with_id("req-list"),
                        ImportedItem::request("Upload", form).with_id("req-upload"),
                    ],
                )
                .with_id("fld-users"),
            ],
        }],
        environments: vec![ImportedEnvironment {
            name: "Shared".into(),
            data: json!({ "user": "guest" }).as_object().unwrap().clone(),
            ..ImportedEnvironment::default()
        }],
        cookies: vec![ImportedCookie {
            domain: "example.com".into(),
            path: "/".into(),
            name: "sid".into(),
            value: "abc".into(),
            ..ImportedCookie::default()
        }],
        history: vec![],
    }
}

/// Collections with key/value row ids cleared (they are regenerated on import).
pub fn without_row_ids(mut collections: Vec<ImportedCollection>) -> Vec<ImportedCollection> {
    fn rows(rows: &mut [KeyValue]) {
        rows.iter_mut()
            .for_each(|r| r.id = reqly_core::Id::from(uuid_nil()));
    }
    fn uuid_nil() -> uuid::Uuid {
        uuid::Uuid::nil()
    }
    fn item(i: &mut ImportedItem) {
        match i {
            ImportedItem::Folder { data, children, .. } => {
                rows(&mut data.headers);
                children.iter_mut().for_each(item);
            }
            ImportedItem::Request { data, .. } => {
                rows(&mut data.params);
                rows(&mut data.path_params);
                rows(&mut data.headers);
                rows(&mut data.body.fields);
            }
        }
    }
    for c in &mut collections {
        c.items.iter_mut().for_each(item);
    }
    collections
}
