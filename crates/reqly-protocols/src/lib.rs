//! Streaming protocols (stage 5): WebSocket, server-sent events, GraphQL
//! subscriptions over WebSocket and gRPC. Connections are driven by the API
//! layer, which turns their messages into events for the UI.

pub mod graphql_ws;
pub mod grpc;
pub mod sse;
pub mod tls;
pub mod ws;
