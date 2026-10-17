use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OAuth1Method {
    #[default]
    #[serde(rename = "HMAC-SHA1")]
    HmacSha1,
    #[serde(rename = "HMAC-SHA256")]
    HmacSha256,
    #[serde(rename = "HMAC-SHA512")]
    HmacSha512,
    #[serde(rename = "RSA-SHA1")]
    RsaSha1,
    #[serde(rename = "RSA-SHA256")]
    RsaSha256,
    #[serde(rename = "PLAINTEXT")]
    Plaintext,
}

/// Where OAuth 1.0 parameters go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OAuth1Location {
    #[default]
    Header,
    Query,
    /// Form body (`application/x-www-form-urlencoded` requests only).
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OAuth2Grant {
    #[default]
    AuthorizationCode,
    Implicit,
    Password,
    ClientCredentials,
    DeviceCode,
}

/// How the client authenticates at the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OAuth2ClientAuth {
    /// `Authorization: Basic base64(client_id:client_secret)`.
    #[default]
    Basic,
    /// `client_id` and `client_secret` in the form body.
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct OAuth2Config {
    pub grant: OAuth2Grant,
    pub auth_url: String,
    pub token_url: String,
    /// Device authorization endpoint (device code grant).
    pub device_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub client_auth: OAuth2ClientAuth,
    pub scope: String,
    pub audience: String,
    pub resource: String,
    pub username: String,
    pub password: String,
    /// Redirect URI registered at the provider; empty = Reqly's default.
    pub redirect_url: String,
    pub pkce: bool,
    pub state: String,
    /// `Authorization: <prefix> <token>`; empty = `Bearer`.
    pub token_prefix: String,
    /// Send the ID token instead of the access token.
    pub use_id_token: bool,
    /// Refresh expired tokens automatically when a refresh token exists.
    pub auto_refresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HawkAlgorithm {
    #[default]
    Sha256,
    Sha1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JwtLocation {
    #[default]
    Header,
    Query,
}

/// Request authentication. `Inherit` uses the nearest folder that defines one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Auth {
    #[default]
    Inherit,
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
        /// Defaults to `Bearer` when empty.
        prefix: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
    /// HTTP Digest (MD5 / SHA-256, qop auth / auth-int), negotiated by the engine.
    Digest {
        username: String,
        password: String,
    },
    /// NTLM v2, negotiated by the engine.
    Ntlm {
        username: String,
        password: String,
        /// Windows domain; may also be given as `DOMAIN\\user`.
        domain: String,
    },
    #[serde(rename = "oauth1")]
    OAuth1 {
        consumer_key: String,
        consumer_secret: String,
        token: String,
        token_secret: String,
        signature_method: OAuth1Method,
        /// PEM private key for RSA methods.
        private_key: String,
        realm: String,
        callback: String,
        verifier: String,
        /// Empty: generated per request.
        nonce: String,
        /// Unix seconds; empty: the current time.
        timestamp: String,
        location: OAuth1Location,
        /// Adds `oauth_body_hash` for non-form bodies.
        include_body_hash: bool,
    },
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2Config),
    #[serde(rename = "awsSigV4")]
    AwsSigV4 {
        access_key: String,
        secret_key: String,
        session_token: String,
        region: String,
        service: String,
    },
    Hawk {
        id: String,
        key: String,
        algorithm: HawkAlgorithm,
        ext: String,
        app: String,
        dlg: String,
        include_payload_hash: bool,
    },
    /// Atlassian ASAP: an RS256 JWT signed per request.
    Asap {
        issuer: String,
        subject: String,
        audience: String,
        key_id: String,
        private_key: String,
        /// Extra claims as a JSON object.
        additional_claims: String,
        expires_in_secs: u64,
    },
    /// Credentials from a netrc file (desktop only).
    Netrc {
        /// Empty: `~/.netrc` (`_netrc` on Windows).
        file: String,
    },
    /// Akamai EdgeGrid (EG1-HMAC-SHA256).
    EdgeGrid {
        access_token: String,
        client_token: String,
        client_secret: String,
        /// Request headers included in the signature (comma separated).
        headers_to_sign: String,
        /// Body bytes included in the content hash (default 131072).
        max_body: u64,
    },
    /// A JWT generated and signed per request.
    Jwt {
        algorithm: String,
        /// HMAC secret or PEM private key.
        secret: String,
        /// Claims as a JSON object (templates allowed).
        payload: String,
        /// Extra header fields as a JSON object.
        header: String,
        /// `Authorization: <prefix> <token>`; empty = `Bearer`.
        prefix: String,
        location: JwtLocation,
        /// Query parameter name for `location: query`.
        query_param: String,
    },
}
