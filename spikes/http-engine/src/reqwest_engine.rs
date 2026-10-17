use std::time::Instant;

use crate::{Req, Res, Timings};

pub async fn send(req: &Req) -> Result<Res, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(if req.follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        });
    if let Some(ca) = &req.ca_pem {
        let cert = reqwest::Certificate::from_pem(ca.as_bytes()).map_err(|e| e.to_string())?;
        builder = builder.tls_certs_only([cert]);
    }
    if let (Some(cert), Some(key)) = (&req.client_cert_pem, &req.client_key_pem) {
        let identity = reqwest::Identity::from_pem(format!("{cert}{key}").as_bytes())
            .map_err(|e| e.to_string())?;
        builder = builder.identity(identity);
    }
    if !req.http2 {
        builder = builder.http1_only();
    }
    if let Some(t) = req.timeout {
        builder = builder.timeout(t);
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    let method = reqwest::Method::from_bytes(req.method.as_bytes()).map_err(|e| e.to_string())?;
    let mut rb = client.request(method, &req.url);
    for (k, v) in &req.headers {
        rb = rb.header(k, v);
    }
    if !req.body.is_empty() {
        rb = rb.body(req.body.clone());
    }

    let started = Instant::now();
    let res = rb.send().await.map_err(|e| e.to_string())?;
    let ttfb = started.elapsed();
    let status = res.status().as_u16();
    let http_version = format!("{:?}", res.version());
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = res.bytes().await.map_err(|e| e.to_string())?.to_vec();

    Ok(Res {
        status,
        http_version,
        headers,
        body,
        // No DNS/connect/TLS breakdown and no raw timeline without a custom connector.
        timings: Timings {
            ttfb: Some(ttfb),
            total: started.elapsed(),
            ..Default::default()
        },
        timeline: Vec::new(),
        redirects: 0,
    })
}
