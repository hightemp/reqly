//! Entity identifiers: time-ordered UUIDv7, serialized as strings.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, type = "string")]
pub struct Id(Uuid);

impl Id {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Id {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| CoreError::new("id.invalid", "invalid id").with_param("value", s))
    }
}

impl From<Uuid> for Id {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_v7_and_time_ordered() {
        let a = Id::new();
        let b = Id::new();
        assert_eq!(a.as_uuid().get_version_num(), 7);
        assert!(a < b);
    }

    #[test]
    fn round_trips_through_string() {
        let id = Id::new();
        assert_eq!(id.to_string().parse::<Id>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{id}\""));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!("nope".parse::<Id>().unwrap_err().code, "id.invalid");
    }
}
