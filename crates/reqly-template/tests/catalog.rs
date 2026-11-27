//! The function catalog describes every argument for the tag editor.

use reqly_template::catalog;
use reqly_template::functions::ArgKind;

#[test]
fn every_argument_is_described() {
    for f in catalog() {
        assert_eq!(f.params.len(), f.args.len(), "{}", f.name);
        for (p, name) in f.params.iter().zip(&f.args) {
            // `args` keeps the old shape: optional ones end with `?`.
            let expected = if p.optional {
                format!("{}?", p.name)
            } else {
                p.name.clone()
            };
            assert_eq!(&expected, name, "{}", f.name);
            assert_eq!(
                p.kind == ArgKind::Choice,
                !p.choices.is_empty(),
                "{}.{}",
                f.name,
                p.name
            );
        }
        // Optional arguments come last.
        let first_optional = f
            .params
            .iter()
            .position(|p| p.optional)
            .unwrap_or(f.params.len());
        assert!(
            f.params[first_optional..].iter().all(|p| p.optional),
            "{}",
            f.name
        );
        // `choices` (completion) are the first argument's.
        assert_eq!(
            f.choices,
            f.params
                .first()
                .map(|p| p.choices.clone())
                .unwrap_or_default()
        );
    }
}

#[test]
fn response_picks_a_request_and_a_trigger() {
    let response = catalog()
        .into_iter()
        .find(|f| f.name == "response")
        .unwrap();
    let kinds: Vec<_> = response
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("field", ArgKind::Choice),
            ("requestId", ArgKind::Request),
            ("path", ArgKind::Text),
            ("trigger", ArgKind::Choice),
            ("maxAgeSeconds", ArgKind::Number),
        ]
    );
    assert_eq!(
        response.params[3].choices,
        ["never", "no-history", "when-expired", "always"]
    );
    let faker = catalog().into_iter().find(|f| f.name == "faker").unwrap();
    assert_eq!(faker.params[0].kind, ArgKind::Faker);
    let os = catalog().into_iter().find(|f| f.name == "os").unwrap();
    assert_eq!(os.args, ["field", "name?"]);
}
