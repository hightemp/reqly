//! Signing schemes against published test vectors.

use std::path::Path;

use reqly_core::model::{Auth, HawkAlgorithm, HttpRequest, KeyValue, OAuth1Location, OAuth1Method};
use reqly_http::auth::{SignContext, edgegrid, finalize, hawk, oauth1, sigv4};
use reqly_http::prepare::prepare;
use serde_json::Value;

const VECTORS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors");

fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(Path::new(VECTORS).join(path)).unwrap()
}

// ----- AWS SigV4 ----------------------------------------------------------------------

/// Parses a raw HTTP request from the test suite.
fn parse_raw(text: &str) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut lines = head.lines();
    let first = lines.next().unwrap();
    let mut parts = first.splitn(3, ' ');
    let method = parts.next().unwrap().to_string();
    let target = first[method.len() + 1..first.rfind(" HTTP/").unwrap()].to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation of a folded header.
            let last = headers.last_mut().unwrap();
            last.1.push(' ');
            last.1.push_str(line.trim());
            continue;
        }
        let (k, v) = line.split_once(':').unwrap();
        headers.push((k.to_string(), v.to_string()));
    }
    (method, target, headers, body.as_bytes().to_vec())
}

#[test]
fn aws_sigv4_test_suite() {
    let mut checked = 0;
    for entry in std::fs::read_dir(Path::new(VECTORS).join("aws-sigv4")).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let context: Value = serde_json::from_str(&read(dir.join("context.json"))).unwrap();
        let (method, target, headers, body) = parse_raw(&read(dir.join("request.txt")));
        let host = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("host"))
            .unwrap()
            .1
            .clone();
        // Keep the target as the suite writes it (raw characters are percent-encoded by Url).
        let url = format!("https://{host}{}", target.replace(' ', "%20"));
        let creds = &context["credentials"];
        let timestamp = context["timestamp"].as_str().unwrap();
        let now = chrono_like(timestamp);
        let signed = sigv4::sign(
            &method,
            &url,
            &headers
                .iter()
                .filter(|(k, _)| {
                    !k.eq_ignore_ascii_case("host") && !k.eq_ignore_ascii_case("x-amz-date")
                })
                .cloned()
                .collect::<Vec<_>>(),
            Some(&body),
            &sigv4::Credentials {
                access_key: creds["access_key_id"].as_str().unwrap(),
                secret_key: creds["secret_access_key"].as_str().unwrap(),
                session_token: creds["token"].as_str().unwrap_or(""),
                region: context["region"].as_str().unwrap(),
                service: context["service"].as_str().unwrap(),
            },
            now,
            context["sign_body"].as_bool().unwrap_or(false),
        )
        .unwrap();
        let expected = read(dir.join("header-signed-request.txt"));
        let expected_auth = expected
            .lines()
            .find_map(|l| l.strip_prefix("Authorization:"))
            .unwrap();
        let auth = &signed.iter().find(|(k, _)| k == "Authorization").unwrap().1;
        assert_eq!(auth, expected_auth, "{name}");
        checked += 1;
    }
    assert!(checked >= 15, "{checked}");
}

/// `2015-08-30T12:36:00Z` → Unix seconds.
fn chrono_like(ts: &str) -> i64 {
    let (date, time) = ts.trim_end_matches('Z').split_once('T').unwrap();
    let d: Vec<i64> = date.split('-').map(|p| p.parse().unwrap()).collect();
    let t: Vec<i64> = time.split(':').map(|p| p.parse().unwrap()).collect();
    let (y, m) = if d[1] <= 2 {
        (d[0] - 1, d[1] + 9)
    } else {
        (d[0], d[1] - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d[2] - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    days * 86_400 + t[0] * 3600 + t[1] * 60 + t[2]
}

// ----- EdgeGrid -------------------------------------------------------------------------

#[test]
fn edgegrid_test_cases() {
    let data: Value = serde_json::from_str(&read("edgegrid/testdata.json")).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&read("edgegrid/testcases.json")).unwrap();
    assert_eq!(
        edgegrid::signing_key(
            data["client_secret"].as_str().unwrap(),
            data["timestamp"].as_str().unwrap()
        ),
        data["sign_key_test"]
    );
    assert_eq!(
        edgegrid::content_hash("POST", b"test_body", 2048),
        data["content_hash_test"]
    );
    let base = data["base_url"].as_str().unwrap().trim_end_matches('/');
    let mut checked = 0;
    for case in cases {
        let Some(expected) = case["expectedAuthorization"].as_str() else {
            continue;
        };
        let req = &case["request"];
        let headers: Vec<(String, String)> = req["headers"]
            .as_array()
            .map(|hs| {
                hs.iter()
                    .flat_map(|h| {
                        h.as_object()
                            .unwrap()
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let params = edgegrid::Params {
            client_token: data["client_token"].as_str().unwrap(),
            access_token: data["access_token"].as_str().unwrap(),
            client_secret: data["client_secret"].as_str().unwrap(),
            headers_to_sign: data["headers_to_sign"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().to_string())
                .collect(),
            max_body: data["max_body"].as_u64().unwrap() as usize,
            timestamp: data["timestamp"].as_str().unwrap(),
            nonce: data["nonce"].as_str().unwrap(),
        };
        let url = format!("{base}{}", req["path"].as_str().unwrap());
        let body = req["data"].as_str().unwrap_or("").as_bytes().to_vec();
        let method = req["method"].as_str().unwrap();
        let header = edgegrid::header(&params, method, &url, &headers, &body).unwrap();
        assert_eq!(header, expected, "{}", case["testName"]);
        checked += 1;
    }
    assert!(checked >= 10, "{checked}");
}

// ----- Hawk ----------------------------------------------------------------------------------

#[test]
fn hawk_readme_vectors() {
    let params = hawk::Params {
        id: "dh37fgj492je",
        key: "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn",
        algorithm: HawkAlgorithm::Sha256,
        ts: 1_353_832_234,
        nonce: "j4h3g2",
        ext: "some-app-ext-data",
        app: "",
        dlg: "",
    };
    let url = "http://example.com:8000/resource/1?b=1&a=2";
    assert_eq!(
        hawk::header(&params, "GET", url, None).unwrap(),
        "Hawk id=\"dh37fgj492je\", ts=\"1353832234\", nonce=\"j4h3g2\", ext=\"some-app-ext-data\", mac=\"6R4rV5iE+NPoym+WwjeHzjAGXUtLNIxmo1vpMofpLAE=\""
    );
    let hash = hawk::payload_hash(
        HawkAlgorithm::Sha256,
        "text/plain",
        b"Thank you for flying Hawk",
    );
    assert_eq!(hash, "Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY=");
    assert_eq!(
        hawk::header(&params, "POST", url, Some(&hash)).unwrap(),
        "Hawk id=\"dh37fgj492je\", ts=\"1353832234\", nonce=\"j4h3g2\", hash=\"Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY=\", ext=\"some-app-ext-data\", mac=\"aSe1DERmZuRl3pI36/9BdZmnErTw3sNzOOAUlfeKjVw=\""
    );
}

// ----- OAuth 1.0 ------------------------------------------------------------------------------

fn oauth1_params<'a>(method: OAuth1Method) -> oauth1::Params<'a> {
    oauth1::Params {
        consumer_key: "dpf43f3p2l4k3l03",
        consumer_secret: "kd94hf93k423kf44",
        token: "nnch734d00sl2jdk",
        token_secret: "pfkkdhi9sl3r4s00",
        method,
        private_key: "",
        callback: "",
        verifier: "",
        nonce: "kllo9940pd9333jh",
        timestamp: "1191242096",
        version: true,
        body_hash: None,
    }
}

#[test]
fn oauth1_core_appendix_example() {
    // OAuth Core 1.0, Appendix A.5.
    let p = oauth1_params(OAuth1Method::HmacSha1);
    let protocol = oauth1::protocol_params(&p);
    let base = oauth1::base_string(
        "GET",
        "http://photos.example.net/photos?file=vacation.jpg&size=original",
        &protocol,
        &[],
    )
    .unwrap();
    assert_eq!(
        base,
        "GET&http%3A%2F%2Fphotos.example.net%2Fphotos&file%3Dvacation.jpg%26oauth_consumer_key%3Ddpf43f3p2l4k3l03%26oauth_nonce%3Dkllo9940pd9333jh%26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D1191242096%26oauth_token%3Dnnch734d00sl2jdk%26oauth_version%3D1.0%26size%3Doriginal"
    );
    assert_eq!(
        oauth1::signature(&p, &base).unwrap(),
        "tR3+Ty81lMeYAr/Fid0kMTYa/WM="
    );
    let plain = oauth1::signature(&oauth1_params(OAuth1Method::Plaintext), &base).unwrap();
    assert_eq!(plain, "kd94hf93k423kf44&pfkkdhi9sl3r4s00");
}

#[test]
fn oauth1_rfc5849_base_string() {
    // RFC 5849 §3.4.1.1 (query + form body + protocol parameters).
    let p = oauth1::Params {
        consumer_key: "9djdj82h48djs9d2",
        consumer_secret: "",
        token: "kkk9d7dh3k39sjv7",
        token_secret: "",
        method: OAuth1Method::HmacSha1,
        private_key: "",
        callback: "",
        verifier: "",
        nonce: "7d8f3e4a",
        timestamp: "137131201",
        version: false,
        body_hash: None,
    };
    let form = vec![
        ("c2".to_string(), String::new()),
        ("a3".to_string(), "2 q".to_string()),
    ];
    let base = oauth1::base_string(
        "POST",
        "http://example.com/request?b5=%3D%253D&a3=a&c%40=&a2=r%20b",
        &oauth1::protocol_params(&p),
        &form,
    )
    .unwrap();
    assert_eq!(
        base,
        "POST&http%3A%2F%2Fexample.com%2Frequest&a2%3Dr%2520b%26a3%3D2%2520q%26a3%3Da%26b5%3D%253D%25253D%26c%2540%3D%26c2%3D%26oauth_consumer_key%3D9djdj82h48djs9d2%26oauth_nonce%3D7d8f3e4a%26oauth_signature_method%3DHMAC-SHA1%26oauth_timestamp%3D137131201%26oauth_token%3Dkkk9d7dh3k39sjv7"
    );
}

#[test]
fn oauth1_rsa_signatures_verify() {
    use base64::Engine;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;
    let key = read("keys/rsa.pem");
    let public = rsa::RsaPublicKey::from_public_key_pem(&read("keys/rsa.pub.pem")).unwrap();
    let mut p = oauth1_params(OAuth1Method::RsaSha256);
    p.private_key = &key;
    let base = "GET&http%3A%2F%2Fexample.com%2F&a%3D1";
    let sig = base64::engine::general_purpose::STANDARD
        .decode(oauth1::signature(&p, base).unwrap())
        .unwrap();
    VerifyingKey::<sha2::Sha256>::new(public.clone())
        .verify(
            base.as_bytes(),
            &Signature::try_from(sig.as_slice()).unwrap(),
        )
        .unwrap();
    p.method = OAuth1Method::RsaSha1;
    let sig = base64::engine::general_purpose::STANDARD
        .decode(oauth1::signature(&p, base).unwrap())
        .unwrap();
    VerifyingKey::<sha1::Sha1>::new(public)
        .verify(
            base.as_bytes(),
            &Signature::try_from(sig.as_slice()).unwrap(),
        )
        .unwrap();
}

// ----- finalize -----------------------------------------------------------------------------

fn ctx() -> SignContext {
    SignContext {
        now: 1_700_000_000,
        nonce: "0123456789abcdef".into(),
        allow_local_files: true,
        oauth2_token: Some("tok".into()),
    }
}

fn finalized(auth: Auth, url: &str) -> reqly_http::prepare::PreparedRequest {
    let r = HttpRequest {
        url: url.into(),
        auth,
        headers: vec![KeyValue::new("X-A", "1")],
        ..Default::default()
    };
    let mut p = prepare(&r, &[]).unwrap();
    finalize(&mut p, &ctx()).unwrap();
    p
}

fn authorization(p: &reqly_http::prepare::PreparedRequest) -> &str {
    &p.headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .unwrap()
        .1
}

#[test]
fn finalize_applies_each_scheme() {
    let oauth1 = finalized(
        Auth::OAuth1 {
            consumer_key: "ck".into(),
            consumer_secret: "cs".into(),
            token: String::new(),
            token_secret: String::new(),
            signature_method: OAuth1Method::HmacSha1,
            private_key: String::new(),
            realm: "photos".into(),
            callback: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            timestamp: String::new(),
            location: OAuth1Location::Header,
            include_body_hash: false,
        },
        "https://api.test/x",
    );
    let h = authorization(&oauth1);
    assert!(
        h.starts_with("OAuth realm=\"photos\", oauth_consumer_key=\"ck\""),
        "{h}"
    );
    assert!(
        h.contains("oauth_timestamp=\"1700000000\"")
            && h.contains("oauth_nonce=\"0123456789abcdef\"")
    );

    let query = finalized(
        Auth::OAuth1 {
            consumer_key: "ck".into(),
            consumer_secret: "cs".into(),
            token: String::new(),
            token_secret: String::new(),
            signature_method: OAuth1Method::HmacSha1,
            private_key: String::new(),
            realm: String::new(),
            callback: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            timestamp: String::new(),
            location: OAuth1Location::Query,
            include_body_hash: false,
        },
        "https://api.test/x?a=1",
    );
    assert!(
        query.url.contains("?a=1&oauth_consumer_key=ck") && query.url.contains("oauth_signature="),
        "{}",
        query.url
    );

    let aws = finalized(
        Auth::AwsSigV4 {
            access_key: "AK".into(),
            secret_key: "SK".into(),
            session_token: String::new(),
            region: "eu-west-1".into(),
            service: "s3".into(),
        },
        "https://bucket.s3.amazonaws.com/key",
    );
    assert!(
        authorization(&aws)
            .starts_with("AWS4-HMAC-SHA256 Credential=AK/20231114/eu-west-1/s3/aws4_request")
    );
    assert!(
        aws.headers.iter().any(|(k, _)| k == "X-Amz-Content-Sha256"),
        "s3 signs the payload"
    );

    let jwt = finalized(
        Auth::Jwt {
            algorithm: "HS256".into(),
            secret: "secret".into(),
            payload: "{\"sub\":\"1\"}".into(),
            header: "{\"kid\":\"k1\"}".into(),
            prefix: "JWT".into(),
            location: reqly_core::model::JwtLocation::Header,
            query_param: String::new(),
        },
        "https://api.test/",
    );
    let token = authorization(&jwt)
        .strip_prefix("JWT ")
        .unwrap()
        .to_string();
    let decoded = jsonwebtoken::decode::<Value>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(b"secret"),
        &{
            let mut v = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
            v.required_spec_claims.clear();
            v.validate_exp = false;
            v
        },
    )
    .unwrap();
    assert_eq!(decoded.claims["sub"], "1");
    assert_eq!(decoded.header.kid.as_deref(), Some("k1"));

    let oauth2 = finalized(Auth::OAuth2(Default::default()), "https://api.test/");
    assert_eq!(authorization(&oauth2), "Bearer tok");

    let digest = finalized(
        Auth::Digest {
            username: "u".into(),
            password: "p".into(),
        },
        "https://api.test/",
    );
    assert!(matches!(
        digest.engine_auth,
        Some(reqly_http::prepare::EngineAuth::Digest { .. })
    ));
    let ntlm = finalized(
        Auth::Ntlm {
            username: "u".into(),
            password: "p".into(),
            domain: "CORP".into(),
        },
        "https://api.test/",
    );
    assert!(
        matches!(ntlm.engine_auth, Some(reqly_http::prepare::EngineAuth::Ntlm { ref username, .. }) if username == "CORP\\u")
    );
}

#[test]
fn asap_tokens_verify_with_the_public_key() {
    let p = finalized(
        Auth::Asap {
            issuer: "reqly".into(),
            subject: "user".into(),
            audience: "api, other".into(),
            key_id: "reqly/key1".into(),
            private_key: read("keys/rsa.pem"),
            additional_claims: "{\"scope\":\"read\"}".into(),
            expires_in_secs: 600,
        },
        "https://api.test/",
    );
    let token = authorization(&p)
        .strip_prefix("Bearer ")
        .unwrap()
        .to_string();
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.validate_exp = false;
    validation.set_audience(&["api"]);
    let decoded = jsonwebtoken::decode::<Value>(
        &token,
        &jsonwebtoken::DecodingKey::from_rsa_pem(read("keys/rsa.pub.pem").as_bytes()).unwrap(),
        &validation,
    )
    .unwrap();
    assert_eq!(decoded.header.kid.as_deref(), Some("reqly/key1"));
    assert_eq!(decoded.claims["iss"], "reqly");
    assert_eq!(decoded.claims["aud"], serde_json::json!(["api", "other"]));
    assert_eq!(decoded.claims["exp"], 1_700_000_600);
    assert_eq!(decoded.claims["scope"], "read");
}

#[test]
fn es256_jwt() {
    let p = finalized(
        Auth::Jwt {
            algorithm: "ES256".into(),
            secret: read("keys/ec.pem"),
            payload: "{\"a\":1}".into(),
            header: String::new(),
            prefix: String::new(),
            location: reqly_core::model::JwtLocation::Query,
            query_param: "jwt".into(),
        },
        "https://api.test/x",
    );
    let token = p.url.split("jwt=").nth(1).unwrap();
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims.clear();
    validation.validate_exp = false;
    jsonwebtoken::decode::<Value>(
        token,
        &jsonwebtoken::DecodingKey::from_ec_pem(read("keys/ec.pub.pem").as_bytes()).unwrap(),
        &validation,
    )
    .unwrap();
}

#[test]
fn netrc_needs_local_files() {
    let dir = std::env::temp_dir().join(format!("reqly-netrc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("netrc");
    std::fs::write(&file, "machine api.test login u password p\n").unwrap();
    let auth = Auth::Netrc {
        file: file.display().to_string(),
    };
    let p = finalized(auth.clone(), "https://api.test/");
    assert_eq!(authorization(&p), "Basic dTpw");

    let r = HttpRequest {
        url: "https://api.test/".into(),
        auth,
        ..Default::default()
    };
    let mut p = prepare(&r, &[]).unwrap();
    let mut server = ctx();
    server.allow_local_files = false;
    assert_eq!(
        finalize(&mut p, &server).unwrap_err().code,
        "http.not_allowed"
    );
}

// ----- engine-negotiated schemes ---------------------------------------------------------

async fn send_finalized(
    r: &HttpRequest,
) -> reqly_core::CoreResult<reqly_http::engine::HttpResponse> {
    let mut p = prepare(r, &[])?;
    finalize(&mut p, &ctx())?;
    reqly_http::engine::execute(
        p,
        Default::default(),
        reqly_http::engine::CancelToken::new(),
        None,
    )
    .await
}

#[tokio::test]
async fn digest_md5_and_sha256_with_qop_auth() {
    let server = reqly_testserver::TestServer::start().await;
    for algorithm in ["MD5", "SHA-256"] {
        let r = HttpRequest {
            url: server.url(&format!("/digest/auth/{algorithm}/alice/s3cret")),
            auth: Auth::Digest {
                username: "alice".into(),
                password: "s3cret".into(),
            },
            ..Default::default()
        };
        let res = send_finalized(&r).await.unwrap();
        assert_eq!(res.status, 200, "{algorithm}");
        let wrong = HttpRequest {
            auth: Auth::Digest {
                username: "alice".into(),
                password: "nope".into(),
            },
            ..r
        };
        assert_eq!(
            send_finalized(&wrong).await.unwrap().status,
            401,
            "{algorithm}"
        );
    }
}

#[tokio::test]
async fn digest_qop_auth_int() {
    let server = reqly_testserver::TestServer::start().await;
    let r = HttpRequest {
        method: "POST".into(),
        url: server.url("/digest/auth-int/MD5/alice/s3cret"),
        body: reqly_core::model::Body {
            kind: reqly_core::model::BodyKind::Text,
            text: "hello".into(),
            ..Default::default()
        },
        auth: Auth::Digest {
            username: "alice".into(),
            password: "s3cret".into(),
        },
        ..Default::default()
    };
    let res = send_finalized(&r).await.unwrap();
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn ntlm_v2_handshake() {
    let server = reqly_testserver::TestServer::start().await;
    let r = HttpRequest {
        url: server.url("/ntlm/alice/s3cret"),
        auth: Auth::Ntlm {
            username: "alice".into(),
            password: "s3cret".into(),
            domain: "CORP".into(),
        },
        ..Default::default()
    };
    let res = send_finalized(&r).await.unwrap();
    assert_eq!(
        res.status,
        200,
        "{}",
        String::from_utf8_lossy(&res.body.to_vec().unwrap())
    );
    let body: Value = serde_json::from_slice(&res.body.to_vec().unwrap()).unwrap();
    assert_eq!(body["domain"], "CORP");
    let wrong = HttpRequest {
        auth: Auth::Ntlm {
            username: "alice".into(),
            password: "nope".into(),
            domain: "CORP".into(),
        },
        ..r
    };
    assert_eq!(send_finalized(&wrong).await.unwrap().status, 403);
}
