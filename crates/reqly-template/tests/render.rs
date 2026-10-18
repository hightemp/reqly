use reqly_core::CoreResult;
use reqly_template::{Host, Locale, RenderOptions, Renderer};
use serde_json::{Value, json};

struct NoHost;

#[async_trait::async_trait]
impl Host for NoHost {
    async fn call(&self, _: &str, _: &[Value]) -> Option<CoreResult<Value>> {
        None
    }
}

struct EchoHost;

#[async_trait::async_trait]
impl Host for EchoHost {
    async fn call(&self, name: &str, args: &[Value]) -> Option<CoreResult<Value>> {
        (name == "cookie").then(|| Ok(json!(format!("cookie:{}", args[1].as_str().unwrap_or("")))))
    }
}

fn vars() -> Value {
    json!({
        "base": "https://api.test",
        "api": { "version": "v2", "hosts": ["a", "b"] },
        "token": "abc",
        "full": "{{ base }}/{{ api.version }}",
        "loop1": "{{ loop2 }}",
        "loop2": "{{ loop1 }}",
        "num": 42,
        "flag": true,
        "obj": { "x": 1 },
        "nothing": null
    })
}

async fn render(src: &str) -> CoreResult<String> {
    Renderer::new(&vars(), &NoHost, RenderOptions::default())
        .render(src)
        .await
}

#[tokio::test]
async fn variables_paths_and_types() {
    assert_eq!(
        render("{{ base }}/{{ _.api.version }}/{{ api.hosts[1] }}")
            .await
            .unwrap(),
        "https://api.test/v2/b"
    );
    assert_eq!(
        render("{{num}}|{{flag}}|{{obj}}|{{nothing}}")
            .await
            .unwrap(),
        "42|true|{\"x\":1}|"
    );
}

#[tokio::test]
async fn variables_can_contain_templates() {
    assert_eq!(
        render("{{ full }}/users").await.unwrap(),
        "https://api.test/v2/users"
    );
}

#[tokio::test]
async fn cycles_are_detected() {
    let err = render("{{ loop1 }}").await.unwrap_err();
    assert_eq!(err.code, "template.cycle");
}

#[tokio::test]
async fn undefined_variables_fail_with_their_name() {
    let err = render("x {{ missing.deep }}").await.unwrap_err();
    assert_eq!(err.code, "template.undefined");
    assert_eq!(err.params["name"], "missing.deep");
}

#[tokio::test]
async fn lenient_mode_renders_missing_as_empty() {
    let opts = RenderOptions {
        strict: false,
        ..RenderOptions::default()
    };
    assert_eq!(
        Renderer::new(&vars(), &NoHost, opts)
            .render("[{{ missing }}]")
            .await
            .unwrap(),
        "[]"
    );
}

#[tokio::test]
async fn encoding_functions() {
    assert_eq!(
        render("{% base64 'encode', 'hi there' %}").await.unwrap(),
        "aGkgdGhlcmU="
    );
    assert_eq!(
        render("{% base64 'decode', 'aGkgdGhlcmU=' %}")
            .await
            .unwrap(),
        "hi there"
    );
    assert_eq!(
        render("{{ $base64('encode', 'url', '??>') }}")
            .await
            .unwrap(),
        "Pz8-"
    );
    assert_eq!(
        render("{% urlencode 'encode', 'a b&c' %}").await.unwrap(),
        "a%20b%26c"
    );
    assert_eq!(
        render("{% urlencode 'decode', 'a%20b' %}").await.unwrap(),
        "a b"
    );
}

#[tokio::test]
async fn hashing() {
    assert_eq!(
        render("{% hash 'md5', 'hex', 'abc' %}").await.unwrap(),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        render("{% hash 'sha1', 'hex', 'abc' %}").await.unwrap(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        render("{% hash 'sha256', 'hex', 'abc' %}").await.unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        render("{% hash 'sha256', 'base64', 'abc' %}")
            .await
            .unwrap(),
        "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
    );
    assert_eq!(
        render("{% hmac 'sha256', 'hex', 'key', 'The quick brown fox jumps over the lazy dog' %}")
            .await
            .unwrap(),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(
        render("{% hash 'sha256', 'hex', token %}")
            .await
            .unwrap()
            .len(),
        64
    );
}

#[tokio::test]
async fn uuid_and_time() {
    let v4 = render("{% uuid 'v4' %}").await.unwrap();
    assert_eq!(uuid::Uuid::parse_str(&v4).unwrap().get_version_num(), 4);
    let v7 = render("{{ $uuid('v7') }}").await.unwrap();
    assert_eq!(uuid::Uuid::parse_str(&v7).unwrap().get_version_num(), 7);
    let unix: i64 = render("{% now 'unix' %}").await.unwrap().parse().unwrap();
    assert!(unix > 1_767_225_600);
    let millis: i64 = render("{% now 'millis' %}").await.unwrap().parse().unwrap();
    assert!(millis / 1000 >= unix);
    assert!(render("{% now 'iso-8601' %}").await.unwrap().contains('T'));
    assert_eq!(render("{% now 'custom', '%Y' %}").await.unwrap().len(), 4);
}

#[tokio::test]
async fn random_values() {
    for _ in 0..20 {
        let n: i64 = render("{% random 'int', 5, 7 %}")
            .await
            .unwrap()
            .parse()
            .unwrap();
        assert!((5..=7).contains(&n));
        let f: f64 = render("{% random 'float', 0, 1 %}")
            .await
            .unwrap()
            .parse()
            .unwrap();
        assert!((0.0..=1.0).contains(&f));
    }
    assert_eq!(render("{% random 'string', 12 %}").await.unwrap().len(), 12);
    let r = render("{% random 'regex', '[A-Z]{3}-[0-9]{4}' %}")
        .await
        .unwrap();
    assert!(r.len() == 8 && r.as_bytes()[3] == b'-', "{r}");
}

#[tokio::test]
async fn jsonpath_and_jwt() {
    assert_eq!(
        render("{% jsonpath api, '$.hosts[0]' %}").await.unwrap(),
        "a"
    );
    let token = render("{% jwt 'encode', '{\"sub\":\"1\"}', 'secret', 'HS256' %}")
        .await
        .unwrap();
    assert_eq!(token.split('.').count(), 3);
    let src = format!("{{% jwt 'decode', '{token}', 'payload' %}}");
    assert_eq!(render(&src).await.unwrap(), "{\"sub\":\"1\"}");
}

#[tokio::test]
async fn postman_dynamic_variables() {
    assert_eq!(
        uuid::Uuid::parse_str(&render("{{$guid}}").await.unwrap())
            .unwrap()
            .get_version_num(),
        4
    );
    assert!(
        render("{{$timestamp}}")
            .await
            .unwrap()
            .parse::<i64>()
            .is_ok()
    );
    assert!(
        render("{{$randomInt}}")
            .await
            .unwrap()
            .parse::<i64>()
            .is_ok()
    );
    assert!(!render("{{$randomFirstName}}").await.unwrap().is_empty());
    assert!(render("{{$randomEmail}}").await.unwrap().contains('@'));
}

#[tokio::test]
async fn faker_in_both_locales() {
    let v = vars();
    let en = Renderer::new(
        &v,
        &NoHost,
        RenderOptions {
            locale: Locale::EN,
            ..RenderOptions::default()
        },
    );
    let ru = Renderer::new(
        &v,
        &NoHost,
        RenderOptions {
            locale: Locale::RU,
            ..RenderOptions::default()
        },
    );
    let name = en.render("{{ $faker.person.firstName }}").await.unwrap();
    assert!(
        name.chars().all(|c| c.is_ascii_alphabetic() || c == '-'),
        "{name}"
    );
    let ru_name = ru.render("{{ $faker.person.firstName }}").await.unwrap();
    assert!(
        ru_name
            .chars()
            .all(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap())
                || c == 'ё'
                || c == 'Ё'),
        "{ru_name}"
    );
    let phone = ru
        .render("{{ $faker.phone.number({ style: 'international' }) }}")
        .await
        .unwrap();
    assert!(phone.starts_with("+7"), "{phone}");
    let n: i64 = en
        .render("{{ $faker.number.int(10, 12) }}")
        .await
        .unwrap()
        .parse()
        .unwrap();
    assert!((10..=12).contains(&n));
    assert!(
        en.render("{{ $faker.internet.email }}")
            .await
            .unwrap()
            .contains('@')
    );
    assert_eq!(
        en.render("{% faker 'nope.nope' %}").await.unwrap_err().code,
        "template.faker_unknown"
    );
}

#[tokio::test]
async fn faker_takes_arrays_objects_and_variables() {
    let v = json!({ "pick": "x", "n": 4 });
    let r = Renderer::new(&v, &NoHost, RenderOptions::default());
    assert_eq!(
        r.render("{{ $faker.helpers.arrayElement([pick]) }}")
            .await
            .unwrap(),
        "x"
    );
    assert_eq!(
        r.render("{{ $faker.number.int({ min: n, 'max': 4 }) }}")
            .await
            .unwrap(),
        "4"
    );
    let s = r
        .render("{% faker 'string.alpha', { length: n, casing: 'upper' } %}")
        .await
        .unwrap();
    assert!(
        s.len() == 4 && s.chars().all(|c| c.is_ascii_uppercase()),
        "{s}"
    );
    assert_eq!(
        r.render("{{ $faker.helpers.arrayElements([1, 2], 2) }}")
            .await
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn faker_supports_every_locale() {
    let v = vars();
    for code in ["de", "fr", "ja", "pt_BR", "uk", "zh_CN"] {
        let r = Renderer::new(
            &v,
            &NoHost,
            RenderOptions {
                locale: Locale::parse(code).unwrap(),
                ..RenderOptions::default()
            },
        );
        assert!(
            !r.render("{{ $faker.location.city }}")
                .await
                .unwrap()
                .is_empty(),
            "{code}"
        );
    }
}

#[tokio::test]
async fn every_postman_dynamic_variable_renders() {
    let v = vars();
    let r = Renderer::new(&v, &NoHost, RenderOptions::default());
    for name in reqly_template::dynamic_variables() {
        let out = r.render(&format!("{{{{${name}}}}}")).await;
        assert!(out.as_ref().is_ok_and(|s| !s.is_empty()), "{name}: {out:?}");
    }
}

#[tokio::test]
async fn stable_mode_reuses_values_within_one_render() {
    let v = vars();
    let r = Renderer::new(
        &v,
        &NoHost,
        RenderOptions {
            stable: true,
            ..RenderOptions::default()
        },
    );
    let out = r.render("{{ $uuid }}|{{ $uuid }}").await.unwrap();
    let (a, b) = out.split_once('|').unwrap();
    assert_eq!(a, b);
    let normal = render("{{ $uuid }}|{{ $uuid }}").await.unwrap();
    let (a, b) = normal.split_once('|').unwrap();
    assert_ne!(a, b);
}

#[tokio::test]
async fn host_functions_and_unknown_functions() {
    let v = vars();
    let r = Renderer::new(&v, &EchoHost, RenderOptions::default());
    assert_eq!(
        r.render("{% cookie 'https://a', 'sid' %}").await.unwrap(),
        "cookie:sid"
    );
    assert_eq!(
        render("{% nonexistent %}").await.unwrap_err().code,
        "template.unknown_function"
    );
}

#[tokio::test]
async fn bad_arguments_are_reported() {
    let err = render("{% hash 'nope', 'hex', 'x' %}").await.unwrap_err();
    assert_eq!(err.code, "template.bad_argument");
    assert_eq!(err.params["function"], "hash");
}

#[tokio::test]
async fn text_without_templates_is_unchanged() {
    assert_eq!(
        render("{\"a\": {\"b\": 1}}").await.unwrap(),
        "{\"a\": {\"b\": 1}}"
    );
}
