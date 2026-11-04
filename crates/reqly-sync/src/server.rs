//! The server side of a sync round, over any [`ServerStore`].

use async_trait::async_trait;
use reqly_core::{CoreResult, Id};

use crate::merge::{Merged, merge};
use crate::protocol::{Cursor, PullInput, PullOutput, PushInput, PushOutput, PushResult};
use crate::record::{Kind, Record, dependency_order};

#[async_trait]
pub trait ServerStore: Send + Sync {
    /// Records of a workspace changed after `since`, oldest first, with the
    /// sequence of each change.
    async fn changed(&self, workspace_id: Id, since: u64) -> CoreResult<Vec<(u64, Record)>>;
    async fn get(&self, kind: Kind, id: Id) -> CoreResult<Option<Record>>;
    /// Stores a record (creating or replacing it).
    async fn put(&self, record: &Record) -> CoreResult<()>;
}

/// Answers `sync.pull` for workspaces the caller may read (`readable`).
pub async fn pull(
    store: &dyn ServerStore,
    input: &PullInput,
    readable: &(dyn Fn(Id) -> bool + Sync),
) -> CoreResult<PullOutput> {
    let mut out = PullOutput {
        records: Vec::new(),
        cursors: Vec::new(),
        unavailable: Vec::new(),
    };
    for cursor in &input.cursors {
        if !readable(cursor.workspace_id) {
            out.unavailable.push(cursor.workspace_id);
            continue;
        }
        let changed = store.changed(cursor.workspace_id, cursor.seq).await?;
        // Writers are serialized, so nothing older than the newest change
        // read here can still appear.
        let seq = changed.iter().map(|(s, _)| *s).fold(cursor.seq, u64::max);
        out.records.extend(changed.into_iter().map(|(_, r)| r));
        out.cursors.push(Cursor {
            workspace_id: cursor.workspace_id,
            seq,
        });
    }
    Ok(out)
}

/// Answers `sync.push`; `writable` decides per workspace (a new workspace
/// asks for its own id).
pub async fn push(
    store: &dyn ServerStore,
    input: PushInput,
    writable: &(dyn Fn(&Record) -> bool + Sync),
) -> CoreResult<PushOutput> {
    let mut changes = input.changes;
    let mut order: Vec<Record> = changes.iter().map(|c| c.record.clone()).collect();
    dependency_order(&mut order);
    let rank: std::collections::HashMap<(Kind, Id), usize> = order
        .iter()
        .enumerate()
        .map(|(i, r)| ((r.kind, r.id), i))
        .collect();
    changes.sort_by_key(|c| rank.get(&(c.record.kind, c.record.id)).copied());

    let mut results = Vec::new();
    for change in changes {
        let record = change.record;
        if !writable(&record) {
            results.push(PushResult::Rejected {
                record,
                code: "access.denied".into(),
            });
            continue;
        }
        let current = store.get(record.kind, record.id).await?;
        let next = match &current {
            Some(current) if !change.force => match merge(change.base.as_ref(), &record, current) {
                Merged::Record(r) => r,
                Merged::Conflict => {
                    results.push(PushResult::Conflict {
                        record,
                        current: current.clone(),
                    });
                    continue;
                }
            },
            _ => record.clone(),
        };
        if current.as_ref().is_none_or(|c| !c.same_content(&next))
            && let Err(e) = store.put(&next).await
        {
            // Invalid or out of place (its collection is gone): the rest
            // of the push goes on.
            results.push(PushResult::Rejected {
                record,
                code: e.code,
            });
            continue;
        }
        results.push(PushResult::Applied { record: next });
    }
    Ok(PushOutput { results })
}
