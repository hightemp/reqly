use reqly_core::Id;
use reqly_core::model::{Assertion, AssertionOperator, AssertionSource};
use reqly_script::TestResult;
use reqly_testing::run::{RunItem, RunReport};
use reqly_testing::{AssertionResult, ResponseFacts, evaluate};

fn item(status: Option<u16>) -> RunItem {
    RunItem {
        iteration: 0,
        request_id: Id::new(),
        name: "Get user".into(),
        path: vec!["Users".into()],
        method: "GET".into(),
        url: "http://x/users/1".into(),
        status,
        total_ms: Some(12.0),
        response_id: None,
        error: None,
        tests: Vec::new(),
        assertions: Vec::new(),
    }
}

fn assertion(passed: bool) -> AssertionResult {
    let a = Assertion::new(
        AssertionSource::Status,
        "",
        AssertionOperator::Equals,
        "200",
    );
    let facts = ResponseFacts {
        status: Some(if passed { 200 } else { 500 }),
        total_ms: None,
        headers: &[],
        body: "",
    };
    evaluate(&a, &facts)
}

fn test(name: &str, passed: bool, skipped: bool) -> TestResult {
    TestResult {
        name: name.into(),
        passed,
        skipped,
        error: (!passed).then(|| "boom".into()),
    }
}

#[test]
fn an_item_passes_without_errors_or_failed_checks() {
    let mut ok = item(Some(200));
    ok.tests = vec![test("a", true, false), test("b", true, true)];
    ok.assertions = vec![assertion(true)];
    assert!(ok.passed());

    let mut failed_test = item(Some(200));
    failed_test.tests = vec![test("a", false, false)];
    assert!(!failed_test.passed());

    let mut failed_assertion = item(Some(200));
    failed_assertion.assertions = vec![assertion(false)];
    assert!(!failed_assertion.passed());

    let mut error = item(None);
    error.error = Some(reqly_core::CoreError::new("http.connect", "refused"));
    assert!(!error.passed());

    // A 500 without checks is not a failure: only checks decide.
    assert!(item(Some(500)).passed());
}

#[test]
fn stats_count_requests_and_checks() {
    let mut a = item(Some(200));
    a.tests = vec![test("a", true, false), test("s", true, true)];
    a.assertions = vec![assertion(true), assertion(false)];
    let mut b = item(None);
    b.error = Some(reqly_core::CoreError::new("http.connect", "refused"));
    let c = item(Some(204));
    let report = RunReport {
        run_id: "r".into(),
        name: "Users".into(),
        environment: None,
        started_at: 1_000,
        finished_at: 1_250,
        iterations: 1,
        items: vec![a, b, c],
        cancelled: false,
        stopped: false,
    };
    let s = report.stats();
    assert_eq!(s.requests, 3);
    assert_eq!(s.failed_requests, 2);
    assert_eq!(s.errors, 1);
    assert_eq!(s.checks, 3);
    assert_eq!(s.failed_checks, 1);
    assert_eq!(s.skipped_checks, 1);
    assert_eq!(s.duration_ms, 250);
    assert!(!report.passed());

    let fine = RunReport {
        items: vec![item(Some(200))],
        ..report.clone()
    };
    assert!(fine.passed());
    let cancelled = RunReport {
        cancelled: true,
        ..fine
    };
    assert!(!cancelled.passed());
}
