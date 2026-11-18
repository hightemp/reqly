//! A core in memory with the real tools behind the HTTP endpoint.

#![allow(dead_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use reqly_api::access::Caller;
use reqly_api::commands::{AppContext, dispatch_as, registry};
use reqly_mcp::{
    Allowed, BoxFuture, Denied, Gate, McpState, OriginPolicy, Session, catalog_tools, router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Open(Caller);

impl Gate for Open {
    fn open<'a>(&'a self, _token: Option<&'a str>) -> BoxFuture<'a, Result<Session, Denied>> {
        Box::pin(async move {
            Ok(Session {
                caller: self.0,
                allowed: Allowed::All,
            })
        })
    }
}

pub struct Fx {
    pub ctx: Arc<AppContext>,
    pub app: axum::Router,
}

impl Fx {
    pub async fn new() -> Self {
        Self::with(Arc::new(AppContext::for_tests().await), Caller::Local)
    }

    pub fn with(ctx: Arc<AppContext>, caller: Caller) -> Self {
        let app = router(McpState {
            ctx: ctx.clone(),
            registry: Arc::new(registry()),
            tools: Arc::new(catalog_tools()),
            gate: Arc::new(Open(caller)),
            origin: OriginPolicy::Loopback,
        });
        Self { ctx, app }
    }

    /// A registry command, as the UI would run it.
    pub async fn cmd(&self, name: &str, input: Value) -> Value {
        dispatch_as(&registry(), &self.ctx, Caller::Local, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// The raw `tools/call` result.
    pub async fn call(&self, tool: &str, args: Value) -> Value {
        let msg = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        });
        let res = self
            .app
            .clone()
            .oneshot(
                Request::post("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from(msg.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(body["error"].is_null(), "{tool}: {body}");
        body["result"].clone()
    }

    /// The structured result of a successful tool call.
    pub async fn tool(&self, tool: &str, args: Value) -> Value {
        let result = self.call(tool, args).await;
        assert_eq!(result["isError"], false, "{tool}: {result}");
        result["structuredContent"].clone()
    }

    /// The error text of a failed tool call.
    pub async fn tool_err(&self, tool: &str, args: Value) -> String {
        let result = self.call(tool, args).await;
        assert_eq!(result["isError"], true, "{tool}: {result}");
        result["content"][0]["text"].as_str().unwrap().to_string()
    }

    pub async fn workspace(&self) -> Value {
        self.cmd("workspace.ensureDefault", json!({})).await[0].clone()
    }

    pub async fn collection(&self, name: &str) -> Value {
        let ws = self.workspace().await;
        self.cmd(
            "collection.create",
            json!({ "workspaceId": ws["id"], "name": name }),
        )
        .await
    }

    pub async fn node(
        &self,
        col: &Value,
        parent: Option<&Value>,
        name: &str,
        data: Value,
    ) -> Value {
        self.cmd(
            "node.create",
            json!({
                "collectionId": col["id"],
                "parentId": parent.map(|p| p["id"].clone()),
                "name": name,
                "data": data,
                "placement": { "type": "end" }
            }),
        )
        .await
    }
}
