mod common;

use reqly_core::model::{Auth, BodyKind, KeyValueKind};
use reqly_interop::{Format, ImportedItem, ReportKind, Source, detect, import};

fn collection_sources() -> Vec<Source> {
    let root = format!("{}/fixtures/bruno", env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<Source>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(Source::new(rel, std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    walk(
        std::path::Path::new(&root),
        std::path::Path::new(&root),
        &mut out,
    );
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn find<'a>(items: &'a [ImportedItem], name: &str) -> &'a ImportedItem {
    for i in items {
        if i.name() == name {
            return i;
        }
        if let ImportedItem::Folder { children, .. } = i
            && let Some(f) = children.iter().find(|c| c.name() == name)
        {
            return f;
        }
    }
    panic!("{name}")
}

fn check(bundle: &reqly_interop::ImportBundle) {
    assert_eq!(bundle.format, Format::Bruno);
    let c = &bundle.collections[0];
    assert_eq!(c.name, "Shop API");
    let names: Vec<_> = c.items.iter().map(ImportedItem::name).collect();
    assert_eq!(names, ["Health", "Users", "Upload"], "ordered by seq");
    assert_eq!(c.variables["apiVersion"], "v1");
    let envs: Vec<_> = c.environments.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(envs, ["Local", "Prod"]);
    assert_eq!(c.environments[0].secrets, ["token"]);
    assert!(!c.environments[0].data.contains_key("legacy"));

    let ImportedItem::Folder {
        data: users,
        children,
        ..
    } = find(&c.items, "Users")
    else {
        panic!()
    };
    assert!(matches!(&users.auth, Auth::Basic { username, .. } if username == "admin"));
    let header_names: Vec<_> = users.headers.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        header_names,
        ["X-Client", "X-Team"],
        "collection headers first"
    );
    let child_names: Vec<_> = children.iter().map(ImportedItem::name).collect();
    assert_eq!(child_names, ["List users", "Create user"]);

    let ImportedItem::Request { data: list, .. } = find(&c.items, "List users") else {
        panic!()
    };
    assert_eq!(list.url, "{{baseUrl}}/{{apiVersion}}/users");
    assert_eq!(list.params.len(), 3);
    assert!(!list.params[2].enabled);
    assert!(list.scripts.after_response.contains("// tests"));

    let ImportedItem::Request { data: create, .. } = find(&c.items, "Create user") else {
        panic!()
    };
    assert_eq!(create.body.kind, BodyKind::Json);
    assert!(create.body.text.contains("\"name\": \"{{name}}\""));
    assert_eq!(create.path_params[0].value, "acme");
    assert_eq!(create.variables["name"], "Alice");
    assert_eq!(create.auth, Auth::None);
    assert_eq!(create.description, "Creates a **user**.");

    let ImportedItem::Request { data: upload, .. } = find(&c.items, "Upload") else {
        panic!()
    };
    assert_eq!(upload.body.kind, BodyKind::Multipart);
    assert_eq!(upload.body.fields[1].kind, KeyValueKind::File);
    assert_eq!(upload.body.fields[1].value, "/tmp/a.png");
    assert!(
        matches!(&upload.auth, Auth::ApiKey { value, .. } if value == "{{ _['process.env.API_KEY'] }}")
    );

    let ImportedItem::Request { data: health, .. } = find(&c.items, "Health") else {
        panic!()
    };
    assert!(matches!(&health.auth, Auth::None));

    let codes: Vec<_> = bundle
        .report
        .entries
        .iter()
        .map(|e| (e.kind, e.code.as_str()))
        .collect();
    for expected in [
        (ReportKind::Unsupported, "protocol_unsupported"),
        (ReportKind::Unsupported, "assert_unsupported"),
        (ReportKind::Unsupported, "process_env"),
        (ReportKind::Converted, "collection_settings"),
        (ReportKind::Info, "secrets_empty"),
    ] {
        assert!(codes.contains(&expected), "{expected:?} in {codes:?}");
    }
}

#[test]
fn directory() {
    let sources = collection_sources();
    assert_eq!(detect(&sources), Some(Format::Bruno));
    check(&import(&sources, None).unwrap());
}

#[test]
fn zip_archive() {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        for s in collection_sources() {
            zip.start_file(format!("export/{}", s.name), options)
                .unwrap();
            zip.write_all(s.content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    let sources = reqly_interop::unzip(buf.get_ref()).unwrap();
    check(&import(&sources, None).unwrap());
}

#[test]
fn single_bru_files() {
    let req = common::fixture("third-party/bruno/request.bru");
    let bundle = import(&[Source::new("request.bru", req)], None).unwrap();
    let c = &bundle.collections[0];
    let ImportedItem::Request { name, data, .. } = &c.items[0] else {
        panic!()
    };
    assert_eq!(name, "Send Bulk SMS");
    assert_eq!(data.method, "GET");
    assert_eq!(data.body.kind, BodyKind::Json);
    assert!(data.headers.iter().any(|h| h.name == "colon:header"));
    assert!(matches!(&data.auth, Auth::Bearer { token, .. } if token == "123"));
    assert_eq!(
        data.description,
        "This request needs auth token to be set in the headers."
    );
}
