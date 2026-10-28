//! Domain model: workspaces, collections and the folder/request tree.

mod assertion;
mod auth;
mod body;
mod environment;
mod kv;
mod node;
mod protocols;
mod request;
mod response;
mod settings;

pub use assertion::{Assertion, AssertionOperator, AssertionSource};
pub use auth::{
    ApiKeyLocation, Auth, HawkAlgorithm, JwtLocation, OAuth1Location, OAuth1Method,
    OAuth2ClientAuth, OAuth2Config, OAuth2Grant,
};
pub use body::{Body, BodyKind, FileRef};
pub use environment::{Environment, EnvironmentKind};
pub use kv::{KeyValue, KeyValueKind};
pub use node::{Collection, FolderData, Node, NodeData, NodeKind, NodeSummary, Workspace};
pub use protocols::{
    GraphqlBody, GrpcRequest, InlineProto, MessageFormat, MessageTemplate, ProtoSource, Protocol,
    WebSocketRequest,
};
pub use request::{HttpRequest, Scripts};
pub use response::{ResponseRecord, ResponseSummary};
pub use settings::{HttpVersion, ProxyMode, ProxySettings, RequestSettings, TlsVersion};
