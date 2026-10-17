//! Domain model: workspaces, collections and the folder/request tree.

mod auth;
mod body;
mod kv;
mod node;
mod request;
mod response;
mod settings;

pub use auth::{ApiKeyLocation, Auth};
pub use body::{Body, BodyKind, FileRef};
pub use kv::{KeyValue, KeyValueKind};
pub use node::{Collection, FolderData, Node, NodeData, NodeKind, NodeSummary, Workspace};
pub use request::HttpRequest;
pub use response::{ResponseRecord, ResponseSummary};
pub use settings::{ProxyMode, ProxySettings, RequestSettings};
