//! gRPC client for any service: descriptors from `.proto` sources (compiled
//! with protox) or server reflection, JSON ↔ protobuf with prost-reflect,
//! and all four call types over one streaming code path.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::{Stream, StreamExt};
use hyper_util::rt::TokioIo;
use prost::Message;
use prost_reflect::{
    DescriptorPool, DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, MethodDescriptor,
    SerializeOptions,
};
use reqly_core::{CoreError, CoreResult};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::metadata::{MetadataKey, MetadataMap, MetadataValue};
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{Status, Streaming};

use crate::tls::{TlsOptions, client_config};

fn grpc_error(code: &str, message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new(code, message.clone()).with_param("message", message)
}

// ----- descriptors ----------------------------------------------------------------------

/// `.proto` sources: in-memory files plus include directories.
#[derive(Debug, Clone, Default)]
pub struct ProtoFiles {
    /// (import path, source)
    pub sources: Vec<(String, String)>,
    pub include_dirs: Vec<std::path::PathBuf>,
}

struct MemoryResolver(HashMap<String, String>);

impl protox::file::FileResolver for MemoryResolver {
    fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
        match self.0.get(name) {
            Some(source) => protox::file::File::from_source(name, source),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// Compiles the sources (and what they import) into a descriptor pool.
pub fn compile(files: &ProtoFiles) -> CoreResult<DescriptorPool> {
    if files.sources.is_empty() {
        return Err(grpc_error(
            "grpc.no_proto",
            "add a .proto file or use server reflection",
        ));
    }
    let mut resolver = protox::file::ChainFileResolver::new();
    resolver.add(MemoryResolver(files.sources.iter().cloned().collect()));
    for dir in &files.include_dirs {
        resolver.add(protox::file::IncludeFileResolver::new(dir.clone()));
    }
    resolver.add(protox::file::GoogleFileResolver::new());
    let mut compiler = protox::Compiler::with_file_resolver(resolver);
    compiler.include_imports(true);
    for (name, _) in &files.sources {
        compiler.open_file(name).map_err(|e| {
            grpc_error("grpc.proto_error", e.to_string()).with_param("file", name.clone())
        })?;
    }
    Ok(compiler.descriptor_pool())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodInfo {
    pub name: String,
    /// `/package.Service/Method`
    pub path: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    /// A sample request message as JSON.
    pub example: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

/// Services of a pool (reflection itself left out), sorted by name.
pub fn services(pool: &DescriptorPool) -> Vec<ServiceInfo> {
    let mut out: Vec<ServiceInfo> = pool
        .services()
        .filter(|s| !s.full_name().starts_with("grpc.reflection."))
        .map(|s| ServiceInfo {
            name: s.full_name().to_string(),
            methods: s
                .methods()
                .map(|m| MethodInfo {
                    name: m.name().to_string(),
                    path: format!("/{}/{}", s.full_name(), m.name()),
                    input_type: m.input().full_name().to_string(),
                    output_type: m.output().full_name().to_string(),
                    client_streaming: m.is_client_streaming(),
                    server_streaming: m.is_server_streaming(),
                    example: serde_json::to_string_pretty(&example(&m.input(), 0))
                        .unwrap_or_default(),
                })
                .collect(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn find_method(
    pool: &DescriptorPool,
    service: &str,
    method: &str,
) -> CoreResult<MethodDescriptor> {
    let svc = pool
        .get_service_by_name(service)
        .ok_or_else(|| grpc_error("grpc.unknown_method", format!("no service {service}")))?;
    svc.methods().find(|m| m.name() == method).ok_or_else(|| {
        grpc_error(
            "grpc.unknown_method",
            format!("no method {method} in {service}"),
        )
    })
}

/// A sample JSON value for a message: every field set to something typical.
pub fn example(desc: &MessageDescriptor, depth: usize) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    let mut seen_oneofs = std::collections::HashSet::new();
    for field in desc.fields() {
        if let Some(oneof) = field.containing_oneof() {
            // Only the first member of a oneof.
            if !seen_oneofs.insert(oneof.full_name().to_string()) {
                continue;
            }
        }
        out.insert(field.json_name().to_string(), field_example(&field, depth));
    }
    serde_json::Value::Object(out)
}

fn scalar_example(kind: &Kind, depth: usize) -> serde_json::Value {
    use serde_json::json;
    match kind {
        Kind::Double | Kind::Float => json!(0.0),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 | Kind::Uint32 | Kind::Fixed32 => json!(0),
        // proto3 JSON writes 64-bit integers as strings.
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 | Kind::Uint64 | Kind::Fixed64 => json!("0"),
        Kind::Bool => json!(false),
        Kind::String => json!(""),
        Kind::Bytes => json!(""),
        Kind::Enum(e) => json!(
            e.values()
                .nth(1)
                .or_else(|| e.values().next())
                .map(|v| v.name().to_string())
        ),
        Kind::Message(m) => match m.full_name() {
            "google.protobuf.Timestamp" => json!("1970-01-01T00:00:00Z"),
            "google.protobuf.Duration" => json!("0s"),
            "google.protobuf.Struct" => json!({}),
            "google.protobuf.Value" => json!(null),
            "google.protobuf.ListValue" => json!([]),
            "google.protobuf.Any" => json!({ "@type": "" }),
            "google.protobuf.FieldMask" => json!(""),
            name if name.starts_with("google.protobuf.") && name.ends_with("Value") => {
                let inner = m.get_field(1).map(|f| f.kind()).unwrap_or(Kind::String);
                scalar_example(&inner, depth)
            }
            _ if depth >= 3 => json!({}),
            _ => example(m, depth + 1),
        },
    }
}

fn field_example(field: &FieldDescriptor, depth: usize) -> serde_json::Value {
    if field.is_map() {
        let Kind::Message(entry) = field.kind() else {
            return serde_json::json!({});
        };
        let value = entry.map_entry_value_field();
        let mut map = serde_json::Map::new();
        map.insert("key".into(), scalar_example(&value.kind(), depth));
        return serde_json::Value::Object(map);
    }
    let value = scalar_example(&field.kind(), depth);
    if field.is_list() {
        serde_json::Value::Array(vec![value])
    } else {
        value
    }
}

/// proto3 JSON → message.
pub fn from_json(desc: &MessageDescriptor, json: &str) -> CoreResult<DynamicMessage> {
    let text = if json.trim().is_empty() { "{}" } else { json };
    let mut de = serde_json::Deserializer::from_str(text);
    let message = DynamicMessage::deserialize(desc.clone(), &mut de).map_err(|e| {
        grpc_error("grpc.invalid_message", e.to_string())
            .with_param("type", desc.full_name().to_string())
    })?;
    de.end()
        .map_err(|e| grpc_error("grpc.invalid_message", e.to_string()))?;
    Ok(message)
}

/// Message → proto3 JSON with every field shown.
pub fn to_json(message: &DynamicMessage) -> String {
    let options = SerializeOptions::new()
        .skip_default_fields(false)
        .stringify_64_bit_integers(true);
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::pretty(&mut out);
    match message.serialize_with_options(&mut ser, &options) {
        Ok(()) => String::from_utf8(out).unwrap_or_default(),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

// ----- connection ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

/// `grpcs://host:port` (TLS), `grpc://host:port` or `host:port` (plaintext);
/// `https://`/`http://` work too.
pub fn parse_target(url: &str) -> CoreResult<Target> {
    let url = url.trim();
    let (tls, rest) = if let Some(r) = url
        .strip_prefix("grpcs://")
        .or_else(|| url.strip_prefix("https://"))
    {
        (true, r)
    } else if let Some(r) = url
        .strip_prefix("grpc://")
        .or_else(|| url.strip_prefix("http://"))
    {
        (false, r)
    } else {
        (false, url)
    };
    let authority = rest.split('/').next().unwrap_or("");
    let parsed = url::Url::parse(&format!("http://{authority}")).map_err(|e| {
        grpc_error("http.invalid_url", e.to_string()).with_param("url", url.to_string())
    })?;
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| {
            grpc_error("http.invalid_url", "the address has no host")
                .with_param("url", url.to_string())
        })?
        .trim_matches(|c| c == '[' || c == ']')
        .to_string();
    let port = parsed.port().unwrap_or(if tls { 443 } else { 80 });
    Ok(Target { host, port, tls })
}

trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}

#[derive(Debug, Clone)]
pub struct ChannelOptions {
    pub tls: TlsOptions,
    pub connect_timeout: Duration,
    /// Connect here instead of resolving the host (network policy).
    pub connect_addr: Option<SocketAddr>,
}

pub async fn connect(target: &Target, options: ChannelOptions) -> CoreResult<Channel> {
    let authority = if target.host.contains(':') {
        format!("[{}]:{}", target.host, target.port)
    } else {
        format!("{}:{}", target.host, target.port)
    };
    // The connector below does TLS itself, so tonic sees an `http` endpoint;
    // the origin keeps `:scheme` right for TLS servers.
    let mut endpoint = Endpoint::from_shared(format!("http://{authority}"))
        .map_err(|e| grpc_error("http.invalid_url", e.to_string()))?
        .connect_timeout(options.connect_timeout)
        .tcp_nodelay(true);
    if target.tls {
        let origin: Uri = format!("https://{authority}")
            .parse()
            .map_err(|e: http::uri::InvalidUri| grpc_error("http.invalid_url", e.to_string()))?;
        endpoint = endpoint.origin(origin);
    }
    let tls_config = if target.tls {
        let mut tls = options.tls.clone();
        tls.alpn = vec![b"h2".to_vec()];
        Some(client_config(&tls)?)
    } else {
        None
    };
    let host = target.host.clone();
    let port = target.port;
    let addr = options.connect_addr;
    let connector = tower::service_fn(move |_: Uri| {
        let tls_config = tls_config.clone();
        let host = host.clone();
        async move {
            let tcp = match addr {
                Some(a) => tokio::net::TcpStream::connect(a).await?,
                None => tokio::net::TcpStream::connect((host.as_str(), port)).await?,
            };
            let _ = tcp.set_nodelay(true);
            let io: Box<dyn Io> = match tls_config {
                Some(config) => {
                    let name = rustls::pki_types::ServerName::try_from(host.clone())
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
                    Box::new(
                        tokio_rustls::TlsConnector::from(config)
                            .connect(name, tcp)
                            .await?,
                    )
                }
                None => Box::new(tcp),
            };
            Ok::<_, std::io::Error>(TokioIo::new(io))
        }
    });
    endpoint
        .connect_with_connector(connector)
        .await
        .map_err(|e| {
            let text = error_chain(&e);
            let code =
                if text.contains("certificate") || text.contains("tls") || text.contains("TLS") {
                    "http.tls"
                } else {
                    "http.connect"
                };
            grpc_error(code, text)
        })
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}

// ----- reflection ------------------------------------------------------------------------------

/// Builds a pool from the server's reflection service (v1, then v1alpha).
pub async fn reflect(
    channel: Channel,
    metadata: &[(String, String)],
) -> CoreResult<DescriptorPool> {
    match reflect_v1(channel.clone(), metadata).await {
        Err(e)
            if e.params.get("grpcCode").and_then(|v| v.as_i64())
                == Some(tonic::Code::Unimplemented as i64) =>
        {
            reflect_v1alpha(channel, metadata).await
        }
        other => other,
    }
}

fn status_error(status: &Status) -> CoreError {
    grpc_error(
        "grpc.status",
        format!("{:?}: {}", status.code(), status.message()),
    )
    .with_param("grpcCode", status.code() as i64)
}

macro_rules! reflection_impl {
    ($name:ident, $module:path) => {
        async fn $name(
            channel: Channel,
            metadata: &[(String, String)],
        ) -> CoreResult<DescriptorPool> {
            use pb::server_reflection_request::MessageRequest;
            use pb::server_reflection_response::MessageResponse;
            use $module as pb;
            let mut client = pb::server_reflection_client::ServerReflectionClient::new(channel);
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<pb::ServerReflectionRequest>();
            let request_of = |m: MessageRequest| pb::ServerReflectionRequest {
                host: String::new(),
                message_request: Some(m),
            };
            let _ = tx.send(request_of(MessageRequest::ListServices(String::new())));
            let mut request =
                tonic::Request::new(tokio_stream::wrappers::UnboundedReceiverStream::new(rx));
            apply_metadata(request.metadata_mut(), metadata)?;
            let mut responses = client
                .server_reflection_info(request)
                .await
                .map_err(|s| status_error(&s))?
                .into_inner();
            let mut files: BTreeMap<String, prost_types::FileDescriptorProto> = BTreeMap::new();
            let mut asked: std::collections::HashSet<String> = Default::default();
            let mut pending = 1usize;
            while pending > 0 {
                let Some(response) = responses.message().await.map_err(|s| status_error(&s))?
                else {
                    break;
                };
                pending -= 1;
                match response.message_response {
                    Some(MessageResponse::ListServicesResponse(list)) => {
                        for s in list.service {
                            if s.name.starts_with("grpc.reflection.") {
                                continue;
                            }
                            let _ =
                                tx.send(request_of(MessageRequest::FileContainingSymbol(s.name)));
                            pending += 1;
                        }
                    }
                    Some(MessageResponse::FileDescriptorResponse(fd)) => {
                        for bytes in fd.file_descriptor_proto {
                            let file = prost_types::FileDescriptorProto::decode(bytes.as_slice())
                                .map_err(|e| grpc_error("grpc.reflection", e.to_string()))?;
                            let name = file.name().to_string();
                            asked.insert(name.clone());
                            for dep in &file.dependency {
                                if !files.contains_key(dep) && asked.insert(dep.clone()) {
                                    let _ = tx.send(request_of(MessageRequest::FileByFilename(
                                        dep.clone(),
                                    )));
                                    pending += 1;
                                }
                            }
                            files.insert(name, file);
                        }
                    }
                    Some(MessageResponse::ErrorResponse(e)) => {
                        return Err(grpc_error("grpc.reflection", e.error_message));
                    }
                    _ => {}
                }
            }
            drop(tx);
            let mut pool = DescriptorPool::new();
            pool.add_file_descriptor_protos(files.into_values())
                .map_err(|e| grpc_error("grpc.reflection", e.to_string()))?;
            Ok(pool)
        }
    };
}

reflection_impl!(reflect_v1, tonic_reflection::pb::v1);
reflection_impl!(reflect_v1alpha, tonic_reflection::pb::v1alpha);

// ----- calls -----------------------------------------------------------------------------------

#[derive(Clone)]
struct DynamicCodec(MessageDescriptor);

struct DynEncoder;
struct DynDecoder(MessageDescriptor);

impl Encoder for DynEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: DynamicMessage, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(dst)
            .map_err(|e| Status::internal(e.to_string()))
    }
}

impl Decoder for DynDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        let mut message = DynamicMessage::new(self.0.clone());
        message
            .merge(src)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Some(message))
    }
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynEncoder;
    type Decoder = DynDecoder;

    fn encoder(&mut self) -> DynEncoder {
        DynEncoder
    }

    fn decoder(&mut self) -> DynDecoder {
        DynDecoder(self.0.clone())
    }
}

fn apply_metadata(map: &mut MetadataMap, metadata: &[(String, String)]) -> CoreResult<()> {
    for (k, v) in metadata {
        let key = k.trim().to_ascii_lowercase();
        if key.is_empty() || key == "content-type" || key == "te" || key == "host" {
            continue;
        }
        if key.ends_with("-bin") {
            let key = MetadataKey::from_bytes(key.as_bytes()).map_err(|_| {
                grpc_error("grpc.invalid_metadata", format!("invalid metadata key {k}"))
            })?;
            let bytes = STANDARD
                .decode(v.trim())
                .unwrap_or_else(|_| v.as_bytes().to_vec());
            map.append_bin(key, MetadataValue::from_bytes(&bytes));
        } else {
            let key = MetadataKey::from_bytes(key.as_bytes()).map_err(|_| {
                grpc_error("grpc.invalid_metadata", format!("invalid metadata key {k}"))
            })?;
            let value = v.parse().map_err(|_| {
                grpc_error("grpc.invalid_metadata", format!("invalid value of {k}"))
            })?;
            map.append(key, value);
        }
    }
    Ok(())
}

/// Metadata as text pairs (binary values in base64).
pub fn metadata_pairs(map: &MetadataMap) -> Vec<(String, String)> {
    map.iter()
        .map(|kv| match kv {
            tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                (k.to_string(), v.to_str().unwrap_or("").to_string())
            }
            tonic::metadata::KeyAndValueRef::Binary(k, v) => (
                k.to_string(),
                STANDARD.encode(v.to_bytes().unwrap_or_default()),
            ),
        })
        .collect()
}

pub struct CallResponse {
    /// Response headers (initial metadata).
    pub headers: Vec<(String, String)>,
    pub messages: Streaming<DynamicMessage>,
}

/// Starts a call of any type. `outgoing` ends the client side when it ends.
pub async fn call(
    channel: Channel,
    method: &MethodDescriptor,
    metadata: &[(String, String)],
    outgoing: impl Stream<Item = DynamicMessage> + Send + 'static,
    deadline: Option<Duration>,
) -> Result<CallResponse, Status> {
    let mut grpc = tonic::client::Grpc::new(channel);
    grpc.ready()
        .await
        .map_err(|e| Status::unavailable(error_chain(&e)))?;
    let path = http::uri::PathAndQuery::from_maybe_shared(format!(
        "/{}/{}",
        method.parent_service().full_name(),
        method.name()
    ))
    .map_err(|e| Status::internal(e.to_string()))?;
    let mut request = tonic::Request::new(outgoing.boxed());
    apply_metadata(request.metadata_mut(), metadata)
        .map_err(|e| Status::invalid_argument(e.message))?;
    if let Some(d) = deadline {
        request.set_timeout(d);
    }
    let response = grpc
        .streaming(request, path, DynamicCodec(method.output()))
        .await?;
    let headers = metadata_pairs(response.metadata());
    Ok(CallResponse {
        headers,
        messages: response.into_inner(),
    })
}

/// The status name as in the gRPC spec (`NOT_FOUND`).
pub fn code_name(code: tonic::Code) -> &'static str {
    use tonic::Code::*;
    match code {
        Ok => "OK",
        Cancelled => "CANCELLED",
        Unknown => "UNKNOWN",
        InvalidArgument => "INVALID_ARGUMENT",
        DeadlineExceeded => "DEADLINE_EXCEEDED",
        NotFound => "NOT_FOUND",
        AlreadyExists => "ALREADY_EXISTS",
        PermissionDenied => "PERMISSION_DENIED",
        ResourceExhausted => "RESOURCE_EXHAUSTED",
        FailedPrecondition => "FAILED_PRECONDITION",
        Aborted => "ABORTED",
        OutOfRange => "OUT_OF_RANGE",
        Unimplemented => "UNIMPLEMENTED",
        Internal => "INTERNAL",
        Unavailable => "UNAVAILABLE",
        DataLoss => "DATA_LOSS",
        Unauthenticated => "UNAUTHENTICATED",
    }
}

/// Shared handle for pools compiled once per call.
pub type Pool = Arc<DescriptorPool>;
