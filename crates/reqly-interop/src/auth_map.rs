//! Mapping of the advanced auth schemes (stage 4) to and from other tools.
//! Basic, bearer and API key stay in each importer next to their quirks.

use reqly_core::model::{
    Auth, HawkAlgorithm, JwtLocation, OAuth1Location, OAuth1Method, OAuth2ClientAuth, OAuth2Config,
    OAuth2Grant,
};
use serde_json::{Value, json};

fn oauth1_method(s: &str) -> OAuth1Method {
    match s.to_ascii_uppercase().as_str() {
        "HMAC-SHA256" => OAuth1Method::HmacSha256,
        "HMAC-SHA512" => OAuth1Method::HmacSha512,
        "RSA-SHA1" => OAuth1Method::RsaSha1,
        "RSA-SHA256" | "RSA-SHA512" => OAuth1Method::RsaSha256,
        "PLAINTEXT" => OAuth1Method::Plaintext,
        _ => OAuth1Method::HmacSha1,
    }
}

fn oauth1_method_name(m: OAuth1Method) -> &'static str {
    match m {
        OAuth1Method::HmacSha1 => "HMAC-SHA1",
        OAuth1Method::HmacSha256 => "HMAC-SHA256",
        OAuth1Method::HmacSha512 => "HMAC-SHA512",
        OAuth1Method::RsaSha1 => "RSA-SHA1",
        OAuth1Method::RsaSha256 => "RSA-SHA256",
        OAuth1Method::Plaintext => "PLAINTEXT",
    }
}

fn hawk_algorithm(s: &str) -> HawkAlgorithm {
    if s.eq_ignore_ascii_case("sha1") {
        HawkAlgorithm::Sha1
    } else {
        HawkAlgorithm::Sha256
    }
}

fn hawk_name(a: HawkAlgorithm) -> &'static str {
    match a {
        HawkAlgorithm::Sha256 => "sha256",
        HawkAlgorithm::Sha1 => "sha1",
    }
}

fn grant(s: &str) -> Option<(OAuth2Grant, bool)> {
    Some(match s {
        "authorization_code" | "authorizationCode" => (OAuth2Grant::AuthorizationCode, false),
        "authorization_code_with_pkce" => (OAuth2Grant::AuthorizationCode, true),
        "implicit" => (OAuth2Grant::Implicit, false),
        "password" | "password_credentials" => (OAuth2Grant::Password, false),
        "client_credentials" => (OAuth2Grant::ClientCredentials, false),
        "device_code" | "urn:ietf:params:oauth:grant-type:device_code" => {
            (OAuth2Grant::DeviceCode, false)
        }
        _ => return None,
    })
}

/// Field access over another tool's auth object.
pub struct Fields<'a> {
    pub get: &'a dyn Fn(&str) -> String,
}

impl Fields<'_> {
    fn s(&self, k: &str) -> String {
        (self.get)(k)
    }

    fn first(&self, keys: &[&str]) -> String {
        keys.iter()
            .map(|k| self.s(k))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }

    fn flag(&self, k: &str) -> bool {
        self.s(k) == "true"
    }
}

/// Insomnia v4/v5 auth types. `None` when the type is unknown.
pub fn from_insomnia(ty: &str, f: &Fields) -> Option<Auth> {
    Some(match ty {
        "digest" => Auth::Digest {
            username: f.s("username"),
            password: f.s("password"),
        },
        "ntlm" => Auth::Ntlm {
            username: f.s("username"),
            password: f.s("password"),
            domain: String::new(),
        },
        "oauth1" => Auth::OAuth1 {
            consumer_key: f.s("consumerKey"),
            consumer_secret: f.s("consumerSecret"),
            token: f.s("tokenKey"),
            token_secret: f.s("tokenSecret"),
            signature_method: oauth1_method(&f.s("signatureMethod")),
            private_key: f.s("privateKey"),
            realm: f.s("realm"),
            callback: f.s("callback"),
            verifier: f.s("verifier"),
            nonce: f.s("nonce"),
            timestamp: f.s("timestamp"),
            location: OAuth1Location::Header,
            include_body_hash: f.flag("includeBodyHash"),
        },
        "oauth2" => {
            let (grant, _) =
                grant(&f.s("grantType")).unwrap_or((OAuth2Grant::AuthorizationCode, false));
            Auth::OAuth2(OAuth2Config {
                grant,
                auth_url: f.s("authorizationUrl"),
                token_url: f.s("accessTokenUrl"),
                client_id: f.s("clientId"),
                client_secret: f.s("clientSecret"),
                client_auth: if f.flag("credentialsInBody") {
                    OAuth2ClientAuth::Body
                } else {
                    OAuth2ClientAuth::Basic
                },
                scope: f.s("scope"),
                audience: f.s("audience"),
                resource: f.s("resource"),
                username: f.s("username"),
                password: f.s("password"),
                redirect_url: f.s("redirectUrl"),
                pkce: f.flag("usePkce"),
                state: f.s("state"),
                token_prefix: f.s("tokenPrefix"),
                use_id_token: f.s("responseType") == "id_token",
                auto_refresh: true,
                ..OAuth2Config::default()
            })
        }
        "iam" => Auth::AwsSigV4 {
            access_key: f.s("accessKeyId"),
            secret_key: f.s("secretAccessKey"),
            session_token: f.s("sessionToken"),
            region: f.s("region"),
            service: f.s("service"),
        },
        "hawk" => Auth::Hawk {
            id: f.s("id"),
            key: f.s("key"),
            algorithm: hawk_algorithm(&f.s("algorithm")),
            ext: f.s("ext"),
            app: String::new(),
            dlg: String::new(),
            include_payload_hash: f.flag("validatePayload"),
        },
        "asap" => Auth::Asap {
            issuer: f.s("issuer"),
            subject: f.s("subject"),
            audience: f.s("audience"),
            key_id: f.s("keyId"),
            private_key: f.s("privateKey"),
            additional_claims: f.s("additionalClaims"),
            expires_in_secs: 600,
        },
        "netrc" => Auth::Netrc {
            file: String::new(),
        },
        _ => return None,
    })
}

/// Insomnia representation; `None` for schemes Insomnia lacks.
pub fn to_insomnia(auth: &Auth, t: &dyn Fn(&str) -> String) -> Option<Value> {
    Some(match auth {
        Auth::Digest { username, password } => {
            json!({ "type": "digest", "username": t(username), "password": t(password), "disabled": false })
        }
        Auth::Ntlm {
            username,
            password,
            domain,
        } => {
            let user = if domain.is_empty() {
                t(username)
            } else {
                format!("{}\\{}", t(domain), t(username))
            };
            json!({ "type": "ntlm", "username": user, "password": t(password), "disabled": false })
        }
        Auth::OAuth1 {
            consumer_key,
            consumer_secret,
            token,
            token_secret,
            signature_method,
            private_key,
            realm,
            callback,
            verifier,
            nonce,
            timestamp,
            include_body_hash,
            ..
        } => json!({
            "type": "oauth1", "disabled": false, "consumerKey": t(consumer_key), "consumerSecret": t(consumer_secret),
            "tokenKey": t(token), "tokenSecret": t(token_secret), "signatureMethod": oauth1_method_name(*signature_method),
            "privateKey": t(private_key), "realm": t(realm), "callback": t(callback), "verifier": t(verifier),
            "nonce": t(nonce), "timestamp": t(timestamp), "version": "1.0", "includeBodyHash": include_body_hash
        }),
        Auth::OAuth2(c) => {
            let grant = match c.grant {
                OAuth2Grant::AuthorizationCode | OAuth2Grant::DeviceCode => "authorization_code",
                OAuth2Grant::Implicit => "implicit",
                OAuth2Grant::Password => "password",
                OAuth2Grant::ClientCredentials => "client_credentials",
            };
            json!({
                "type": "oauth2", "disabled": false, "grantType": grant, "authorizationUrl": t(&c.auth_url),
                "accessTokenUrl": t(&c.token_url), "clientId": t(&c.client_id), "clientSecret": t(&c.client_secret),
                "credentialsInBody": c.client_auth == OAuth2ClientAuth::Body, "scope": t(&c.scope),
                "audience": t(&c.audience), "resource": t(&c.resource), "username": t(&c.username),
                "password": t(&c.password), "redirectUrl": t(&c.redirect_url), "usePkce": c.pkce,
                "pkceMethod": "S256", "state": t(&c.state), "tokenPrefix": t(&c.token_prefix),
                "responseType": if c.use_id_token { "id_token" } else { "token" }
            })
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            session_token,
            region,
            service,
        } => json!({
            "type": "iam", "disabled": false, "accessKeyId": t(access_key), "secretAccessKey": t(secret_key),
            "sessionToken": t(session_token), "region": t(region), "service": t(service)
        }),
        Auth::Hawk {
            id,
            key,
            algorithm,
            ext,
            include_payload_hash,
            ..
        } => json!({
            "type": "hawk", "disabled": false, "id": t(id), "key": t(key), "algorithm": hawk_name(*algorithm),
            "ext": t(ext), "validatePayload": include_payload_hash
        }),
        Auth::Asap {
            issuer,
            subject,
            audience,
            key_id,
            private_key,
            additional_claims,
            ..
        } => json!({
            "type": "asap", "disabled": false, "issuer": t(issuer), "subject": t(subject), "audience": t(audience),
            "keyId": t(key_id), "privateKey": t(private_key), "additionalClaims": t(additional_claims)
        }),
        Auth::Netrc { .. } => json!({ "type": "netrc", "disabled": false }),
        _ => return None,
    })
}

/// Postman v2.0/v2.1 auth types.
pub fn from_postman(ty: &str, f: &Fields) -> Option<Auth> {
    Some(match ty {
        "digest" => Auth::Digest {
            username: f.s("username"),
            password: f.s("password"),
        },
        "ntlm" => Auth::Ntlm {
            username: f.s("username"),
            password: f.s("password"),
            domain: f.s("domain"),
        },
        "oauth1" => Auth::OAuth1 {
            consumer_key: f.s("consumerKey"),
            consumer_secret: f.s("consumerSecret"),
            token: f.s("token"),
            token_secret: f.s("tokenSecret"),
            signature_method: oauth1_method(&f.s("signatureMethod")),
            private_key: f.s("privateKey"),
            realm: f.s("realm"),
            callback: f.s("callback"),
            verifier: f.s("verifier"),
            nonce: f.s("nonce"),
            timestamp: f.s("timestamp"),
            location: match f.s("addParamsToHeader").as_str() {
                "false" => OAuth1Location::Query,
                _ => OAuth1Location::Header,
            },
            include_body_hash: f.flag("includeBodyHash"),
        },
        "oauth2" => {
            let (grant, pkce) =
                grant(&f.s("grant_type")).unwrap_or((OAuth2Grant::AuthorizationCode, false));
            Auth::OAuth2(OAuth2Config {
                grant,
                auth_url: f.s("authUrl"),
                token_url: f.s("accessTokenUrl"),
                client_id: f.s("clientId"),
                client_secret: f.s("clientSecret"),
                client_auth: if f.s("client_authentication") == "body" {
                    OAuth2ClientAuth::Body
                } else {
                    OAuth2ClientAuth::Basic
                },
                scope: f.s("scope"),
                audience: f.s("audience"),
                resource: f.s("resource"),
                username: f.s("username"),
                password: f.s("password"),
                redirect_url: f.first(&["redirect_uri", "callbackUrl"]),
                pkce,
                state: f.s("state"),
                token_prefix: f.s("headerPrefix"),
                auto_refresh: true,
                ..OAuth2Config::default()
            })
        }
        "awsv4" => Auth::AwsSigV4 {
            access_key: f.s("accessKey"),
            secret_key: f.s("secretKey"),
            session_token: f.s("sessionToken"),
            region: f.s("region"),
            service: f.s("service"),
        },
        "hawk" => Auth::Hawk {
            id: f.s("authId"),
            key: f.s("authKey"),
            algorithm: hawk_algorithm(&f.s("algorithm")),
            ext: f.s("extraData"),
            app: f.s("app"),
            dlg: f.s("delegation"),
            include_payload_hash: f.flag("includePayloadHash"),
        },
        "edgegrid" | "akamai" => Auth::EdgeGrid {
            access_token: f.s("accessToken"),
            client_token: f.s("clientToken"),
            client_secret: f.s("clientSecret"),
            headers_to_sign: f.s("headersToSign"),
            max_body: 131_072,
        },
        "jwt" => Auth::Jwt {
            algorithm: f.first(&["algorithm"]).to_uppercase(),
            secret: f.first(&["secret", "privateKey"]),
            payload: f.s("payload"),
            header: f.s("header"),
            prefix: f.s("headerPrefix"),
            location: if f.s("addTokenTo") == "queryParam" {
                JwtLocation::Query
            } else {
                JwtLocation::Header
            },
            query_param: f.first(&["queryParamKey"]),
        },
        "asap" => Auth::Asap {
            issuer: f.s("iss"),
            subject: f.s("sub"),
            audience: f.s("aud"),
            key_id: f.s("kid"),
            private_key: f.s("privateKey"),
            additional_claims: f.s("claims"),
            expires_in_secs: f.s("exp").parse().unwrap_or(600),
        },
        _ => return None,
    })
}

/// Postman `(type, params)`; `None` for schemes Postman lacks.
pub fn to_postman(
    auth: &Auth,
    t: &dyn Fn(&str) -> String,
) -> Option<(&'static str, Vec<(&'static str, Value)>)> {
    let s = |v: &str| Value::String(t(v));
    Some(match auth {
        Auth::Digest { username, password } => (
            "digest",
            vec![("username", s(username)), ("password", s(password))],
        ),
        Auth::Ntlm {
            username,
            password,
            domain,
        } => (
            "ntlm",
            vec![
                ("username", s(username)),
                ("password", s(password)),
                ("domain", s(domain)),
            ],
        ),
        Auth::OAuth1 {
            consumer_key,
            consumer_secret,
            token,
            token_secret,
            signature_method,
            private_key,
            realm,
            callback,
            verifier,
            nonce,
            timestamp,
            location,
            include_body_hash,
        } => (
            "oauth1",
            vec![
                ("consumerKey", s(consumer_key)),
                ("consumerSecret", s(consumer_secret)),
                ("token", s(token)),
                ("tokenSecret", s(token_secret)),
                (
                    "signatureMethod",
                    json!(oauth1_method_name(*signature_method)),
                ),
                ("privateKey", s(private_key)),
                ("realm", s(realm)),
                ("callback", s(callback)),
                ("verifier", s(verifier)),
                ("nonce", s(nonce)),
                ("timestamp", s(timestamp)),
                ("version", json!("1.0")),
                (
                    "addParamsToHeader",
                    json!(*location == OAuth1Location::Header),
                ),
                ("includeBodyHash", json!(include_body_hash)),
            ],
        ),
        Auth::OAuth2(c) => {
            let grant = match (c.grant, c.pkce) {
                (OAuth2Grant::AuthorizationCode, true) => "authorization_code_with_pkce",
                (OAuth2Grant::AuthorizationCode | OAuth2Grant::DeviceCode, _) => {
                    "authorization_code"
                }
                (OAuth2Grant::Implicit, _) => "implicit",
                (OAuth2Grant::Password, _) => "password_credentials",
                (OAuth2Grant::ClientCredentials, _) => "client_credentials",
            };
            (
                "oauth2",
                vec![
                    ("grant_type", json!(grant)),
                    ("authUrl", s(&c.auth_url)),
                    ("accessTokenUrl", s(&c.token_url)),
                    ("clientId", s(&c.client_id)),
                    ("clientSecret", s(&c.client_secret)),
                    (
                        "client_authentication",
                        json!(if c.client_auth == OAuth2ClientAuth::Body {
                            "body"
                        } else {
                            "header"
                        }),
                    ),
                    ("scope", s(&c.scope)),
                    ("audience", s(&c.audience)),
                    ("resource", s(&c.resource)),
                    ("username", s(&c.username)),
                    ("password", s(&c.password)),
                    ("redirect_uri", s(&c.redirect_url)),
                    ("state", s(&c.state)),
                    ("headerPrefix", s(&c.token_prefix)),
                    ("addTokenTo", json!("header")),
                ],
            )
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            session_token,
            region,
            service,
        } => (
            "awsv4",
            vec![
                ("accessKey", s(access_key)),
                ("secretKey", s(secret_key)),
                ("sessionToken", s(session_token)),
                ("region", s(region)),
                ("service", s(service)),
            ],
        ),
        Auth::Hawk {
            id,
            key,
            algorithm,
            ext,
            app,
            dlg,
            include_payload_hash,
        } => (
            "hawk",
            vec![
                ("authId", s(id)),
                ("authKey", s(key)),
                ("algorithm", json!(hawk_name(*algorithm))),
                ("extraData", s(ext)),
                ("app", s(app)),
                ("delegation", s(dlg)),
                ("includePayloadHash", json!(include_payload_hash)),
            ],
        ),
        Auth::EdgeGrid {
            access_token,
            client_token,
            client_secret,
            headers_to_sign,
            ..
        } => (
            "edgegrid",
            vec![
                ("accessToken", s(access_token)),
                ("clientToken", s(client_token)),
                ("clientSecret", s(client_secret)),
                ("headersToSign", s(headers_to_sign)),
            ],
        ),
        Auth::Jwt {
            algorithm,
            secret,
            payload,
            header,
            prefix,
            location,
            query_param,
        } => (
            "jwt",
            vec![
                ("algorithm", json!(algorithm)),
                (
                    if algorithm.starts_with("HS") {
                        "secret"
                    } else {
                        "privateKey"
                    },
                    s(secret),
                ),
                ("payload", s(payload)),
                ("header", s(header)),
                ("headerPrefix", s(prefix)),
                (
                    "addTokenTo",
                    json!(if *location == JwtLocation::Query {
                        "queryParam"
                    } else {
                        "header"
                    }),
                ),
                ("queryParamKey", s(query_param)),
            ],
        ),
        Auth::Asap {
            issuer,
            subject,
            audience,
            key_id,
            private_key,
            additional_claims,
            expires_in_secs,
        } => (
            "asap",
            vec![
                ("alg", json!("RS256")),
                ("iss", s(issuer)),
                ("sub", s(subject)),
                ("aud", s(audience)),
                ("kid", s(key_id)),
                ("privateKey", s(private_key)),
                ("claims", s(additional_claims)),
                ("exp", json!(expires_in_secs.to_string())),
            ],
        ),
        _ => return None,
    })
}

/// Bruno `auth:<mode>` blocks.
pub fn from_bruno(mode: &str, f: &Fields) -> Option<Auth> {
    Some(match mode {
        "digest" => Auth::Digest {
            username: f.s("username"),
            password: f.s("password"),
        },
        "ntlm" => Auth::Ntlm {
            username: f.s("username"),
            password: f.s("password"),
            domain: f.s("domain"),
        },
        "awsv4" => Auth::AwsSigV4 {
            access_key: f.s("accessKeyId"),
            secret_key: f.s("secretAccessKey"),
            session_token: f.s("sessionToken"),
            region: f.s("region"),
            service: f.s("service"),
        },
        "oauth2" => {
            let (grant, _) =
                grant(&f.s("grant_type")).unwrap_or((OAuth2Grant::AuthorizationCode, false));
            Auth::OAuth2(OAuth2Config {
                grant,
                auth_url: f.s("authorization_url"),
                token_url: f.s("access_token_url"),
                client_id: f.s("client_id"),
                client_secret: f.s("client_secret"),
                client_auth: if f.s("credentials_placement") == "body" {
                    OAuth2ClientAuth::Body
                } else {
                    OAuth2ClientAuth::Basic
                },
                scope: f.s("scope"),
                username: f.s("username"),
                password: f.s("password"),
                redirect_url: f.s("callback_url"),
                pkce: f.flag("pkce"),
                state: f.s("state"),
                token_prefix: f.s("token_header_prefix"),
                auto_refresh: f.s("auto_refresh_token") != "false",
                ..OAuth2Config::default()
            })
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fields<'a>(map: &'a HashMap<&'a str, &'a str>) -> impl Fn(&str) -> String + 'a {
        move |k| map.get(k).map(|v| v.to_string()).unwrap_or_default()
    }

    #[test]
    fn postman_round_trip() {
        let samples = [
            Auth::Digest {
                username: "u".into(),
                password: "p".into(),
            },
            Auth::Ntlm {
                username: "u".into(),
                password: "p".into(),
                domain: "D".into(),
            },
            Auth::AwsSigV4 {
                access_key: "a".into(),
                secret_key: "s".into(),
                session_token: "t".into(),
                region: "r".into(),
                service: "x".into(),
            },
            Auth::OAuth2(OAuth2Config {
                grant: OAuth2Grant::AuthorizationCode,
                pkce: true,
                client_id: "c".into(),
                auto_refresh: true,
                ..Default::default()
            }),
            Auth::Hawk {
                id: "i".into(),
                key: "k".into(),
                algorithm: HawkAlgorithm::Sha1,
                ext: "e".into(),
                app: "a".into(),
                dlg: "d".into(),
                include_payload_hash: true,
            },
        ];
        for auth in samples {
            let (ty, params) = to_postman(&auth, &|s| s.to_string()).unwrap();
            let map: HashMap<&str, String> = params
                .iter()
                .map(|(k, v)| {
                    (
                        *k,
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string()),
                    )
                })
                .collect();
            let get = |k: &str| map.get(k).cloned().unwrap_or_default();
            let back = from_postman(ty, &Fields { get: &get }).unwrap();
            assert_eq!(back, auth, "{ty}");
        }
    }

    #[test]
    fn insomnia_oauth2() {
        let map = HashMap::from([
            ("grantType", "client_credentials"),
            ("accessTokenUrl", "https://t"),
            ("clientId", "c"),
            ("credentialsInBody", "true"),
        ]);
        let get = fields(&map);
        let Some(Auth::OAuth2(c)) = from_insomnia("oauth2", &Fields { get: &get }) else {
            panic!()
        };
        assert_eq!(c.grant, OAuth2Grant::ClientCredentials);
        assert_eq!(c.client_auth, OAuth2ClientAuth::Body);
        assert_eq!(c.token_url, "https://t");
    }
}
