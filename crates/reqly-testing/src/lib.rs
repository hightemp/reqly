//! No-code assertions on a response (PRD 7.16) and, later, the collection
//! runner.

pub mod data;
pub mod report;
pub mod run;

use reqly_core::CoreError;
use reqly_core::id::Id;
use reqly_core::model::{
    Assertion, AssertionOperator, AssertionOperator as Op, AssertionSource, AssertionSource as Src,
};
use reqly_http::filter::{FilterKind, query_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// The parts of a response assertions look at.
#[derive(Debug, Clone, Copy)]
pub struct ResponseFacts<'a> {
    pub status: Option<u16>,
    pub total_ms: Option<f64>,
    pub headers: &'a [(String, String)],
    pub body: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AssertionResult {
    /// The assertion's id.
    pub id: Id,
    /// The check as evaluated (templates rendered), so a stored result
    /// describes itself after the assertion changes.
    pub source: AssertionSource,
    pub property: String,
    pub operator: AssertionOperator,
    pub expected: String,
    pub passed: bool,
    /// The checked value as text; absent when the response has none.
    #[ts(optional)]
    pub actual: Option<String>,
    /// Why the check failed when the value alone does not say (schema errors).
    #[ts(optional)]
    pub detail: Option<String>,
    /// The assertion could not be evaluated (bad expression, regex, schema).
    #[ts(optional)]
    pub error: Option<CoreError>,
}

impl AssertionResult {
    /// A result for `assertion` that has not been evaluated yet (failed).
    pub fn new(assertion: &Assertion) -> Self {
        Self {
            id: assertion.id,
            source: assertion.source,
            property: assertion.property.clone(),
            operator: assertion.operator,
            expected: assertion.value.clone(),
            passed: false,
            actual: None,
            detail: None,
            error: None,
        }
    }

    /// The check in English, for reports: `header Content-Type contains json`.
    pub fn describe(&self) -> String {
        let source = match self.source {
            Src::Status => "status",
            Src::ResponseTime => "response time",
            Src::Header => "header",
            Src::Body => "body",
            Src::JsonPath => "JSONPath",
            Src::Jq => "jq",
        };
        let operator = match self.operator {
            Op::Equals => "equals",
            Op::NotEquals => "does not equal",
            Op::Contains => "contains",
            Op::NotContains => "does not contain",
            Op::Matches => "matches",
            Op::LessThan => "less than",
            Op::LessThanOrEqual => "at most",
            Op::GreaterThan => "greater than",
            Op::GreaterThanOrEqual => "at least",
            Op::Exists => "exists",
            Op::NotExists => "does not exist",
            Op::MatchesSchema => "matches schema",
        };
        let mut parts = vec![source];
        if matches!(self.source, Src::Header | Src::JsonPath | Src::Jq) {
            parts.push(self.property.trim());
        }
        parts.push(operator);
        if !self.operator.is_unary() && self.operator != Op::MatchesSchema {
            parts.push(&self.expected);
        }
        parts.join(" ")
    }
}

/// Longest `actual` kept in a result.
const ACTUAL_LIMIT: usize = 1000;

/// Evaluates the enabled assertions, in order.
pub fn evaluate_all(assertions: &[Assertion], facts: &ResponseFacts) -> Vec<AssertionResult> {
    assertions
        .iter()
        .filter(|a| a.enabled)
        .map(|a| evaluate(a, facts))
        .collect()
}

pub fn evaluate(assertion: &Assertion, facts: &ResponseFacts) -> AssertionResult {
    let mut result = AssertionResult::new(assertion);
    let actual = match actual_value(assertion, facts) {
        Ok(v) => v,
        Err(e) => {
            result.error = Some(e);
            return result;
        }
    };
    result.actual = actual.as_ref().map(|v| truncate(text(v)));
    match check(assertion.operator, actual.as_ref(), &assertion.value) {
        Ok(Outcome::Pass) => result.passed = true,
        Ok(Outcome::Fail(detail)) => result.detail = detail,
        Err(e) => result.error = Some(e),
    }
    result
}

fn actual_value(assertion: &Assertion, facts: &ResponseFacts) -> Result<Option<Value>, CoreError> {
    Ok(match assertion.source {
        Src::Status => facts.status.map(Value::from),
        Src::ResponseTime => facts.total_ms.map(Value::from),
        Src::Header => {
            let values: Vec<&str> = facts
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case(assertion.property.trim()))
                .map(|(_, value)| value.as_str())
                .collect();
            (!values.is_empty()).then(|| Value::String(values.join(", ")))
        }
        Src::Body => {
            if assertion.operator == Op::MatchesSchema {
                Some(serde_json::from_str(facts.body).map_err(|e| {
                    CoreError::new("filter.input", e.to_string())
                        .with_param("detail", e.to_string())
                })?)
            } else {
                Some(Value::String(facts.body.to_string()))
            }
        }
        Src::JsonPath | Src::Jq => {
            let kind = if assertion.source == Src::Jq {
                FilterKind::Jq
            } else {
                FilterKind::JsonPath
            };
            let mut values = query_json(kind, facts.body, assertion.property.trim())?;
            match values.len() {
                0 => None,
                1 => values.pop(),
                _ => Some(Value::Array(values)),
            }
        }
    })
}

enum Outcome {
    Pass,
    Fail(Option<String>),
}

fn outcome(passed: bool) -> Outcome {
    if passed {
        Outcome::Pass
    } else {
        Outcome::Fail(None)
    }
}

fn check(op: Op, actual: Option<&Value>, expected: &str) -> Result<Outcome, CoreError> {
    let Some(actual) = actual else {
        return Ok(outcome(op == Op::NotExists));
    };
    Ok(match op {
        Op::Exists => Outcome::Pass,
        Op::NotExists => outcome(false),
        Op::Equals => outcome(equals(actual, expected)),
        Op::NotEquals => outcome(!equals(actual, expected)),
        Op::Contains => outcome(contains(actual, expected)),
        Op::NotContains => outcome(!contains(actual, expected)),
        Op::Matches => {
            let re = regex::Regex::new(expected).map_err(|e| {
                CoreError::new("assertion.regex", e.to_string()).with_param("detail", e.to_string())
            })?;
            outcome(re.is_match(&text(actual)))
        }
        Op::LessThan | Op::LessThanOrEqual | Op::GreaterThan | Op::GreaterThanOrEqual => {
            let a = number(actual).ok_or_else(|| not_number(&text(actual)))?;
            let e = expected
                .trim()
                .parse::<f64>()
                .map_err(|_| not_number(expected))?;
            outcome(match op {
                Op::LessThan => a < e,
                Op::LessThanOrEqual => a <= e,
                Op::GreaterThan => a > e,
                _ => a >= e,
            })
        }
        Op::MatchesSchema => schema(actual, expected)?,
    })
}

fn not_number(value: &str) -> CoreError {
    CoreError::new("assertion.notNumber", format!("{value} is not a number"))
        .with_param("value", truncate(value.to_string()))
}

/// Strings print bare, everything else as compact JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn truncate(mut s: String) -> String {
    if s.len() > ACTUAL_LIMIT {
        let mut end = ACTUAL_LIMIT;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push('…');
    }
    s
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The expected value as JSON when it parses, otherwise as a string.
fn parse_expected(expected: &str) -> Value {
    serde_json::from_str(expected).unwrap_or_else(|_| Value::String(expected.to_string()))
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn equals(actual: &Value, expected: &str) -> bool {
    same(actual, &parse_expected(expected)) || text(actual) == expected
}

fn contains(actual: &Value, expected: &str) -> bool {
    match actual {
        Value::Array(items) => {
            let wanted = parse_expected(expected);
            items
                .iter()
                .any(|item| same(item, &wanted) || text(item) == expected)
        }
        Value::Object(map) => map.contains_key(expected),
        other => text(other).contains(expected),
    }
}

fn schema(actual: &Value, expected: &str) -> Result<Outcome, CoreError> {
    let invalid = |e: String| CoreError::new("assertion.schema", e.clone()).with_param("detail", e);
    let schema: Value = serde_json::from_str(expected).map_err(|e| invalid(e.to_string()))?;
    let validator = jsonschema::validator_for(&schema).map_err(|e| invalid(e.to_string()))?;
    let errors: Vec<String> = validator
        .iter_errors(actual)
        .take(10)
        .map(|e| {
            let path = e.instance_path().to_string();
            if path.is_empty() {
                e.to_string()
            } else {
                format!("{path}: {e}")
            }
        })
        .collect();
    Ok(if errors.is_empty() {
        Outcome::Pass
    } else {
        Outcome::Fail(Some(errors.join("\n")))
    })
}
