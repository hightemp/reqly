use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com".into());
    let req = spike_http_engine::Req {
        method: "GET".into(),
        url,
        ..Default::default()
    };
    match spike_http_engine::curl_engine::send(&req, Arc::new(AtomicBool::new(false))) {
        Ok(res) => println!("{} {} bytes {:?}", res.status, res.body.len(), res.timings),
        Err(e) => eprintln!("error: {e}"),
    }
}
