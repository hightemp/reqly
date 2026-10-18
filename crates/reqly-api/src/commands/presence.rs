//! Who is in a workspace and which request they have open (multi-user
//! server). Each browser tab reports itself with `presence.set`; the server
//! forgets it when the tab's WebSocket closes.

use std::collections::HashMap;
use std::sync::Mutex;

use reqly_core::{CoreResult, Id};
use reqly_storage::users::UserRepo;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AppContext;
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<PresenceSet>();
}

struct Entry {
    user: Id,
    name: String,
    workspace: Id,
    node: Option<Id>,
}

#[derive(Default)]
pub struct Presence {
    clients: Mutex<HashMap<String, Entry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresencePerson {
    pub client_id: String,
    pub user_id: Id,
    pub name: String,
    /// The request or folder open in the tab.
    pub node_id: Option<Id>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresenceChanged {
    pub workspace_id: Id,
    pub people: Vec<PresencePerson>,
}

impl Presence {
    fn people(clients: &HashMap<String, Entry>, workspace: Id) -> Vec<PresencePerson> {
        let mut people: Vec<PresencePerson> = clients
            .iter()
            .filter(|(_, e)| e.workspace == workspace)
            .map(|(client, e)| PresencePerson {
                client_id: client.clone(),
                user_id: e.user,
                name: e.name.clone(),
                node_id: e.node,
            })
            .collect();
        people.sort_by(|a, b| a.name.cmp(&b.name).then(a.client_id.cmp(&b.client_id)));
        people
    }

    fn publish(ctx: &AppContext, workspace_id: Id, people: Vec<PresencePerson>) {
        ctx.events.publish(Event::new(
            "presence.changed",
            PresenceChanged {
                workspace_id,
                people,
            },
        ));
    }

    /// Forgets a tab (its WebSocket closed) and tells the others.
    pub fn leave(&self, ctx: &AppContext, client_id: &str) {
        let Ok(mut clients) = self.clients.lock() else {
            return;
        };
        if let Some(entry) = clients.remove(client_id) {
            let people = Self::people(&clients, entry.workspace);
            drop(clients);
            Self::publish(ctx, entry.workspace, people);
        }
    }
}

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresenceSetInput {
    /// The browser tab, as in the WebSocket URL (`?client=`).
    pub client_id: String,
    pub workspace_id: Id,
    #[serde(default)]
    #[ts(optional)]
    pub node_id: Option<Id>,
}

command!(
    /// Reports where this tab is; returns who else is in the workspace.
    PresenceSet,
    "presence.set",
    PresenceSetInput,
    Vec<PresencePerson>,
    |ctx, input| {
        let Some(user) = crate::access::current().user_id() else {
            return Ok(Vec::new());
        };
        if input.client_id.is_empty() || input.client_id.len() > 64 {
            return Err(reqly_core::CoreError::new("command.invalid_input", "bad client id"));
        }
        let name = UserRepo::new(&ctx.storage, ctx.clock.as_ref())
            .get(user)
            .await?
            .display_name;
        let (people, left) = {
            let mut clients = ctx
                .presence
                .clients
                .lock()
                .map_err(|_| reqly_core::CoreError::internal("presence lock"))?;
            if clients.get(&input.client_id).is_some_and(|e| e.user != user) {
                return Err(reqly_core::CoreError::new(
                    "command.invalid_input",
                    "client id in use",
                ));
            }
            let previous = clients.insert(
                input.client_id,
                Entry {
                    user,
                    name,
                    workspace: input.workspace_id,
                    node: input.node_id,
                },
            );
            let left = previous
                .map(|p| p.workspace)
                .filter(|ws| *ws != input.workspace_id)
                .map(|ws| (ws, Presence::people(&clients, ws)));
            (Presence::people(&clients, input.workspace_id), left)
        };
        if let Some((ws, others)) = left {
            Presence::publish(ctx, ws, others);
        }
        Presence::publish(ctx, input.workspace_id, people.clone());
        Ok(people)
    }
);
