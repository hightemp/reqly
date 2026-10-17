//! GraphQL endpoint: `/graphql` (POST and GET) and subscriptions over
//! `graphql-transport-ws` / `graphql-ws` at `/graphql/ws`.

use std::time::Duration;

use async_graphql::{Context, EmptyMutation, Object, Schema, SimpleObject, Subscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse, GraphQLSubscription};
use axum::Router;
use futures::Stream;

#[derive(SimpleObject, Clone)]
struct User {
    id: i32,
    name: String,
    email: String,
}

fn users() -> Vec<User> {
    vec![
        User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.test".into(),
        },
        User {
            id: 2,
            name: "Bob".into(),
            email: "bob@example.test".into(),
        },
    ]
}

pub struct Query;

#[Object]
impl Query {
    /// Greets `name`.
    async fn hello(&self, name: Option<String>) -> String {
        format!("Hello, {}!", name.unwrap_or_else(|| "world".into()))
    }

    async fn users(&self) -> Vec<User> {
        users()
    }

    async fn user(&self, id: i32) -> Option<User> {
        users().into_iter().find(|u| u.id == id)
    }

    /// Value of a request header (to check headers and auth).
    async fn header(&self, ctx: &Context<'_>, name: String) -> Option<String> {
        ctx.data_opt::<axum::http::HeaderMap>()
            .and_then(|h| h.get(&name))
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }
}

pub struct Subscription;

#[Subscription]
impl Subscription {
    /// Counts from 1 to `to`, one value every 20 ms.
    async fn counter(&self, to: i32) -> impl Stream<Item = i32> {
        futures::stream::unfold(0, move |n| async move {
            if n >= to {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            Some((n + 1, n + 1))
        })
    }
}

pub type TestSchema = Schema<Query, EmptyMutation, Subscription>;

async fn handler(
    axum::extract::State(schema): axum::extract::State<TestSchema>,
    headers: axum::http::HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner().data(headers)).await.into()
}

pub fn router() -> Router {
    let schema: TestSchema = Schema::build(Query, EmptyMutation, Subscription).finish();
    Router::new()
        .route("/graphql", axum::routing::get(handler).post(handler))
        .with_state(schema.clone())
        .route_service("/graphql/ws", GraphQLSubscription::new(schema))
}
