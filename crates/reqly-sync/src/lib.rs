//! Desktop ↔ server sync (ADR-013): records, three-way merges, the server
//! side of `sync.pull` / `sync.push` and the client's sync round.

pub mod engine;
pub mod memory;
pub mod merge;
pub mod protocol;
pub mod record;
pub mod server;
pub mod text;

pub use engine::{Conflict, Entry, LocalStore, Remote, Report, Resolution, resolve, round};
pub use merge::{Merged, merge};
pub use protocol::{Change, Cursor, PullInput, PullOutput, PushInput, PushOutput, PushResult};
pub use record::{Kind, Record};
pub use server::ServerStore;
