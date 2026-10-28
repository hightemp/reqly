use reqly_core::model::{Assertion, AssertionOperator, AssertionSource};
use reqly_core::{CoreError, Id};
use reqly_script::TestResult;
use reqly_testing::report::{ReportFormat, render_report};
use reqly_testing::run::{RunItem, RunReport};
use reqly_testing::{ResponseFacts, evaluate};

fn report() -> RunReport {
    let facts = ResponseFacts {
        status: Some(200),
        total_ms: Some(30.0),
        headers: &[],
        body: "",
    };
    let ok = evaluate(
        &Assertion::new(
            AssertionSource::Status,
            "",
            AssertionOperator::Equals,
            "200",
        ),
        &facts,
    );
    let bad = evaluate(
        &Assertion::new(
            AssertionSource::Status,
            "",
            AssertionOperator::Equals,
            "201",
        ),
        &facts,
    );
    let item = |iteration, name: &str| RunItem {
        iteration,
        request_id: Id::new(),
        name: name.into(),
        path: vec!["Users".into()],
        method: "GET".into(),
        url: "http://x/users?a=1&b=<2>".into(),
        status: Some(200),
        total_ms: Some(30.0),
        response_id: None,
        error: None,
        tests: vec![
            TestResult {
                name: "body has \"id\" & <name>".into(),
                passed: true,
                skipped: false,
                error: None,
            },
            TestResult {
                name: "later".into(),
                passed: true,
                skipped: true,
                error: None,
            },
        ],
        assertions: vec![ok.clone(), bad.clone()],
    };
    let mut failed = item(1, "Delete");
    failed.status = None;
    failed.tests.clear();
    failed.assertions.clear();
    failed.error = Some(CoreError::new("http.connect", "connection refused"));
    RunReport {
        run_id: "r1".into(),
        name: "Demo <API>".into(),
        environment: Some("dev".into()),
        started_at: 1_700_000_000_000,
        finished_at: 1_700_000_001_500,
        iterations: 2,
        items: vec![item(0, "Get user"), failed],
        cancelled: false,
        stopped: false,
    }
}

#[test]
fn json_has_the_report_and_its_stats() {
    let out = render_report(&report(), ReportFormat::Json);
    assert_eq!(out.file_name, "Demo API-report.json");
    let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
    assert_eq!(v["name"], "Demo <API>");
    assert_eq!(v["passed"], false);
    assert_eq!(v["stats"]["requests"], 2);
    assert_eq!(v["stats"]["failedChecks"], 1);
    assert_eq!(v["items"][0]["assertions"][1]["passed"], false);
}

#[test]
fn junit_has_a_suite_per_request_and_a_case_per_check() {
    let out = render_report(&report(), ReportFormat::Junit);
    assert_eq!(out.file_name, "Demo API-report.xml");
    let package = sxd_document::parser::parse(&out.text).expect("well-formed XML");
    let doc = package.as_document();
    let xpath = |e: &str| sxd_xpath::evaluate_xpath(&doc, e).unwrap().string();
    assert_eq!(xpath("/testsuites/@name"), "Demo <API>");
    assert_eq!(xpath("/testsuites/@tests"), "5");
    assert_eq!(xpath("/testsuites/@failures"), "1");
    assert_eq!(xpath("/testsuites/@errors"), "1");
    assert_eq!(xpath("count(/testsuites/testsuite)"), "2");
    assert_eq!(
        xpath("/testsuites/testsuite[1]/@name"),
        "Users / Get user (iteration 1)"
    );
    assert_eq!(xpath("/testsuites/testsuite[1]/@skipped"), "1");
    assert_eq!(
        xpath("/testsuites/testsuite[1]/testcase[1]/@name"),
        "status equals 200"
    );
    assert_eq!(
        xpath("/testsuites/testsuite[1]/testcase[3]/@name"),
        "body has \"id\" & <name>"
    );
    assert_eq!(
        xpath("count(/testsuites/testsuite[1]/testcase[4]/skipped)"),
        "1"
    );
    assert!(xpath("/testsuites/testsuite[1]/testcase[2]/failure/@message").contains("200"));
    assert_eq!(
        xpath("/testsuites/testsuite[2]/testcase[1]/error/@type"),
        "http.connect"
    );
    assert_eq!(xpath("/testsuites/testsuite[1]/@time"), "0.030");
}

#[test]
fn html_is_self_contained_and_escaped() {
    let out = render_report(&report(), ReportFormat::Html);
    assert_eq!(out.file_name, "Demo API-report.html");
    let html = out.text;
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Demo &lt;API&gt;"));
    assert!(html.contains("body has &quot;id&quot; &amp; &lt;name&gt;"));
    assert!(html.contains("http://x/users?a=1&amp;b=&lt;2&gt;"));
    assert!(html.contains("connection refused"));
    assert!(!html.contains("<script"));
}
