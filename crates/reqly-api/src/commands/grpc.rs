//! gRPC: service descriptions and calls (the calls run as connections in
//! [`super::stream`]).

use std::time::Duration;

use reqly_core::model::{FileRef, GrpcRequest, NodeData};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_protocols::grpc::{self, ChannelOptions, ProtoFiles, ServiceInfo};
use reqly_storage::tree::TreeRepo;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AppContext, Platform};
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<GrpcDescribe>();
}

fn grpc_error(code: &str, message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new(code, message.clone()).with_param("message", message)
}

/// `.proto` sources of a request.
async fn proto_files(ctx: &AppContext, request: &GrpcRequest) -> CoreResult<ProtoFiles> {
    let server = ctx.platform == Platform::Server;
    let mut files = ProtoFiles::default();
    for inline in &request.proto.inline {
        files
            .sources
            .push((inline.name.clone(), inline.text.clone()));
    }
    for file in &request.proto.files {
        match file {
            FileRef::Path { path } => {
                if server {
                    return Err(grpc_error(
                        "http.not_allowed",
                        "local files are not available on the server",
                    )
                    .with_param("setting", "proto"));
                }
                let path = std::path::PathBuf::from(path);
                let text = tokio::fs::read_to_string(&path).await.map_err(|e| {
                    grpc_error("grpc.proto_error", format!("{}: {e}", path.display()))
                })?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Some(dir) = path.parent() {
                    files.include_dirs.push(dir.to_path_buf());
                }
                files.sources.push((name, text));
            }
            FileRef::Blob { hash, name, .. } => {
                let bytes = ctx.blobs.read(hash).await?;
                files
                    .sources
                    .push((name.clone(), String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
    }
    if !request.proto.import_paths.is_empty() {
        if server {
            return Err(grpc_error(
                "http.not_allowed",
                "import paths are not available on the server",
            )
            .with_param("setting", "importPaths"));
        }
        files.include_dirs.extend(
            request
                .proto
                .import_paths
                .iter()
                .map(std::path::PathBuf::from),
        );
    }
    Ok(files)
}

/// What a gRPC call needs, prepared from a stored request (templates
/// rendered, folder auth applied).
pub(crate) struct PreparedCall {
    pub target: grpc::Target,
    pub channel_options: ChannelOptions,
    pub metadata: Vec<(String, String)>,
    pub message: String,
    pub deadline: Option<Duration>,
    pub request: GrpcRequest,
}

pub(crate) async fn prepare_call(ctx: &AppContext, request_id: Id) -> CoreResult<PreparedCall> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let node = tree.get_node(request_id).await?;
    let NodeData::Grpc(request) = node.data else {
        return Err(CoreError::new("grpc.not_grpc", "not a gRPC request"));
    };
    let (prepared, _) =
        super::http::prepare_stored(ctx, request_id, &Default::default(), &[]).await?;
    let target = grpc::parse_target(&prepared.url)?;
    let check_url = format!(
        "{}://{}:{}/",
        if target.tls { "https" } else { "http" },
        target.host,
        target.port
    );
    let connect_addr = match &ctx.network_policy {
        Some(policy) => policy.check_target(&check_url).await?,
        None => None,
    };
    let message = match &prepared.body {
        reqly_http::prepare::PreparedBody::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => "{}".into(),
    };
    Ok(PreparedCall {
        channel_options: ChannelOptions {
            tls: super::http::stream_tls(ctx, &check_url, &prepared.settings).await?,
            connect_timeout: Duration::from_millis(
                prepared
                    .settings
                    .connect_timeout_ms
                    .unwrap_or(30_000)
                    .max(1),
            ),
            connect_addr,
        },
        target,
        metadata: prepared
            .headers
            .into_iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type"))
            .collect(),
        message,
        deadline: prepared
            .settings
            .timeout_ms
            .filter(|t| *t > 0)
            .map(Duration::from_millis),
        request,
    })
}

/// Descriptors for a call: reflection over the channel, or the request's files.
pub(crate) async fn descriptor_pool(
    ctx: &AppContext,
    call: &PreparedCall,
    channel: Option<tonic::transport::Channel>,
) -> CoreResult<prost_reflect::DescriptorPool> {
    if call.request.proto.reflection {
        let channel = match channel {
            Some(c) => c,
            None => grpc::connect(&call.target, call.channel_options.clone()).await?,
        };
        grpc::reflect(channel, &call.metadata).await
    } else {
        grpc::compile(&proto_files(ctx, &call.request).await?)
    }
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GrpcDescribeInput {
    pub request_id: Id,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GrpcServiceInfo {
    pub name: String,
    pub methods: Vec<GrpcMethodInfo>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GrpcMethodInfo {
    pub name: String,
    pub path: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    /// A sample request as JSON.
    pub example: String,
}

impl From<ServiceInfo> for GrpcServiceInfo {
    fn from(s: ServiceInfo) -> Self {
        Self {
            name: s.name,
            methods: s
                .methods
                .into_iter()
                .map(|m| GrpcMethodInfo {
                    name: m.name,
                    path: m.path,
                    input_type: m.input_type,
                    output_type: m.output_type,
                    client_streaming: m.client_streaming,
                    server_streaming: m.server_streaming,
                    example: m.example,
                })
                .collect(),
        }
    }
}

pub struct GrpcDescribe;

impl Command<AppContext> for GrpcDescribe {
    const NAME: &'static str = "grpc.describe";
    type Input = GrpcDescribeInput;
    type Output = Vec<GrpcServiceInfo>;

    async fn run(ctx: &AppContext, input: GrpcDescribeInput) -> CoreResult<Vec<GrpcServiceInfo>> {
        let call = prepare_call(ctx, input.request_id).await?;
        let pool = descriptor_pool(ctx, &call, None).await?;
        Ok(grpc::services(&pool).into_iter().map(Into::into).collect())
    }
}
