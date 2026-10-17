#[tokio::main]
async fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com".into());
    let req = spike_http_engine::Req {
        method: "GET".into(),
        url,
        ..Default::default()
    };
    match spike_http_engine::reqwest_engine::send(&req).await {
        Ok(res) => println!("{} {} bytes {:?}", res.status, res.body.len(), res.timings),
        Err(e) => eprintln!("error: {e}"),
    }
}
