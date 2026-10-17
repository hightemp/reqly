//! Runs the shared transport contract over real HTTP.

use reqly_api::contract::{Client, run_all};
use reqly_core::CoreError;
use reqly_server::{AppState, router};
use serde_json::Value;

struct HttpClient {
    base: String,
    http: reqwest::Client,
}

impl Client for HttpClient {
    async fn call(&self, name: &str, input: Value) -> Result<Value, CoreError> {
        let res = self
            .http
            .post(format!("{}/api/v1/cmd/{name}", self.base))
            .json(&input)
            .send()
            .await
            .expect("request");
        let ok = res.status().is_success();
        let body: Value = res.json().await.expect("json body");
        if ok {
            Ok(body)
        } else {
            Err(serde_json::from_value(body).expect("CoreError body"))
        }
    }
}

#[tokio::test]
async fn http_transport_satisfies_contract() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router(AppState::for_tests().await)).into_future());

    let client = HttpClient {
        base: format!("http://{addr}"),
        http: reqwest::Client::builder().no_proxy().build().unwrap(),
    };
    run_all(&client, "server").await;
}
