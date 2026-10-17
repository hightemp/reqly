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
}
