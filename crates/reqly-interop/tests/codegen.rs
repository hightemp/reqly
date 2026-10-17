//! Snapshot of every code generation target over a set of requests.

use reqly_http::prepare::{MultipartPart, PreparedBody, PreparedRequest};
use reqly_interop::codegen::{CodeRequest, Options, TARGETS, generate};

fn prepared(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: PreparedBody,
) -> PreparedRequest {
    PreparedRequest {
        method: method.into(),
        url: url.into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body,
        ..Default::default()
    }
}

fn requests() -> Vec<(&'static str, CodeRequest)> {
    vec![
        (
            "GET with query and auth",
            prepared(
                "GET",
                "https://api.example.com/users?page=1&q=a%20b",
                &[
                    ("Accept", "application/json"),
                    ("Authorization", "Bearer t0k'en\"$x"),
                ],
                PreparedBody::None,
            ),
        ),
        (
            "POST JSON",
            prepared(
                "POST",
                "https://api.example.com/users",
                &[("Content-Type", "application/json")],
                PreparedBody::Bytes(b"{\"name\":\"Ann\",\"tags\":[\"a\"]}".to_vec()),
            ),
        ),
        (
            "PUT form",
            prepared(
                "PUT",
                "http://localhost:8080/form",
                &[("Content-Type", "application/x-www-form-urlencoded")],
                PreparedBody::Bytes(b"a=1&b=x+y%26z".to_vec()),
            ),
        ),
        (
            "POST multipart",
            prepared(
                "POST",
                "https://api.example.com/upload",
                &[("Content-Type", "multipart/form-data")],
                PreparedBody::Multipart(vec![
                    MultipartPart {
                        name: "title".into(),
                        value: "Hello".into(),
                        file: None,
                        blob: None,
                    },
                    MultipartPart {
                        name: "file".into(),
                        value: String::new(),
                        file: Some("/tmp/a.png".into()),
                        blob: None,
                    },
                ]),
            ),
        ),
        (
            "PATCH file",
            prepared(
                "PATCH",
                "https://api.example.com/raw",
                &[("Content-Type", "application/octet-stream")],
                PreparedBody::File("/tmp/data.bin".into()),
            ),
        ),
    ]
    .into_iter()
    .map(|(name, p)| (name, CodeRequest::from_prepared(&p)))
    .collect()
}

#[test]
fn snapshots() {
    let requests = requests();
    for (target, _, label, _) in TARGETS {
        let mut out = String::new();
        for (name, r) in &requests {
            out.push_str(&format!("// ===== {name} =====\n"));
            out.push_str(&generate(*target, r, Options::default()));
            out.push('\n');
        }
        insta::with_settings!({ snapshot_suffix => format!("{target:?}"), description => label.to_string() }, {
            insta::assert_snapshot!(out);
        });
    }
}

#[test]
fn form_bodies_are_decoded() {
    let r = &requests()[2].1;
    assert_eq!(
        r.body,
        reqly_interop::codegen::CodeBody::Form(vec![
            ("a".into(), "1".into()),
            ("b".into(), "x y&z".into())
        ])
    );
}

#[test]
fn multipart_content_type_is_left_to_the_library() {
    let r = &requests()[3].1;
    assert!(r.headers.is_empty());
}

/// Writes runnable samples for manual verification against an echo server:
/// `REQLY_ECHO=http://127.0.0.1:9999 REQLY_FILES=/tmp/x REQLY_CODEGEN_OUT=/tmp/out cargo test -p reqly-interop --test codegen -- --ignored`
#[test]
#[ignore]
fn write_samples() {
    let echo = std::env::var("REQLY_ECHO").expect("REQLY_ECHO");
    let files = std::env::var("REQLY_FILES").expect("REQLY_FILES");
    let out =
        std::path::PathBuf::from(std::env::var("REQLY_CODEGEN_OUT").expect("REQLY_CODEGEN_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let cases = vec![
        (
            "get",
            prepared(
                "GET",
                &format!("{echo}/get?page=1&q=a%20b"),
                &[("Accept", "application/json"), ("X-Quote", "it's \"q\" $x")],
                PreparedBody::None,
            ),
        ),
        (
            "json",
            prepared(
                "POST",
                &format!("{echo}/json"),
                &[("Content-Type", "application/json")],
                PreparedBody::Bytes(b"{\"name\":\"Ann\"}".to_vec()),
            ),
        ),
        (
            "form",
            prepared(
                "PUT",
                &format!("{echo}/form"),
                &[("Content-Type", "application/x-www-form-urlencoded")],
                PreparedBody::Bytes(b"a=1&b=x+y%26z".to_vec()),
            ),
        ),
        (
            "multipart",
            prepared(
                "POST",
                &format!("{echo}/multipart"),
                &[("Content-Type", "multipart/form-data")],
                PreparedBody::Multipart(vec![
                    MultipartPart {
                        name: "title".into(),
                        value: "Hello".into(),
                        file: None,
                        blob: None,
                    },
                    MultipartPart {
                        name: "file".into(),
                        value: String::new(),
                        file: Some(format!("{files}/a.txt").into()),
                        blob: None,
                    },
                ]),
            ),
        ),
        (
            "file",
            prepared(
                "PATCH",
                &format!("{echo}/file"),
                &[("Content-Type", "application/octet-stream")],
                PreparedBody::File(format!("{files}/data.bin").into()),
            ),
        ),
    ];
    for (target, _, _, _) in TARGETS {
        for (case, p) in &cases {
            let code = generate(*target, &CodeRequest::from_prepared(p), Options::default());
            std::fs::write(out.join(format!("{target:?}.{case}.txt")), code).unwrap();
        }
    }
}
