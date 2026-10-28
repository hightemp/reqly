//! Results of a collection run.

use reqly_core::{CoreError, Id};
use reqly_script::TestResult;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AssertionResult;

/// One request sent by a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunItem {
    /// Zero-based.
    pub iteration: u32,
    pub request_id: Id,
    pub name: String,
    /// Names of the folders the request is in, outermost first.
    pub path: Vec<String>,
    pub method: String,
    pub url: String,
    #[ts(optional)]
    pub status: Option<u16>,
    #[ts(optional)]
    pub total_ms: Option<f64>,
    /// The response kept in history.
    #[ts(optional)]
    pub response_id: Option<Id>,
    /// The request failed (no response) or a script stopped it.
    #[ts(optional)]
    pub error: Option<CoreError>,
    pub tests: Vec<TestResult>,
    pub assertions: Vec<AssertionResult>,
}

impl RunItem {
    /// No error and every test and assertion passed (the status alone does not count).
    pub fn passed(&self) -> bool {
        self.error.is_none()
            && self.tests.iter().all(|t| t.passed)
            && self.assertions.iter().all(|a| a.passed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunReport {
    pub run_id: String,
    /// The collection or folder that ran.
    pub name: String,
    #[ts(optional)]
    pub environment: Option<String>,
    /// Unix milliseconds.
    pub started_at: i64,
    pub finished_at: i64,
    pub iterations: u32,
    pub items: Vec<RunItem>,
    /// Stopped by the user.
    pub cancelled: bool,
    /// Stopped at the first failure ("stop on failure").
    pub stopped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunStats {
    pub requests: u32,
    /// Requests with an error or a failed check.
    pub failed_requests: u32,
    /// Requests without a response.
    pub errors: u32,
    /// Tests and assertions that ran (not skipped).
    pub checks: u32,
    pub failed_checks: u32,
    pub skipped_checks: u32,
    pub duration_ms: i64,
}

impl RunReport {
    pub fn stats(&self) -> RunStats {
        let mut s = RunStats {
            duration_ms: self.finished_at - self.started_at,
            ..RunStats::default()
        };
        for item in &self.items {
            s.requests += 1;
            s.failed_requests += u32::from(!item.passed());
            s.errors += u32::from(item.error.is_some());
            for t in &item.tests {
                if t.skipped {
                    s.skipped_checks += 1;
                } else {
                    s.checks += 1;
                    s.failed_checks += u32::from(!t.passed);
                }
            }
            s.checks += item.assertions.len() as u32;
            s.failed_checks += item.assertions.iter().filter(|a| !a.passed).count() as u32;
        }
        s
    }

    /// Every request passed and the run was not cancelled.
    pub fn passed(&self) -> bool {
        !self.cancelled && self.items.iter().all(RunItem::passed)
    }
}
