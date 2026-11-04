//! In-memory stores, for tests and simulations.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use reqly_core::{CoreResult, Id};

use crate::engine::{Conflict, Entry, LocalStore, Remote};
use crate::protocol::{Cursor, PullInput, PullOutput, PushInput, PushOutput};
use crate::record::{Kind, Record};
use crate::server::{self, ServerStore};

#[derive(Default)]
struct ServerState {
    seq: u64,
    records: HashMap<(Kind, Id), (u64, Record)>,
}

/// A server that keeps records in memory and answers in process.
#[derive(Default)]
pub struct MemoryServer {
    state: Mutex<ServerState>,
}

impl MemoryServer {
    /// Every record of a workspace.
    pub fn records(&self, workspace_id: Id) -> Vec<Record> {
        let state = self.state.lock().unwrap();
        let mut out: Vec<Record> = state
            .records
            .values()
            .filter(|(_, r)| r.workspace_id == workspace_id)
            .map(|(_, r)| r.clone())
            .collect();
        out.sort_by_key(|r| (r.kind, r.id));
        out
    }
}

#[async_trait]
impl ServerStore for MemoryServer {
    async fn changed(&self, workspace_id: Id, since: u64) -> CoreResult<Vec<(u64, Record)>> {
        let state = self.state.lock().unwrap();
        let mut out: Vec<(u64, Record)> = state
            .records
            .values()
            .filter(|(s, r)| *s > since && r.workspace_id == workspace_id)
            .cloned()
            .collect();
        out.sort_by_key(|(s, _)| *s);
        Ok(out)
    }

    async fn get(&self, kind: Kind, id: Id) -> CoreResult<Option<Record>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .records
            .get(&(kind, id))
            .map(|(_, r)| r.clone()))
    }

    async fn put(&self, record: &Record) -> CoreResult<()> {
        let mut state = self.state.lock().unwrap();
        state.seq += 1;
        let seq = state.seq;
        state
            .records
            .insert((record.kind, record.id), (seq, record.clone()));
        Ok(())
    }
}

#[async_trait]
impl Remote for MemoryServer {
    async fn pull(&self, input: PullInput) -> CoreResult<PullOutput> {
        server::pull(self, &input, &|_| true).await
    }

    async fn push(&self, input: PushInput) -> CoreResult<PushOutput> {
        server::push(self, input, &|_| true).await
    }
}

#[derive(Default)]
struct ClientState {
    records: HashMap<(Kind, Id), Record>,
    bases: HashMap<(Kind, Id), Record>,
    cursors: HashMap<Id, u64>,
    conflicts: HashMap<(Kind, Id), Conflict>,
}

/// A client that keeps records in memory.
#[derive(Default)]
pub struct MemoryClient {
    state: Mutex<ClientState>,
}

impl MemoryClient {
    /// Starts syncing a workspace.
    pub fn follow(&self, workspace_id: Id) {
        self.state
            .lock()
            .unwrap()
            .cursors
            .entry(workspace_id)
            .or_insert(0);
    }

    /// A local edit.
    pub fn put(&self, record: Record) {
        self.state
            .lock()
            .unwrap()
            .records
            .insert((record.kind, record.id), record);
    }

    /// Drops a row, as purging the trash does.
    pub fn purge(&self, kind: Kind, id: Id) {
        self.state.lock().unwrap().records.remove(&(kind, id));
    }

    pub fn get(&self, kind: Kind, id: Id) -> Option<Record> {
        self.state.lock().unwrap().records.get(&(kind, id)).cloned()
    }

    pub fn records(&self, workspace_id: Id) -> Vec<Record> {
        let state = self.state.lock().unwrap();
        let mut out: Vec<Record> = state
            .records
            .values()
            .filter(|r| r.workspace_id == workspace_id)
            .cloned()
            .collect();
        out.sort_by_key(|r| (r.kind, r.id));
        out
    }
}

#[async_trait]
impl LocalStore for MemoryClient {
    async fn cursors(&self) -> CoreResult<Vec<Cursor>> {
        let mut out: Vec<Cursor> = self
            .state
            .lock()
            .unwrap()
            .cursors
            .iter()
            .map(|(&workspace_id, &seq)| Cursor { workspace_id, seq })
            .collect();
        out.sort_by_key(|c| c.workspace_id);
        Ok(out)
    }

    async fn set_cursor(&self, cursor: Cursor) -> CoreResult<()> {
        self.state
            .lock()
            .unwrap()
            .cursors
            .insert(cursor.workspace_id, cursor.seq);
        Ok(())
    }

    async fn entry(&self, kind: Kind, id: Id) -> CoreResult<Entry> {
        let state = self.state.lock().unwrap();
        Ok(Entry {
            local: state.records.get(&(kind, id)).cloned(),
            base: state.bases.get(&(kind, id)).cloned(),
        })
    }

    async fn changed(&self) -> CoreResult<Vec<Entry>> {
        let state = self.state.lock().unwrap();
        let mut keys: Vec<(Kind, Id)> = state
            .records
            .iter()
            .filter(|(_, r)| state.cursors.contains_key(&r.workspace_id))
            .map(|(k, _)| *k)
            .chain(state.bases.keys().copied())
            .collect();
        keys.sort();
        keys.dedup();
        Ok(keys
            .into_iter()
            .map(|k| Entry {
                local: state.records.get(&k).cloned(),
                base: state.bases.get(&k).cloned(),
            })
            .filter(Entry::is_changed)
            .collect())
    }

    async fn write(&self, local: Option<&Record>, base: &Record) -> CoreResult<()> {
        let mut state = self.state.lock().unwrap();
        let key = (base.kind, base.id);
        if let Some(local) = local {
            state.records.insert(key, local.clone());
        }
        state.bases.insert(key, base.clone());
        Ok(())
    }

    async fn conflicts(&self) -> CoreResult<Vec<Conflict>> {
        let mut out: Vec<Conflict> = self
            .state
            .lock()
            .unwrap()
            .conflicts
            .values()
            .cloned()
            .collect();
        out.sort_by_key(|c| (c.local.kind, c.local.id));
        Ok(out)
    }

    async fn set_conflict(&self, conflict: &Conflict) -> CoreResult<()> {
        self.state
            .lock()
            .unwrap()
            .conflicts
            .insert((conflict.local.kind, conflict.local.id), conflict.clone());
        Ok(())
    }

    async fn remove_conflict(&self, kind: Kind, id: Id) -> CoreResult<()> {
        self.state.lock().unwrap().conflicts.remove(&(kind, id));
        Ok(())
    }
}
