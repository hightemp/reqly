//! Secrets an agent never reads (ADR-014): auth credentials, credential-like
//! header and parameter values, secret environment variables. Values that
//! are only template references (`{{token}}`) reveal nothing and stay.

use reqly_core::CoreResult;
use serde_json::{Map, Value, json};

use crate::agent::ToolContext;

pub const HIDDEN: &str = "••••";

/// Field names whose values are credentials, in any auth type.
const AUTH_SECRETS: &[&str] = &[
    "password",
    "token",
    "value",
    "consumerSecret",
    "tokenSecret",
    "privateKey",
    "secretKey",
    "sessionToken",
    "key",
    "clientSecret",
    "accessToken",
    "clientToken",
    "secret",
    "refreshToken",
];

/// Whether a header, parameter or argument name looks like a credential.
pub fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "password",
        "secret",
        "token",
        "apikey",
        "api_key",
        "api-key",
        "privatekey",
        "passphrase",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|s| name.contains(s))
}

fn hide(value: &Value) -> Value {
    match value.as_str() {
        Some("") => value.clone(),
        Some(s) if only_template(s) => value.clone(),
        _ => Value::String(HIDDEN.into()),
    }
}

fn only_template(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("{{") && t.ends_with("}}") && t[2..t.len() - 2].find("}}").is_none()
}

/// Request or folder data with credentials hidden.
pub fn node_data(data: &Value) -> Value {
    let mut data = data.clone();
    mask_tree(&mut data);
    data
}

fn mask_tree(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(auth) = map.get_mut("auth") {
                mask_auth(auth);
            }
            if is_row(map)
                && map
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(secret_name)
            {
                let hidden = hide(&map["value"]);
                map.insert("value".into(), hidden);
            }
            for (k, v) in map.iter_mut() {
                if k != "auth" {
                    mask_tree(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_tree),
        _ => {}
    }
}

/// A header, parameter or form row: `{ "name": …, "value": … }`.
fn is_row(map: &Map<String, Value>) -> bool {
    map.contains_key("name") && map.get("value").is_some_and(Value::is_string)
}

fn mask_auth(auth: &mut Value) {
    let Value::Object(map) = auth else { return };
    let kind = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    for (k, v) in map.iter_mut() {
        // An API key's `key` is the header name, not the secret.
        let secret = AUTH_SECRETS.contains(&k.as_str()) && !(kind == "apiKey" && k == "key");
        if secret && v.is_string() {
            *v = hide(v);
        }
    }
}

/// Environment variables with the secret ones hidden.
pub fn variables(data: &Value, secrets: &[String]) -> Value {
    let Value::Object(map) = data else {
        return data.clone();
    };
    Value::Object(
        map.iter()
            .map(|(k, v)| {
                let v = if secrets.contains(k) {
                    Value::String(HIDDEN.into())
                } else {
                    v.clone()
                };
                (k.clone(), v)
            })
            .collect(),
    )
}

/// Removes secret values from what a request produced (a rendered request,
/// a response that echoes a header): the secret variables of the
/// collection's environments and literal credentials of the request.
#[derive(Default)]
pub struct Scrub {
    values: Vec<String>,
}

/// Shorter values are too likely to match ordinary text.
const MIN_SECRET_LEN: usize = 4;

impl Scrub {
    pub async fn for_request(cx: &ToolContext<'_>, node: &Value) -> CoreResult<Self> {
        let mut values = Vec::new();
        let envs = cx
            .call(
                "environment.list",
                json!({ "collectionId": node["collectionId"] }),
            )
            .await?;
        for env in envs["collection"]
            .as_array()
            .into_iter()
            .chain(envs["global"].as_array())
            .flatten()
        {
            for name in env["secrets"].as_array().into_iter().flatten() {
                let value = &env["data"][name.as_str().unwrap_or("")];
                match value {
                    Value::String(s) => values.push(s.clone()),
                    Value::Null => {}
                    other => values.push(other.to_string()),
                }
            }
        }
        literal_secrets(&node["data"], &mut values);
        values.retain(|v| v.chars().count() >= MIN_SECRET_LEN && !only_template(v));
        values.sort_by_key(|v| std::cmp::Reverse(v.len()));
        values.dedup();
        Ok(Self { values })
    }

    pub fn apply(&self, value: &mut Value) {
        if self.values.is_empty() {
            return;
        }
        match value {
            Value::String(s) => {
                for secret in &self.values {
                    if s.contains(secret.as_str()) {
                        *s = s.replace(secret.as_str(), HIDDEN);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| self.apply(v)),
            Value::Object(map) => map.values_mut().for_each(|v| self.apply(v)),
            _ => {}
        }
    }
}

/// Credentials typed into the request itself (not variables).
fn literal_secrets(data: &Value, out: &mut Vec<String>) {
    if let Value::Object(auth) = &data["auth"] {
        let kind = auth.get("type").and_then(Value::as_str).unwrap_or("");
        for (k, v) in auth {
            let secret = AUTH_SECRETS.contains(&k.as_str()) && !(kind == "apiKey" && k == "key");
            if let (true, Some(s)) = (secret, v.as_str()) {
                out.push(s.to_string());
            }
        }
    }
    for list in ["headers", "params"] {
        for row in data[list].as_array().into_iter().flatten() {
            if row["name"].as_str().is_some_and(secret_name)
                && let Some(v) = row["value"].as_str()
            {
                // "Bearer abc": the credential part matters.
                out.push(v.rsplit(' ').next().unwrap_or(v).to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hides_auth_credentials_of_every_kind() {
        let data = node_data(&json!({
            "auth": { "type": "apiKey", "key": "X-Key", "value": "v", "location": "header" },
            "headers": [{ "name": "Authorization", "value": "Bearer x" }]
        }));
        assert_eq!(data["auth"]["key"], "X-Key");
        assert_eq!(data["auth"]["value"], HIDDEN);
        assert_eq!(data["headers"][0]["value"], HIDDEN);
        let data =
            node_data(&json!({ "auth": { "type": "basic", "username": "u", "password": "" } }));
        assert_eq!(data["auth"]["username"], "u");
        assert_eq!(data["auth"]["password"], "", "empty stays empty");
        let data = node_data(&json!({ "auth": { "type": "bearer", "token": "{{ token }}" } }));
        assert_eq!(data["auth"]["token"], "{{ token }}");
        let data = node_data(&json!({ "auth": { "type": "bearer", "token": "{{a}}x{{b}}" } }));
        assert_eq!(data["auth"]["token"], HIDDEN);
    }

    #[test]
    fn hides_secret_variables() {
        let out = variables(&json!({ "a": 1, "b": "s" }), &["b".to_string()]);
        assert_eq!(out, json!({ "a": 1, "b": HIDDEN }));
    }
}
