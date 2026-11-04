//! The client side of a sync round: pull, then push (ADR-013).

use std::collections::HashMap;

use async_trait::async_trait;
use reqly_core::{CoreResult, Id};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::merge::{Merged, merge};
use crate::protocol::{Change, Cursor, PullInput, PullOutput, PushInput, PushOutput, PushResult};
use crate::record::{Kind, Record, dependency_order};

/// A synced entity on the client: the local record (`None`: the row is
/// gone) and the base, the record as last got from the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub local: Option<Record>,
    pub base: Option<Record>,
}

impl Entry {
    /// The local record, a deletion for a row that is gone.
    pub fn current(&self) -> Option<Record> {
        self.local.clone().or_else(|| {
            self.base.clone().map(|mut b| {
                b.deleted = true;
                b
            })
        })
    }

    pub fn is_changed(&self) -> bool {
        match (&self.local, &self.base) {
            (Some(l), Some(b)) => !l.same_content(b),
            (None, Some(b)) => !b.deleted,
            (Some(_), None) => true,
            (None, None) => false,
        }
    }
}

/// Deleted on one side and changed on the other; waits for the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncConflict")]
pub struct Conflict {
    /// The local version (deleted or changed).
    pub local: Record,
    /// The server's version.
    pub remote: Record,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncResolution")]
pub enum Resolution {
    /// Send the local version over the server's.
    KeepLocal,
    /// Take the server's version.
    TakeRemote,
}

/// The client's synced data.
#[async_trait]
pub trait LocalStore: Send + Sync {
    /// The synced workspaces and how far they are pulled.
    async fn cursors(&self) -> CoreResult<Vec<Cursor>>;
    async fn set_cursor(&self, cursor: Cursor) -> CoreResult<()>;
    async fn entry(&self, kind: Kind, id: Id) -> CoreResult<Entry>;
    /// Entries of synced workspaces with a local change ([`Entry::is_changed`]).
    async fn changed(&self) -> CoreResult<Vec<Entry>>;
    /// Writes `local` as the local record (`None`: leaves the row as is) and
    /// makes `base` its base.
    async fn write(&self, local: Option<&Record>, base: &Record) -> CoreResult<()>;
    async fn conflicts(&self) -> CoreResult<Vec<Conflict>>;
    async fn set_conflict(&self, conflict: &Conflict) -> CoreResult<()>;
    async fn remove_conflict(&self, kind: Kind, id: Id) -> CoreResult<()>;
}

/// The server, over the network or in process.
#[async_trait]
pub trait Remote: Send + Sync {
    async fn pull(&self, input: PullInput) -> CoreResult<PullOutput>;
    async fn push(&self, input: PushInput) -> CoreResult<PushOutput>;
}

/// What a round did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncReport")]
pub struct Report {
    /// Server records written locally.
    pub pulled: u32,
    /// Local changes the server stored.
    pub pushed: u32,
    /// Conflicts waiting for the user after the round.
    pub conflicts: u32,
    /// Local changes the server refused, with the error code.
    pub rejected: Vec<Rejected>,
    /// Synced workspaces the server no longer gives (deleted, or access
    /// taken away).
    pub unavailable: Vec<Id>,
    /// Records written locally (to refresh the UI).
    #[serde(skip)]
    #[ts(skip)]
    pub written: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SyncRejected")]
pub struct Rejected {
    pub kind: Kind,
    pub id: Id,
    pub code: String,
}

/// One sync round: pull the server's changes, then push the local ones.
pub async fn round(
    local: &dyn LocalStore,
    remote: &dyn Remote,
    secrets: bool,
) -> CoreResult<Report> {
    let mut report = Report::default();
    let cursors = local.cursors().await?;
    if cursors.is_empty() {
        return Ok(report);
    }
    pull(local, remote, cursors, secrets, &mut report).await?;
    push(local, remote, secrets, &mut report).await?;
    report.conflicts = local.conflicts().await?.len() as u32;
    Ok(report)
}

async fn pull(
    local: &dyn LocalStore,
    remote: &dyn Remote,
    cursors: Vec<Cursor>,
    secrets: bool,
    report: &mut Report,
) -> CoreResult<()> {
    let out = remote.pull(PullInput { cursors, secrets }).await?;
    let mut conflicts: HashMap<(Kind, Id), Conflict> = local
        .conflicts()
        .await?
        .into_iter()
        .map(|c| ((c.local.kind, c.local.id), c))
        .collect();
    let mut records = out.records;
    dependency_order(&mut records);
    for record in records {
        if let Some(conflict) = conflicts.get_mut(&(record.kind, record.id)) {
            conflict.remote = record;
            local.set_conflict(conflict).await?;
            continue;
        }
        let entry = local.entry(record.kind, record.id).await?;
        if entry.is_changed() {
            // Pushed next and merged by the server, against the old base.
            if entry
                .local
                .as_ref()
                .is_some_and(|l| l.same_content(&record))
            {
                local.write(None, &record).await?;
            }
            continue;
        }
        if entry
            .local
            .as_ref()
            .is_none_or(|l| !l.same_content(&record))
        {
            report.pulled += 1;
            report.written.push(record.clone());
        }
        local.write(Some(&record), &record).await?;
    }
    for cursor in out.cursors {
        local.set_cursor(cursor).await?;
    }
    // A workspace that never synced is not on the server yet: it is being
    // uploaded, not gone.
    for ws in out.unavailable {
        if local.entry(Kind::Workspace, ws).await?.base.is_some() {
            report.unavailable.push(ws);
        }
    }
    Ok(())
}

async fn push(
    local: &dyn LocalStore,
    remote: &dyn Remote,
    secrets: bool,
    report: &mut Report,
) -> CoreResult<()> {
    let waiting: std::collections::HashSet<(Kind, Id)> = local
        .conflicts()
        .await?
        .iter()
        .map(|c| (c.local.kind, c.local.id))
        .collect();
    let changes: Vec<Change> = local
        .changed()
        .await?
        .into_iter()
        .filter_map(|e| {
            let record = e.current()?;
            (!waiting.contains(&(record.kind, record.id))).then_some(Change {
                record,
                base: e.base,
                force: false,
            })
        })
        .collect();
    if changes.is_empty() {
        return Ok(());
    }
    let sent = changes
        .iter()
        .map(|c| ((c.record.kind, c.record.id), c.record.clone()))
        .collect();
    let out = remote.push(PushInput { changes, secrets }).await?;
    apply_results(local, &sent, out, report).await
}

async fn apply_results(
    local: &dyn LocalStore,
    sent: &HashMap<(Kind, Id), Record>,
    out: PushOutput,
    report: &mut Report,
) -> CoreResult<()> {
    for result in out.results {
        match result {
            PushResult::Applied { record } => {
                report.pushed += 1;
                let sent = sent.get(&(record.kind, record.id));
                if sent.is_none_or(|s| !s.same_content(&record)) {
                    report.written.push(record.clone());
                }
                settle(local, sent, &record).await?;
            }
            PushResult::Conflict { record, current } => {
                local
                    .set_conflict(&Conflict {
                        local: record,
                        remote: current,
                    })
                    .await?;
            }
            PushResult::Rejected { record, code } => {
                // Without edit access the change can never be stored: take
                // the server's version back instead of retrying forever.
                if code == "access.denied"
                    && let Some(base) = local.entry(record.kind, record.id).await?.base
                {
                    local.write(Some(&base), &base).await?;
                    report.written.push(base);
                }
                report.rejected.push(Rejected {
                    kind: record.kind,
                    id: record.id,
                    code,
                });
            }
        }
    }
    Ok(())
}

/// Takes the server's record after a push of `sent`. The local record may
/// have changed meanwhile (the user kept typing): then that change is
/// rebased on the server's record and stays a local change.
async fn settle(local: &dyn LocalStore, sent: Option<&Record>, server: &Record) -> CoreResult<()> {
    let entry = local.entry(server.kind, server.id).await?;
    match entry.current() {
        None => local.write(Some(server), server).await,
        Some(cur) if cur.same_content(server) => local.write(None, server).await,
        Some(cur) if sent.is_none_or(|s| s.same_content(&cur)) => {
            local.write(Some(server), server).await
        }
        Some(cur) => match merge(sent, &cur, server) {
            Merged::Record(m) if m.same_content(server) => local.write(Some(server), server).await,
            Merged::Record(m) => local.write(Some(&m), server).await,
            Merged::Conflict => {
                local
                    .set_conflict(&Conflict {
                        local: cur,
                        remote: server.clone(),
                    })
                    .await
            }
        },
    }
}

/// Settles a conflict as the user chose.
pub async fn resolve(
    local: &dyn LocalStore,
    remote: &dyn Remote,
    kind: Kind,
    id: Id,
    resolution: Resolution,
    secrets: bool,
) -> CoreResult<Report> {
    let mut report = Report::default();
    let Some(conflict) = local
        .conflicts()
        .await?
        .into_iter()
        .find(|c| c.local.kind == kind && c.local.id == id)
    else {
        return Ok(report);
    };
    local.remove_conflict(kind, id).await?;
    match resolution {
        Resolution::TakeRemote => {
            local
                .write(Some(&conflict.remote), &conflict.remote)
                .await?;
            report.pulled = 1;
            report.written.push(conflict.remote);
        }
        Resolution::KeepLocal => {
            let sent = HashMap::from([((kind, id), conflict.local.clone())]);
            let out = remote
                .push(PushInput {
                    changes: vec![Change {
                        record: conflict.local,
                        base: Some(conflict.remote),
                        force: true,
                    }],
                    secrets,
                })
                .await?;
            apply_results(local, &sent, out, &mut report).await?;
        }
    }
    report.conflicts = local.conflicts().await?.len() as u32;
    Ok(report)
}
