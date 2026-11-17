//! `.http` / `.rest` files (JetBrains HTTP Client, VS Code REST Client) and
//! raw HTTP request text.

use reqly_core::model::{BodyKind, FileRef, HttpRequest, HttpVersion, KeyValueKind};
use reqly_interop::{Format, ImportedItem, Source, detect, import};

fn requests(items: &[ImportedItem]) -> Vec<(String, HttpRequest)> {
    let mut out = Vec::new();
    for item in items {
        match item {
            ImportedItem::Folder { children, .. } => out.extend(requests(children)),
            ImportedItem::Request { name, data, .. } => out.push((name.clone(), data.clone())),
            _ => {}
        }
    }
    out
}

fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

const FILE: &str = r#"@host = https://api.example.com
@token = secret

### List users
GET {{host}}/users?page=1&q=a%20b
Accept: application/json
Authorization: Bearer {{token}}

### Create a user
# @name createUser
POST {{host}}/users HTTP/1.1
Content-Type: application/json

{
  "name": "Ada",
  "id": "{{$uuid}}"
}

> {%
  client.global.set("id", response.body.id);
%}

###
// a form
POST {{host}}/login
Content-Type: application/x-www-form-urlencoded

user=ada&password=p%40ss

###
GET {{host}}/search
    ?q=reqly
    &limit=10
"#;

#[test]
fn detects_http_files_and_text() {
    assert_eq!(detect(&[Source::new("api.http", FILE)]), Some(Format::Http));
    assert_eq!(
        detect(&[Source::new("clipboard", FILE)]),
        Some(Format::Http)
    );
    assert_eq!(
        detect(&[Source::new("clipboard", "GET /a HTTP/1.1\nHost: x.test\n")]),
        Some(Format::Http)
    );
    assert_eq!(
        detect(&[Source::new("x.rest", "https://x.test/a\n")]),
        Some(Format::Http)
    );
    // Not HTTP: curl, JSON, prose.
    assert_eq!(
        detect(&[Source::new("clipboard", "curl https://x.test")]),
        Some(Format::Curl)
    );
    assert_eq!(detect(&[Source::new("clipboard", "{\"a\": 1}")]), None);
    assert_eq!(detect(&[Source::new("clipboard", "get the milk")]), None);
}

#[test]
fn imports_an_http_file() {
    let bundle = import(&[Source::new("api.http", FILE)], None).unwrap();
    assert_eq!(bundle.format, Format::Http);
    let c = &bundle.collections[0];
    assert_eq!(c.name, "api");
    // File variables become collection variables.
    assert_eq!(c.variables["host"], "https://api.example.com");
    assert_eq!(c.variables["token"], "secret");
    // Several requests: one folder named after the file.
    let ImportedItem::Folder { name, .. } = &c.items[0] else {
        panic!("{:?}", c.items)
    };
    assert_eq!(name, "api");

    let reqs = requests(&c.items);
    assert_eq!(reqs.len(), 4);

    let (name, list) = &reqs[0];
    assert_eq!(name, "List users");
    assert_eq!(list.method, "GET");
    assert_eq!(list.url, "{{host}}/users");
    let params: Vec<_> = list
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(params, [("page", "1"), ("q", "a b")]);
    assert_eq!(header(list, "Accept"), Some("application/json"));
    assert_eq!(header(list, "Authorization"), Some("Bearer {{token}}"));
    assert_eq!(list.body.kind, BodyKind::None);

    let (name, create) = &reqs[1];
    assert_eq!(name, "createUser", "# @name wins over the ### title");
    assert_eq!(create.method, "POST");
    assert_eq!(create.body.kind, BodyKind::Json);
    assert_eq!(
        create.body.text,
        "{\n  \"name\": \"Ada\",\n  \"id\": \"{{$guid}}\"\n}"
    );
    // The response handler uses another API: kept commented out and reported.
    assert!(create.scripts.after_response.contains("// "));
    assert!(create.scripts.after_response.contains("client.global.set"));
    assert!(
        bundle
            .report
            .entries
            .iter()
            .any(|e| e.code == "http_response_handler")
    );

    let (name, login) = &reqs[2];
    assert_eq!(name, "POST /login");
    assert_eq!(login.body.kind, BodyKind::FormUrlencoded);
    let fields: Vec<_> = login
        .body
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect();
    assert_eq!(fields, [("user", "ada"), ("password", "p@ss")]);
    assert_eq!(header(login, "Content-Type"), None);

    let (_, search) = &reqs[3];
    let params: Vec<_> = search
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(params, [("q", "reqly"), ("limit", "10")]);
}

#[test]
fn imports_raw_http_text() {
    let text = "POST /api/items?x=1 HTTP/2\r\nHost: shop.test:8080\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello";
    let bundle = import(&[Source::new("clipboard", text)], None).unwrap();
    let reqs = requests(&bundle.collections[0].items);
    assert_eq!(reqs.len(), 1, "one request is not wrapped in a folder");
    assert!(matches!(
        bundle.collections[0].items[0],
        ImportedItem::Request { .. }
    ));
    let (name, r) = &reqs[0];
    assert_eq!(name, "POST /api/items");
    assert_eq!(r.url, "http://shop.test:8080/api/items");
    assert_eq!(r.params[0].name, "x");
    assert_eq!(header(r, "Host"), None);
    assert_eq!(header(r, "Content-Length"), None);
    assert_eq!(r.body.kind, BodyKind::Text);
    assert_eq!(r.body.text, "hello");
    assert_eq!(r.settings.http_version, Some(HttpVersion::Http2));
}

#[test]
fn https_for_port_443_and_get_by_default() {
    let bundle = import(
        &[Source::new(
            "clipboard",
            "GET / HTTP/1.1\nHost: secure.test:443\n",
        )],
        None,
    )
    .unwrap();
    assert_eq!(
        requests(&bundle.collections[0].items)[0].1.url,
        "https://secure.test:443/"
    );
    let bundle = import(&[Source::new("a.http", "https://x.test/ping\n")], None).unwrap();
    let r = &requests(&bundle.collections[0].items)[0].1;
    assert_eq!(
        (r.method.as_str(), r.url.as_str()),
        ("GET", "https://x.test/ping")
    );
}

#[test]
fn file_bodies_and_multipart() {
    let text = r#"POST https://x.test/upload
Content-Type: application/octet-stream

< ./data/photo.png

###
POST https://x.test/form
Content-Type: multipart/form-data; boundary=WebAppBoundary

--WebAppBoundary
Content-Disposition: form-data; name="title"

Holiday
--WebAppBoundary
Content-Disposition: form-data; name="file"; filename="a.txt"
Content-Type: text/plain

< ./a.txt
--WebAppBoundary--
"#;
    let bundle = import(&[Source::new("u.http", text)], None).unwrap();
    let reqs = requests(&bundle.collections[0].items);
    let upload = &reqs[0].1;
    assert_eq!(upload.body.kind, BodyKind::Binary);
    assert_eq!(
        upload.body.file,
        Some(FileRef::Path {
            path: "./data/photo.png".into()
        })
    );
    let form = &reqs[1].1;
    assert_eq!(form.body.kind, BodyKind::Multipart);
    assert_eq!(form.body.fields.len(), 2);
    assert_eq!(
        (
            form.body.fields[0].name.as_str(),
            form.body.fields[0].value.as_str()
        ),
        ("title", "Holiday")
    );
    assert_eq!(form.body.fields[1].kind, KeyValueKind::File);
    assert_eq!(form.body.fields[1].value, "./a.txt");
    assert_eq!(header(form, "Content-Type"), None);
    // Relative file paths are reported: they resolve against the .http file.
    assert!(
        bundle
            .report
            .entries
            .iter()
            .any(|e| e.code == "http_file_ref")
    );
}

#[test]
fn several_files_become_folders() {
    let bundle = import(
        &[
            Source::new("users.http", "GET https://x.test/users\n"),
            Source::new("orders.http", "GET https://x.test/orders\n"),
        ],
        None,
    )
    .unwrap();
    let names: Vec<_> = bundle.collections[0]
        .items
        .iter()
        .map(|i| i.name().to_string())
        .collect();
    assert_eq!(names, ["users", "orders"]);
}

#[test]
fn rejects_text_without_requests() {
    let err = import(&[Source::new("a.http", "# only a comment\n")], None).unwrap_err();
    assert_eq!(err.code, "import.invalid");
}

#[test]
fn pasted_text_is_not_wrapped_in_a_folder() {
    let text = "GET https://x.test/a\n\n###\nGET https://x.test/b\n";
    let bundle = import(&[Source::new("clipboard", text)], None).unwrap();
    let c = &bundle.collections[0];
    assert_eq!(c.name, "HTTP import");
    let names: Vec<_> = c.items.iter().map(|i| i.name().to_string()).collect();
    assert_eq!(names, ["GET /a", "GET /b"]);

    let curl = "curl https://x.test/a\ncurl https://x.test/b\n";
    let bundle = import(&[Source::new("clipboard", curl)], None).unwrap();
    let c = &bundle.collections[0];
    assert_eq!(c.name, "cURL import");
    assert_eq!(c.items.len(), 2);
    assert!(matches!(c.items[0], ImportedItem::Request { .. }));
}
