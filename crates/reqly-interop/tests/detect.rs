mod common;

use reqly_interop::{Format, Source, detect};

#[test]
fn every_fixture_is_detected() {
    let cases: &[(&str, Format)] = &[
        ("insomnia/v4-full.json", Format::Insomnia),
        ("insomnia/v5-full.yaml", Format::Insomnia),
        ("third-party/insomnia-4/basic-input.yaml", Format::Insomnia),
        ("postman/full-v2_1.json", Format::Postman),
        ("postman/globals.json", Format::Postman),
        (
            "third-party/postman/minimal-v2_0-input.json",
            Format::Postman,
        ),
        ("third-party/postman-env/basic-input.json", Format::Postman),
        ("third-party/curl/complex-input.sh", Format::Curl),
        ("third-party/har/deep-input.json", Format::Har),
        ("third-party/har/form-data-input.json", Format::Har),
        ("third-party/openapi3/petstore-input.json", Format::OpenApi),
        (
            "third-party/openapi3/petstore-yml-input.yml",
            Format::OpenApi,
        ),
        ("third-party/swagger2/petstore-input.json", Format::OpenApi),
        ("third-party/bruno/request.bru", Format::Bruno),
    ];
    for (path, format) in cases {
        let sources = [Source::new(*path, common::fixture(path))];
        assert_eq!(detect(&sources), Some(*format), "{path}");
    }
    let reqly = reqly_interop::reqly::export(&common::sample(), &Default::default()).unwrap();
    assert_eq!(detect(&[Source::new("x", reqly)]), Some(Format::Reqly));
}

#[test]
fn unknown_content() {
    for text in ["hello", "{\"a\": 1}", "a: b\n", "[1, 2]", ""] {
        assert_eq!(detect(&[Source::new("x", text)]), None, "{text}");
    }
}
