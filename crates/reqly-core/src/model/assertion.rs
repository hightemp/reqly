use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::id::Id;

/// What part of the response an assertion checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AssertionSource {
    #[default]
    Status,
    /// Total time in milliseconds.
    ResponseTime,
    /// `property` is the header name (case-insensitive).
    Header,
    /// The body as text.
    Body,
    /// `property` is a JSONPath expression over the JSON body.
    JsonPath,
    /// `property` is a jq program over the JSON body.
    Jq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AssertionOperator {
    #[default]
    Equals,
    NotEquals,
    Contains,
    NotContains,
    /// `value` is a regular expression.
    Matches,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Exists,
    NotExists,
    /// `value` is a JSON Schema.
    MatchesSchema,
}

impl AssertionOperator {
    /// Operators that ignore `value`.
    pub fn is_unary(self) -> bool {
        matches!(self, Self::Exists | Self::NotExists)
    }
}

/// A no-code test on the response, evaluated after the after-response script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Assertion {
    pub id: Id,
    pub enabled: bool,
    pub source: AssertionSource,
    /// Header name, JSONPath or jq expression; unused for the other sources.
    pub property: String,
    pub operator: AssertionOperator,
    /// The expected value (a template); a JSON Schema for `matchesSchema`.
    pub value: String,
}

impl Default for Assertion {
    fn default() -> Self {
        Self {
            id: Id::new(),
            enabled: true,
            source: AssertionSource::Status,
            property: String::new(),
            operator: AssertionOperator::Equals,
            value: String::new(),
        }
    }
}

impl Assertion {
    pub fn new(
        source: AssertionSource,
        property: impl Into<String>,
        operator: AssertionOperator,
        value: impl Into<String>,
    ) -> Self {
        Self {
            source,
            property: property.into(),
            operator,
            value: value.into(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case() {
        let a = Assertion::new(
            AssertionSource::JsonPath,
            "$.id",
            AssertionOperator::GreaterThanOrEqual,
            "1",
        );
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["source"], "jsonPath");
        assert_eq!(json["operator"], "greaterThanOrEqual");
        assert_eq!(json["property"], "$.id");
        assert_eq!(json["enabled"], true);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let a: Assertion = serde_json::from_str(r#"{"operator":"exists"}"#).unwrap();
        assert_eq!(a.source, AssertionSource::Status);
        assert!(a.enabled);
        assert!(a.operator.is_unary());
    }
}
