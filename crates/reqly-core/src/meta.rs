//! Metadata carried by every syncable entity (see ADR-004).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::clock::{Clock, Timestamp};
use crate::id::Id;
use crate::sort_key::SortKey;

/// Monotonic per-entity revision counter, bumped on every change.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, TS,
)]
#[ts(export, type = "number")]
pub struct Version(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Meta {
    pub id: Id,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub version: Version,
    pub deleted_at: Option<Timestamp>,
    pub sort_key: SortKey,
}

impl Meta {
    pub fn new(clock: &dyn Clock, sort_key: SortKey) -> Self {
        let now = clock.now();
        Self {
            id: Id::new(),
            created_at: now,
            updated_at: now,
            version: Version(1),
            deleted_at: None,
            sort_key,
        }
    }

    pub fn touch(&mut self, clock: &dyn Clock) {
        self.updated_at = clock.now();
        self.version.0 += 1;
    }

    pub fn soft_delete(&mut self, clock: &dyn Clock) {
        self.deleted_at = Some(clock.now());
        self.touch(clock);
    }

    pub fn restore(&mut self, clock: &dyn Clock) {
        self.deleted_at = None;
        self.touch(clock);
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    fn meta(clock: &FixedClock) -> Meta {
        Meta::new(clock, SortKey::between(None, None).unwrap())
    }

    #[test]
    fn new_meta_starts_at_version_one() {
        let clock = FixedClock::new(10);
        let m = meta(&clock);
        assert_eq!(m.version, Version(1));
        assert_eq!(m.created_at, Timestamp(10));
        assert_eq!(m.updated_at, Timestamp(10));
        assert!(!m.is_deleted());
    }

    #[test]
    fn touch_bumps_version_and_updated_at() {
        let clock = FixedClock::new(10);
        let mut m = meta(&clock);
        clock.advance(5);
        m.touch(&clock);
        assert_eq!(m.version, Version(2));
        assert_eq!(m.updated_at, Timestamp(15));
        assert_eq!(m.created_at, Timestamp(10));
    }

    #[test]
    fn soft_delete_and_restore_are_versioned_changes() {
        let clock = FixedClock::new(10);
        let mut m = meta(&clock);
        m.soft_delete(&clock);
        assert!(m.is_deleted());
        m.restore(&clock);
        assert!(!m.is_deleted());
        assert_eq!(m.version, Version(3));
    }

    #[test]
    fn serializes_camel_case() {
        let clock = FixedClock::new(10);
        let json = serde_json::to_value(meta(&clock)).unwrap();
        assert!(json.get("createdAt").is_some());
        assert!(json.get("sortKey").is_some());
    }
}
