//! Scripts as Postman and Insomnia users write them, the sandbox and limits.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqly_script::{
    Limits, Pair, Phase, Scopes, ScriptBody, ScriptHost, ScriptInfo, ScriptInput, ScriptOutput,
    ScriptRequest, ScriptResponse, SendSpec, VariableScope, run,
};
use serde_json::json;

#[derive(Default)]
struct Host {
    sent: Mutex<Vec<SendSpec>>,
}

impl ScriptHost for Host {
    async fn send(&self, request: SendSpec) -> ScriptResponse {
        self.sent.lock().unwrap().push(request.clone());
        tokio::time::sleep(Duration::from_millis(5)).await;
        if request.url.contains("fail") {
            return ScriptResponse {
                error: Some("connection refused".into()),
                ..Default::default()
            };
        }
        ScriptResponse {
            code: 200,
            status: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: json!({ "token": "t-123", "echo": request.url }).to_string(),
            time_ms: 5.0,
            size: 30,
            error: None,
        }
    }
}

fn request() -> ScriptRequest {
    ScriptRequest {
        id: "r1".into(),
        name: "Login".into(),
        method: "POST".into(),
        url: "https://api.test/login".into(),
        query: vec![Pair {
            key: "a".into(),
            value: "1".into(),
            disabled: false,
        }],
        headers: vec![Pair {
            key: "Accept".into(),
            value: "application/json".into(),
            disabled: false,
        }],
        body: ScriptBody {
            mode: "raw".into(),
            raw: "{\"user\":\"u\"}".into(),
            fields: vec![],
        },
        auth: json!({ "type": "none" }),
    }
}

fn response(code: u16, body: serde_json::Value) -> ScriptResponse {
    ScriptResponse {
        code,
        status: if code == 200 {
            "OK".into()
        } else {
            "Not Found".into()
        },
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("X-Id".into(), "7".into()),
        ],
        body: body.to_string(),
        time_ms: 12.0,
        size: 20,
        error: None,
    }
}

fn input(phase: Phase, script: &str) -> ScriptInput {
    ScriptInput {
        phase,
        label: "test script".into(),
        script: script.into(),
        request: request(),
        response: (phase == Phase::Post)
            .then(|| response(200, json!({ "token": "abc", "items": [1, 2, 3] }))),
        variables: Scopes {
            globals: serde_json::from_value(json!({ "g": "global", "shared": "from-globals" }))
                .unwrap(),
            collection: serde_json::from_value(json!({ "base": "https://api.test" })).unwrap(),
            environment: serde_json::from_value(json!({ "shared": "from-env", "secret": "s3" }))
                .unwrap(),
            ..Scopes::default()
        },
        info: ScriptInfo {
            request_name: "Login".into(),
            request_id: "r1".into(),
            iteration: 0,
            iteration_count: 1,
        },
        limits: Limits::default(),
        locale: reqly_template::Locale::EN,
    }
}

async fn exec(input: ScriptInput) -> ScriptOutput {
    run(Arc::new(Host::default()), input)
        .await
        .unwrap_or_else(|e| panic!("{e:?}"))
}

#[tokio::test]
async fn token_from_response_and_tests() {
    let out = exec(input(
        Phase::Post,
        r#"
        const json = pm.response.json();
        pm.environment.set('token', json.token);
        pm.collectionVariables.set('count', json.items.length);
        pm.test('status is 200', () => pm.response.to.have.status(200));
        pm.test('has items', function () { pm.expect(json.items).to.have.lengthOf(3); });
        pm.test('header', () => { pm.response.to.have.header('X-Id', '7'); });
        pm.test('fails', () => pm.expect(json.token).to.eql('nope'));
        tests['legacy'] = responseCode.code === 200;
        "#,
    ))
    .await;
    let changes: Vec<(VariableScope, &str, serde_json::Value)> = out
        .changes
        .iter()
        .map(|c| (c.scope, c.key.as_str(), c.value.clone()))
        .collect();
    assert_eq!(
        changes,
        [
            (VariableScope::Environment, "token", json!("abc")),
            (VariableScope::Collection, "count", json!(3))
        ]
    );
    let results: Vec<(&str, bool)> = out
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed))
        .collect();
    assert_eq!(
        results,
        [
            ("status is 200", true),
            ("has items", true),
            ("header", true),
            ("fails", false),
            ("legacy", true)
        ]
    );
    assert!(
        out.tests[3]
            .error
            .as_deref()
            .unwrap()
            .contains("expected 'abc' to deeply equal 'nope'"),
        "{:?}",
        out.tests[3]
    );
}

#[tokio::test]
async fn pre_request_signing_and_request_changes() {
    let out = exec(input(
        Phase::Pre,
        r#"
        const ts = '1700000000';
        const sig = CryptoJS.HmacSHA256(pm.request.body.raw + ts, pm.environment.get('secret')).toString(CryptoJS.enc.Hex);
        pm.request.headers.add({ key: 'X-Signature', value: sig });
        pm.request.headers.upsert({ key: 'Accept', value: 'text/plain' });
        pm.request.headers.remove('Nope');
        pm.request.url.query.add({ key: 'ts', value: ts });
        pm.request.method = 'put';
        pm.request.body.update(JSON.stringify({ user: 'u', ts }));
        pm.variables.set('signature', sig);
        console.log('signed', { sig: sig.slice(0, 8) });
        "#,
    ))
    .await;
    let r = &out.request;
    assert_eq!(r.method, "PUT");
    let sig = r
        .headers
        .iter()
        .find(|h| h.key == "X-Signature")
        .unwrap()
        .value
        .clone();
    assert_eq!(sig.len(), 64);
    assert_eq!(
        r.headers.iter().find(|h| h.key == "Accept").unwrap().value,
        "text/plain"
    );
    assert_eq!(
        r.query.iter().map(|q| q.key.as_str()).collect::<Vec<_>>(),
        ["a", "ts"]
    );
    assert_eq!(r.body.raw, r#"{"user":"u","ts":"1700000000"}"#);
    assert_eq!(out.changes[0].scope, VariableScope::Local);
    assert_eq!(out.logs[0].level, "log");
    assert!(out.logs[0].text.starts_with("signed {"), "{:?}", out.logs);
}

#[tokio::test]
async fn variables_resolve_in_postman_order() {
    let out = exec(input(
        Phase::Pre,
        r#"
        console.log(pm.variables.get('shared'), pm.variables.get('g'), pm.variables.get('base'), pm.globals.get('shared'));
        pm.variables.set('shared', 'local');
        console.log(pm.variables.get('shared'), pm.variables.replaceIn('{{base}}/x/{{ shared }}/{{missing}}'));
        pm.environment.unset('secret');
        console.log(String(pm.environment.has('secret')), JSON.stringify(pm.environment.toObject()));
        "#,
    ))
    .await;
    let lines: Vec<&str> = out.logs.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(lines[0], "from-env global https://api.test from-globals");
    assert_eq!(lines[1], "local https://api.test/x/local/{{missing}}");
    assert_eq!(lines[2], r#"false {"shared":"from-env"}"#);
    assert!(out.changes.iter().any(|c| c.unset && c.key == "secret"));
}

#[tokio::test]
async fn send_request_callback_and_await() {
    let host = Arc::new(Host::default());
    let out = run(
        host.clone(),
        input(
            Phase::Pre,
            r#"
            pm.sendRequest('https://auth.test/token', (err, res) => {
                pm.environment.set('cb', res.json().token);
            });
            const res = await pm.sendRequest({
                url: 'https://auth.test/form', method: 'POST',
                header: { 'X-A': '1' },
                body: { mode: 'urlencoded', urlencoded: [{ key: 'a', value: 'b c' }] },
            });
            pm.environment.set('awaited', res.code);
            try { await pm.sendRequest('https://fail.test'); } catch (e) { console.error('failed:', e.message); }
            pm.sendRequest('https://fail.test', (err) => console.warn('cb error:', err.message));
            "#,
        ),
    )
    .await
    .unwrap();
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 4);
    assert_eq!(sent[1].method, "POST");
    assert_eq!(sent[1].body.as_deref(), Some("a=b%20c"));
    assert!(sent[1].headers.iter().any(|(k, _)| k == "Content-Type"));
    let env: Vec<(&str, serde_json::Value)> = out
        .changes
        .iter()
        .map(|c| (c.key.as_str(), c.value.clone()))
        .collect();
    assert!(env.contains(&("cb", json!("t-123"))));
    assert!(env.contains(&("awaited", json!(200))));
    let texts: Vec<&str> = out.logs.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&"failed: connection refused"), "{texts:?}");
    assert!(texts.contains(&"cb error: connection refused"), "{texts:?}");
}

#[tokio::test]
async fn insomnia_scripts_and_libraries() {
    let out = exec(input(
        Phase::Post,
        r#"
        insomnia.environment.set('token', insomnia.response.json().token);
        insomnia.test('ok', () => insomnia.expect(insomnia.response.code).to.equal(200));
        const _l = require('lodash');
        const moment = require('moment');
        const { v4 } = require('uuid');
        console.log(_l.sum([1, 2]), _.chunk([1, 2, 3], 2).length, moment('2026-01-31').add(1, 'day').format('YYYY-MM-DD'));
        console.log(v4().length, btoa('hi'), atob('aGk='), typeof rq.faker('person.firstName'));
        setTimeout(() => console.log('timer'), 10);
        try { require('fs'); } catch (e) { console.log(e.message); }
        "#,
    ))
    .await;
    let texts: Vec<&str> = out.logs.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts[0], "3 2 2026-02-01");
    assert_eq!(texts[1], "36 aGk= hi string");
    assert_eq!(texts[2], "module \"fs\" is not available in Reqly scripts");
    assert_eq!(texts[3], "timer");
    assert_eq!(out.tests[0].name, "ok");
    assert!(out.tests[0].passed);
}

#[tokio::test]
async fn sandbox_has_no_ambient_access() {
    let out = exec(input(
        Phase::Pre,
        "console.log(typeof process, typeof fetch, typeof XMLHttpRequest, typeof std, typeof os, typeof __rq_module === 'function')",
    ))
    .await;
    assert_eq!(
        out.logs[0].text,
        "undefined undefined undefined undefined undefined true"
    );
}

#[tokio::test]
async fn limits_and_errors() {
    let mut i = input(Phase::Pre, "console.log('before'); while (true) {}");
    i.limits.time = Duration::from_millis(200);
    let err = run(Arc::new(Host::default()), i).await.unwrap_err();
    assert_eq!(err.code, "script.timeout");
    assert_eq!(err.params["logs"][0]["text"], "before");

    let mut i = input(
        Phase::Pre,
        "const a = []; while (true) a.push(new Array(100000).fill(1));",
    );
    i.limits.memory = 32 << 20;
    assert_eq!(
        run(Arc::new(Host::default()), i).await.unwrap_err().code,
        "script.memory"
    );

    let err = run(
        Arc::new(Host::default()),
        input(Phase::Pre, "\n\nnotDefined.call();"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "script.error");
    assert!(err.message.contains("notDefined"), "{}", err.message);
    assert_eq!(err.params["script"], "test script");

    let err = run(
        Arc::new(Host::default()),
        input(Phase::Pre, "this is not js"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "script.error");

    // Waiting for the host does not count towards the time limit.
    let mut i = input(
        Phase::Pre,
        "await new Promise((r) => setTimeout(r, 300)); console.log('done')",
    );
    i.limits.time = Duration::from_millis(200);
    let out = run(Arc::new(Host::default()), i).await.unwrap();
    assert_eq!(out.logs[0].text, "done");
}

#[tokio::test]
async fn skip_request_and_next_request() {
    let out = exec(input(
        Phase::Pre,
        "pm.execution.skipRequest(); postman.setNextRequest('Other');",
    ))
    .await;
    assert!(out.skip_request);
    assert_eq!(out.next_request, Some(Some("Other".into())));
    let out = exec(input(Phase::Post, "postman.setNextRequest(null);")).await;
    assert_eq!(out.next_request, Some(None));
    let out = exec(input(Phase::Post, "1")).await;
    assert_eq!(out.next_request, None);
}

#[tokio::test]
async fn chai_loads_quickly_after_the_first_use() {
    let started = std::time::Instant::now();
    exec(input(Phase::Post, "pm.expect(1).to.equal(1)")).await;
    let first = started.elapsed();
    let started = std::time::Instant::now();
    for _ in 0..5 {
        exec(input(Phase::Post, "pm.expect(1).to.equal(1)")).await;
    }
    let each = started.elapsed() / 5;
    eprintln!("first run {first:?}, then {each:?} per run");
    assert!(each < Duration::from_millis(250), "{each:?}");
}

#[tokio::test]
async fn faker_js_api_and_locales() {
    let mut i = input(
        Phase::Pre,
        r#"
        const { faker, fakerDE, fakerEN } = require('@faker-js/faker');
        const { faker: fakerAT } = require('@faker-js/faker/locale/de_AT');
        console.log(/^[А-Яа-яЁё-]+$/.test(faker.person.firstName()), /^[А-Яа-яЁё-]+$/.test(rq.faker.person.lastName()));
        console.log(typeof fakerDE.location.city(), typeof fakerAT.location.city(), typeof fakerEN.finance.iban());
        console.log(faker.helpers.arrayElement(['only']), faker.number.int({ min: 5, max: 5 }), rq.faker('number.int', { min: 7, max: 7 }));
        faker.seed(42);
        const a = faker.string.alpha(8);
        faker.seed(42);
        console.log(a === faker.string.alpha(8), globalThis.faker === faker);
        console.log(pm.variables.replaceIn('{{$randomFirstName}}').length > 0, pm.variables.replaceIn('{{$faker.number.int({ min: 3, max: 3 })}}'));
        console.log(pm.variables.replaceIn('{{$faker.nope.nope}} {{$guid}}').startsWith('{{$faker.nope.nope}} '));
        console.log(faker.image.dataUri({ type: 'svg-base64' }).startsWith('data:image/svg+xml;base64,'));
        "#,
    );
    i.locale = reqly_template::Locale::RU;
    let out = exec(i).await;
    let texts: Vec<&str> = out.logs.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "true true",
            "string string string",
            "only 5 7",
            "true true",
            "true 3",
            "true",
            "true"
        ]
    );
}

#[tokio::test]
async fn faker_package_exports_load_the_whole_package() {
    let out = exec(input(
        Phase::Pre,
        r#"
        const { Faker, allLocales, en, ru } = require('@faker-js/faker');
        const custom = new Faker({ locale: [ru, en] });
        console.log(typeof custom.person.firstName(), Object.keys(allLocales).length > 50);
        "#,
    ))
    .await;
    assert_eq!(out.logs[0].text, "string true");
}

#[tokio::test]
async fn faker_loads_quickly_after_the_first_use() {
    let script = "console.log(faker.person.firstName())";
    let started = std::time::Instant::now();
    exec(input(Phase::Pre, script)).await;
    let first = started.elapsed();
    let started = std::time::Instant::now();
    for _ in 0..5 {
        exec(input(Phase::Pre, script)).await;
    }
    let each = started.elapsed() / 5;
    eprintln!("first run {first:?}, then {each:?} per run");
    assert!(each < Duration::from_millis(250), "{each:?}");
}
