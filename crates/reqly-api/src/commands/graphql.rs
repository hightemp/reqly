//! GraphQL schemas: introspection through the request's own endpoint,
//! headers and auth, or SDL from a file; cached per request.

use reqly_core::{CoreError, CoreResult, Id};
use reqly_http::engine::CancelToken;
use reqly_http::prepare::PreparedBody;
use reqly_storage::graphql::{GraphqlRepo, GraphqlSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<GraphqlIntrospect>()
        .register::<GraphqlSchemaGet>()
        .register::<GraphqlSchemaSet>()
        .register::<GraphqlSchemaClear>();
}

/// The introspection query of graphql-js (`getIntrospectionQuery`), with
/// descriptions and deprecated fields.
pub const INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives { name description locations args { ...InputValue } }
  }
}
fragment FullType on __Type {
  kind name description
  fields(includeDeprecated: true) {
    name description args { ...InputValue } type { ...TypeRef } isDeprecated deprecationReason
  }
  inputFields { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason }
  possibleTypes { ...TypeRef }
}
fragment InputValue on __InputValue { name description type { ...TypeRef } defaultValue }
fragment TypeRef on __Type {
  kind name
  ofType { kind name ofType { kind name ofType { kind name ofType { kind name
    ofType { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } } } }
}"#;

macro_rules! command {
    ($ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphqlRequestInput {
    pub request_id: Id,
}

fn repo(ctx: &AppContext) -> GraphqlRepo<'_> {
    GraphqlRepo::new(&ctx.storage, ctx.clock.as_ref())
}

command!(
    GraphqlIntrospect,
    "graphql.introspect",
    GraphqlRequestInput,
    GraphqlSchema,
    |ctx, input| {
        let (mut prepared, workspace_id) =
            super::http::prepare_stored(ctx, input.request_id, &Default::default(), &[]).await?;
        // Same endpoint, headers and auth; the introspection query as the body.
        prepared.method = "POST".into();
        if let Ok(mut url) = url::Url::parse(&prepared.url) {
            // Drop GraphQL GET parameters.
            let keep: Vec<(String, String)> = url
                .query_pairs()
                .filter(|(k, _)| !matches!(k.as_ref(), "query" | "variables" | "operationName"))
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            url.set_query(None);
            if !keep.is_empty() {
                url.query_pairs_mut().extend_pairs(keep);
            }
            prepared.url = url.to_string();
        }
        prepared
            .headers
            .retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
        prepared
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        prepared.body = PreparedBody::Bytes(
        serde_json::json!({ "query": INTROSPECTION_QUERY, "operationName": "IntrospectionQuery" })
            .to_string()
            .into_bytes(),
    );
        let source = prepared.url.clone();
        let options = super::http::exec_options(ctx, workspace_id, &prepared.url).await?;
        let response = super::http::execute(ctx, prepared, options, CancelToken::new()).await?;
        let bytes = response
            .body
            .to_vec()
            .map_err(|e| CoreError::new("http.io", e.to_string()))?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|_| {
            CoreError::new(
                "graphql.introspection_failed",
                "the endpoint did not answer JSON",
            )
            .with_param("status", u64::from(response.status))
            .with_param("message", format!("HTTP {}", response.status))
        })?;
        let Some(schema) = json.get("data").filter(|d| d.get("__schema").is_some()) else {
            let message = json["errors"][0]["message"]
                .as_str()
                .unwrap_or("no __schema in the answer")
                .to_string();
            return Err(
                CoreError::new("graphql.introspection_failed", message.clone())
                    .with_param("status", u64::from(response.status))
                    .with_param("message", message),
            );
        };
        repo(ctx)
            .put(
                input.request_id,
                "introspection",
                &source,
                &schema.to_string(),
            )
            .await
    }
);

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphqlSchemaOutput {
    #[ts(optional)]
    pub schema: Option<GraphqlSchema>,
}

command!(
    GraphqlSchemaGet,
    "graphql.schemaGet",
    GraphqlRequestInput,
    GraphqlSchemaOutput,
    |ctx, input| {
        Ok(GraphqlSchemaOutput {
            schema: repo(ctx).get(input.request_id).await?,
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphqlSchemaSetInput {
    pub request_id: Id,
    /// `sdl` or `introspection` (JSON).
    pub format: String,
    /// File name shown as the source.
    pub source: String,
    pub schema: String,
}

command!(
    GraphqlSchemaSet,
    "graphql.schemaSet",
    GraphqlSchemaSetInput,
    GraphqlSchema,
    |ctx, input| {
        let format = match input.format.as_str() {
            "sdl" => "sdl",
            "introspection" => {
                // Accept `{ data: { __schema } }`, `{ __schema }`.
                let v: Value = serde_json::from_str(&input.schema).map_err(|e| {
                    CoreError::new("graphql.invalid_schema", e.to_string())
                        .with_param("message", e.to_string())
                })?;
                let inner = v.get("data").unwrap_or(&v);
                if inner.get("__schema").is_none() {
                    return Err(CoreError::new(
                        "graphql.invalid_schema",
                        "no __schema in the file",
                    )
                    .with_param("message", "no __schema in the file"));
                }
                return repo(ctx)
                    .put(
                        input.request_id,
                        "introspection",
                        &input.source,
                        &inner.to_string(),
                    )
                    .await;
            }
            other => {
                return Err(CoreError::new(
                    "graphql.invalid_schema",
                    format!("unknown format {other}"),
                )
                .with_param("message", format!("unknown format {other}")));
            }
        };
        repo(ctx)
            .put(input.request_id, format, &input.source, &input.schema)
            .await
    }
);

command!(
    GraphqlSchemaClear,
    "graphql.schemaClear",
    GraphqlRequestInput,
    Empty,
    |ctx, input| {
        repo(ctx).delete(input.request_id).await?;
        Ok(Empty {})
    }
);
