//! Domain primitives and services shared by Reqly desktop and server.

pub mod clock;
pub mod error;
pub mod id;
pub mod meta;
pub mod model;
pub mod sort_key;

pub use clock::{Clock, FixedClock, SystemClock, Timestamp};
pub use error::{CoreError, CoreResult};
pub use id::Id;
pub use meta::{Meta, Version};
pub use sort_key::SortKey;
