//! Turns in progress, by chat: how to stop them and the tool calls that
//! wait for the user.

use std::collections::HashMap;
use std::sync::Mutex;

use reqly_core::Id;
use reqly_http::engine::CancelToken;
use tokio::sync::oneshot;

/// The user's answer to a tool call that waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allow this one and every later call in the chat.
    AllowAlways,
    Deny,
}

pub(crate) struct Turn {
    pub cancel: CancelToken,
    /// Tool call id → where the decision goes.
    pub waiting: HashMap<String, oneshot::Sender<Decision>>,
}

#[derive(Default)]
pub struct Turns(Mutex<HashMap<Id, Turn>>);

impl Turns {
    pub fn running(&self, chat: Id) -> bool {
        self.0
            .lock()
            .map(|t| t.contains_key(&chat))
            .unwrap_or(false)
    }

    /// Registers a turn; `None` when the chat already has one.
    pub(crate) fn start(&self, chat: Id) -> Option<CancelToken> {
        let mut turns = self.0.lock().ok()?;
        if turns.contains_key(&chat) {
            return None;
        }
        let cancel = CancelToken::new();
        turns.insert(
            chat,
            Turn {
                cancel: cancel.clone(),
                waiting: HashMap::new(),
            },
        );
        Some(cancel)
    }

    pub(crate) fn finish(&self, chat: Id) {
        if let Ok(mut turns) = self.0.lock() {
            turns.remove(&chat);
        }
    }

    /// Stops the chat's turn; waiting tool calls are denied. False when no
    /// turn runs.
    pub fn cancel(&self, chat: Id) -> bool {
        let Ok(mut turns) = self.0.lock() else {
            return false;
        };
        let Some(turn) = turns.get_mut(&chat) else {
            return false;
        };
        turn.cancel.cancel();
        for (_, tx) in turn.waiting.drain() {
            let _ = tx.send(Decision::Deny);
        }
        true
    }

    /// Waits for the user's decision on a tool call.
    pub(crate) fn wait(&self, chat: Id, call_id: &str) -> Option<oneshot::Receiver<Decision>> {
        let mut turns = self.0.lock().ok()?;
        let turn = turns.get_mut(&chat)?;
        let (tx, rx) = oneshot::channel();
        turn.waiting.insert(call_id.to_string(), tx);
        Some(rx)
    }

    /// Ids of the tool calls that wait, in no particular order.
    pub fn waiting(&self, chat: Id) -> Vec<String> {
        self.0
            .lock()
            .ok()
            .and_then(|t| t.get(&chat).map(|t| t.waiting.keys().cloned().collect()))
            .unwrap_or_default()
    }

    /// Delivers a decision; false when nothing waits for it.
    pub(crate) fn decide(&self, chat: Id, call_id: &str, decision: Decision) -> bool {
        let Ok(mut turns) = self.0.lock() else {
            return false;
        };
        turns
            .get_mut(&chat)
            .and_then(|t| t.waiting.remove(call_id))
            .is_some_and(|tx| tx.send(decision).is_ok())
    }
}
