//! Import (preview, from URL, apply) and export of collections.

use std::collections::{BTreeSet, HashMap};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqly_core::model::{EnvironmentKind, Node, NodeData, NodeKind, ResponseRecord};
use reqly_core::{CoreError, CoreResult, Id};
use reqly_interop::{
    ExportData, ExportFile, ExportFormat, ExportedResponse, Format, ImportBundle, ImportReport,
    ImportedCollection, ImportedCookie, ImportedEnvironment, ImportedItem, Source,
};
use reqly_storage::cookies::{Cookie, CookieRepo};
use reqly_storage::environments::EnvironmentRepo;
use reqly_storage::history::{HistoryRepo, NewResponse};
use reqly_storage::tree::{TreeInsert, TreeRepo};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::AppContext;
use super::http::CookiesChanged;
use super::template::EnvironmentsChanged;
use super::tree::{collections_changed, tree_changed};
use crate::events::Event;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<ImportPreview>()
        .register::<ImportFromUrl>()
        .register::<ImportApply>()
        .register::<ExportRun>();
}

/// Largest import accepted (all sources together).
const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;

macro_rules! command {
    ($ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportSource {
    /// File name (or URL), used for detection and messages.
    pub name: String,
    pub content: String,
}

fn sources(input: &[ImportSource]) -> CoreResult<Vec<Source>> {
    let total: usize = input.iter().map(|s| s.content.len()).sum();
    if total > MAX_IMPORT_BYTES {
        return Err(
            CoreError::new("import.too_large", "the import is too large")
                .with_param("limitMb", (MAX_IMPORT_BYTES / 1024 / 1024) as u64),
        );
    }
    Ok(input
        .iter()
        .map(|s| Source::new(&s.name, &s.content))
        .collect())
}

// ----- preview -------------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportPreviewInput {
    pub sources: Vec<ImportSource>,
    /// Detected when absent.
    #[ts(optional)]
    pub format: Option<Format>,
}

command!(
    ImportPreview,
    "import.preview",
    ImportPreviewInput,
    ImportBundle,
    |_ctx, input| { reqly_interop::import(&sources(&input.sources)?, input.format) }
);

// ----- from URL ------------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportFromUrlInput {
    pub url: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportFromUrlOutput {
    pub name: String,
    pub content: String,
}

command!(
    ImportFromUrl,
    "import.fromUrl",
    ImportFromUrlInput,
    ImportFromUrlOutput,
    |ctx, input| {
        use reqly_core::model::HttpRequest;
        use reqly_http::engine::{CancelToken, ExecOptions};
        let request = HttpRequest {
            method: "GET".into(),
            url: input.url.clone(),
            ..HttpRequest::default()
        };
        let prepared = reqly_http::prepare::prepare(&request, &[])?;
        let options = ExecOptions {
            max_body_in_memory: MAX_IMPORT_BYTES,
            ..ExecOptions::default()
        };
        let response = match &ctx.network_policy {
            Some(policy) => {
                reqly_http::policy::send_checked(
                    prepared,
                    options,
                    CancelToken::new(),
                    None,
                    policy,
                )
                .await?
            }
            None => {
                reqly_http::engine::execute(prepared, options, CancelToken::new(), None).await?
            }
        };
        if !(200..300).contains(&response.status) {
            return Err(
                CoreError::new("import.download", "the URL did not return the file")
                    .with_param("status", u64::from(response.status)),
            );
        }
        let bytes = response
            .body
            .to_vec()
            .map_err(|e| CoreError::new("import.download", e.to_string()))?;
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err(
                CoreError::new("import.too_large", "the import is too large")
                    .with_param("limitMb", (MAX_IMPORT_BYTES / 1024 / 1024) as u64),
            );
        }
        let name = url::Url::parse(&input.url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|mut s| s.next_back().map(str::to_string))
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| input.url.clone());
        Ok(ImportFromUrlOutput {
            name,
            content: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
);

// ----- apply ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeStrategy {
    /// Add everything next to the existing items.
    Append,
    /// Same-named folders are merged, same-named requests replaced.
    Merge,
    /// Existing items go to the trash first.
    Replace,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
#[ts(export)]
pub enum ImportTarget {
    /// One new collection per imported collection.
    NewCollections,
    /// Everything into an existing collection.
    Collection {
        collection_id: Id,
        strategy: MergeStrategy,
    },
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportApplyInput {
    pub sources: Vec<ImportSource>,
    #[ts(optional)]
    pub format: Option<Format>,
    pub workspace_id: Id,
    pub target: ImportTarget,
    /// Import environments (default true).
    #[ts(optional)]
    pub environments: Option<bool>,
    /// Import cookies (default true).
    #[ts(optional)]
    pub cookies: Option<bool>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportApplyOutput {
    pub collection_ids: Vec<Id>,
    pub requests: u32,
    pub environments: u32,
    pub report: ImportReport,
}

/// Assigns new ids and rewrites references to source ids in templates.
struct IdMap(HashMap<String, Id>);

impl IdMap {
    fn collect(items: &[ImportedItem], into: &mut HashMap<String, Id>) {
        for item in items {
            if let Some(id) = item.id() {
                into.insert(id.to_string(), Id::new());
            }
            Self::collect(item.children(), into);
        }
    }

    fn new(collections: &[ImportedCollection]) -> Self {
        let mut map = HashMap::new();
        for c in collections {
            Self::collect(&c.items, &mut map);
        }
        Self(map)
    }

    fn rewrite_value(&self, value: &mut Value) {
        match value {
            Value::String(s) if s.contains("{%") || s.contains("{{") => {
                for (old, new) in &self.0 {
                    if s.contains(old.as_str()) {
                        for q in ['\'', '"'] {
                            *s = s.replace(&format!("{q}{old}{q}"), &format!("{q}{new}{q}"));
                        }
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| self.rewrite_value(v)),
            Value::Object(map) => map.values_mut().for_each(|v| self.rewrite_value(v)),
            _ => {}
        }
    }

    fn rewrite(&self, data: NodeData) -> NodeData {
        if self.0.is_empty() {
            return data;
        }
        let mut value = serde_json::to_value(&data).unwrap_or(Value::Null);
        self.rewrite_value(&mut value);
        serde_json::from_value(value).unwrap_or(data)
    }

    fn to_insert(&self, item: &ImportedItem) -> TreeInsert {
        TreeInsert {
            id: item.id().and_then(|id| self.0.get(id).copied()),
            name: item.name().to_string(),
            data: self.rewrite(item.node_data()),
            children: item.children().iter().map(|c| self.to_insert(c)).collect(),
        }
    }
}

/// Inserts `items` under `parent`, merging with `existing` nodes by name.
async fn merge_items(
    tree: &TreeRepo<'_>,
    ids: &IdMap,
    collection_id: Id,
    parent: Option<Id>,
    items: &[ImportedItem],
    existing: &[Node],
) -> CoreResult<()> {
    let mut append = Vec::new();
    for item in items {
        let same = existing.iter().find(|n| {
            n.parent_id == parent
                && n.name == item.name()
                && matches!(
                    (n.data.kind(), item),
                    (NodeKind::Folder, ImportedItem::Folder { .. })
                        | (NodeKind::Request, ImportedItem::Request { .. })
                )
        });
        match (same, item) {
            (Some(folder), ImportedItem::Folder { children, .. }) => {
                tree.update_node(
                    folder.meta.id,
                    None,
                    Some(ids.rewrite(item.node_data())),
                    None,
                )
                .await?;
                Box::pin(merge_items(
                    tree,
                    ids,
                    collection_id,
                    Some(folder.meta.id),
                    children,
                    existing,
                ))
                .await?;
            }
            (Some(request), ImportedItem::Request { .. }) => {
                tree.update_node(
                    request.meta.id,
                    None,
                    Some(ids.rewrite(item.node_data())),
                    None,
                )
                .await?;
            }
            _ => append.push(ids.to_insert(item)),
        }
    }
    if !append.is_empty() {
        tree.insert_tree(collection_id, parent, &append).await?;
    }
    Ok(())
}

fn to_cookie(c: &ImportedCookie) -> Cookie {
    Cookie {
        id: Id::new(),
        domain: c.domain.trim_start_matches('.').to_string(),
        include_subdomains: c.include_subdomains,
        path: if c.path.is_empty() {
            "/".into()
        } else {
            c.path.clone()
        },
        secure: c.secure,
        http_only: c.http_only,
        expires: c.expires,
        name: c.name.clone(),
        value: c.value.clone(),
    }
}

async fn add_environment(
    ctx: &AppContext,
    workspace_id: Id,
    collection_id: Option<Id>,
    env: &ImportedEnvironment,
) -> CoreResult<()> {
    let repo = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
    let kind = if collection_id.is_some() {
        EnvironmentKind::Sub
    } else {
        EnvironmentKind::Global
    };
    let name = if env.name.trim().is_empty() {
        "Imported"
    } else {
        env.name.as_str()
    };
    let created = repo.create(workspace_id, collection_id, kind, name).await?;
    repo.update(
        created.meta.id,
        None,
        Some(env.color.as_deref()),
        Some(env.data.clone()),
        Some(env.secrets.clone()),
    )
    .await?;
    Ok(())
}

/// Adds `variables` to the collection's base environment (imported keys win).
async fn merge_base(
    ctx: &AppContext,
    collection_id: Id,
    variables: &Map<String, Value>,
    secrets: &[String],
) -> CoreResult<()> {
    if variables.is_empty() {
        return Ok(());
    }
    let repo = EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref());
    let base = repo
        .list_for_collection(collection_id)
        .await?
        .into_iter()
        .find(|e| e.kind == EnvironmentKind::Base)
        .ok_or_else(|| CoreError::internal("base environment missing"))?;
    let mut data = base.data.clone();
    data.extend(variables.clone());
    let mut all_secrets: BTreeSet<String> = base.secrets.iter().cloned().collect();
    all_secrets.extend(secrets.iter().cloned());
    repo.update(
        base.meta.id,
        None,
        None,
        Some(data),
        Some(all_secrets.into_iter().collect()),
    )
    .await?;
    Ok(())
}

async fn import_history(
    ctx: &AppContext,
    workspace_id: Id,
    ids: &IdMap,
    history: &[ExportedResponse],
) -> CoreResult<()> {
    let repo = HistoryRepo::new(&ctx.storage, ctx.clock.as_ref());
    for h in history {
        let Some(&request_id) = ids.0.get(&h.request_id) else {
            continue;
        };
        let Ok(r) = serde_json::from_value::<ResponseRecord>(h.record.clone()) else {
            continue;
        };
        let body_hash = match &h.body_base64 {
            Some(b64) => {
                let bytes = STANDARD
                    .decode(b64)
                    .map_err(|e| CoreError::new("import.invalid", e.to_string()))?;
                Some(ctx.blobs.put_bytes(&bytes).await?.hash)
            }
            None => None,
        };
        repo.insert(NewResponse {
            request_id,
            workspace_id,
            status: r.status,
            status_text: r.status_text,
            http_version: r.http_version,
            headers: r.headers,
            body_hash,
            body_size: r.body_size,
            headers_size: r.headers_size,
            content_type: r.content_type,
            timings: r.timings,
            timeline: r.timeline,
            request_snapshot: r.request_snapshot,
            redirects: r.redirects,
            final_url: r.final_url,
            remote_addr: r.remote_addr,
            error: r.error,
        })
        .await?;
    }
    Ok(())
}

pub async fn apply_bundle(
    ctx: &AppContext,
    bundle: &ImportBundle,
    workspace_id: Id,
    target: &ImportTarget,
    with_environments: bool,
    with_cookies: bool,
) -> CoreResult<ImportApplyOutput> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    tree.list_collections(workspace_id).await?; // validates the workspace
    let ids = IdMap::new(&bundle.collections);
    let mut collection_ids = Vec::new();
    let mut environments = 0u32;

    match target {
        ImportTarget::NewCollections => {
            for c in &bundle.collections {
                let name = if c.name.trim().is_empty() {
                    "Imported"
                } else {
                    c.name.as_str()
                };
                let col = tree.create_collection(workspace_id, name).await?;
                if c.settings != Default::default() {
                    tree.update_collection_settings(col.meta.id, &c.settings)
                        .await?;
                }
                let inserts: Vec<TreeInsert> = c.items.iter().map(|i| ids.to_insert(i)).collect();
                tree.insert_tree(col.meta.id, None, &inserts).await?;
                if with_environments {
                    merge_base(ctx, col.meta.id, &c.variables, &c.variable_secrets).await?;
                    for env in &c.environments {
                        add_environment(ctx, workspace_id, Some(col.meta.id), env).await?;
                        environments += 1;
                    }
                }
                collection_ids.push(col.meta.id);
            }
            if with_environments {
                for env in &bundle.environments {
                    add_environment(ctx, workspace_id, None, env).await?;
                    environments += 1;
                }
            }
        }
        ImportTarget::Collection {
            collection_id,
            strategy,
        } => {
            let collection_id = *collection_id;
            let col = tree.get_collection(collection_id).await?;
            if col.workspace_id != workspace_id {
                return Err(CoreError::not_found("collection", collection_id));
            }
            let mut existing = tree.list_nodes(collection_id).await?;
            if *strategy == MergeStrategy::Replace {
                let roots: Vec<Id> = existing
                    .iter()
                    .filter(|n| n.parent_id.is_none())
                    .map(|n| n.meta.id)
                    .collect();
                if !roots.is_empty() {
                    tree.delete_nodes(&roots).await?;
                }
                existing.clear();
            }
            for c in &bundle.collections {
                if *strategy == MergeStrategy::Merge {
                    merge_items(&tree, &ids, collection_id, None, &c.items, &existing).await?;
                    existing = tree.list_nodes(collection_id).await?;
                } else {
                    let inserts: Vec<TreeInsert> =
                        c.items.iter().map(|i| ids.to_insert(i)).collect();
                    tree.insert_tree(collection_id, None, &inserts).await?;
                }
                if with_environments {
                    merge_base(ctx, collection_id, &c.variables, &c.variable_secrets).await?;
                    for env in &c.environments {
                        add_environment(ctx, workspace_id, Some(collection_id), env).await?;
                        environments += 1;
                    }
                }
            }
            // Standalone environments (e.g. a Postman environment file) belong to the target.
            if with_environments {
                for env in &bundle.environments {
                    add_environment(ctx, workspace_id, Some(collection_id), env).await?;
                    environments += 1;
                }
            }
            collection_ids.push(collection_id);
        }
    }

    if with_cookies && !bundle.cookies.is_empty() {
        let repo = CookieRepo::new(&ctx.storage, ctx.clock.as_ref());
        for c in &bundle.cookies {
            repo.upsert(workspace_id, &to_cookie(c)).await?;
        }
        ctx.events.publish(Event::new(
            "cookies.changed",
            CookiesChanged { workspace_id },
        ));
    }
    import_history(ctx, workspace_id, &ids, &bundle.history).await?;

    collections_changed(ctx, workspace_id);
    tree_changed(ctx, collection_ids.clone(), Vec::new());
    if environments > 0 {
        ctx.events.publish(Event::new(
            "environments.changed",
            EnvironmentsChanged {
                workspace_id,
                collection_id: None,
            },
        ));
    }
    let requests = bundle
        .collections
        .iter()
        .map(|c| c.request_count())
        .sum::<usize>() as u32;
    Ok(ImportApplyOutput {
        collection_ids,
        requests,
        environments,
        report: bundle.report.clone(),
    })
}

command!(
    ImportApply,
    "import.apply",
    ImportApplyInput,
    ImportApplyOutput,
    |ctx, input| {
        let bundle = reqly_interop::import(&sources(&input.sources)?, input.format)?;
        apply_bundle(
            ctx,
            &bundle,
            input.workspace_id,
            &input.target,
            input.environments.unwrap_or(true),
            input.cookies.unwrap_or(true),
        )
        .await
    }
);

// ----- export --------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
#[ts(export)]
pub enum ExportScope {
    Workspace {
        workspace_id: Id,
    },
    Collection {
        collection_id: Id,
    },
    /// Folders and/or requests of one collection, with their descendants.
    Nodes {
        node_ids: Vec<Id>,
    },
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ExportOptionsInput {
    /// Include environments (default true).
    pub environments: Option<bool>,
    pub secrets: bool,
    pub cookies: bool,
    pub history: bool,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportRunInput {
    pub scope: ExportScope,
    pub format: ExportFormat,
    #[serde(default)]
    #[ts(optional)]
    pub options: Option<ExportOptionsInput>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportRunOutput {
    /// Usually one file; Postman exports add one file per environment.
    pub files: Vec<ExportFile>,
}

fn build_items(
    nodes: &[Node],
    parent: Option<Id>,
    only: Option<&BTreeSet<Id>>,
) -> Vec<ImportedItem> {
    nodes
        .iter()
        .filter(|n| n.parent_id == parent)
        .filter(|n| only.is_none_or(|set| set.contains(&n.meta.id)))
        .map(|n| {
            match &n.data {
                NodeData::Folder(f) => ImportedItem::folder(
                    n.name.clone(),
                    f.clone(),
                    build_items(nodes, Some(n.meta.id), None),
                ),
                NodeData::Request(r) => ImportedItem::request(n.name.clone(), r.clone()),
            }
            .with_id(n.meta.id.to_string())
        })
        .collect()
}

async fn collection_data(
    ctx: &AppContext,
    collection_id: Id,
    only: Option<&[Id]>,
    with_environments: bool,
) -> CoreResult<(ImportedCollection, Vec<Node>)> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let col = tree.get_collection(collection_id).await?;
    let nodes = tree.list_nodes(collection_id).await?;
    let items = match only {
        None => build_items(&nodes, None, None),
        Some(ids) => {
            // Selected nodes become top-level items (descendants of a selected
            // folder are included through it).
            let selected: BTreeSet<Id> = ids.iter().copied().collect();
            let mut roots = Vec::new();
            for node in nodes.iter().filter(|n| selected.contains(&n.meta.id)) {
                let mut p = node.parent_id;
                let mut covered = false;
                while let Some(pid) = p {
                    if selected.contains(&pid) {
                        covered = true;
                        break;
                    }
                    p = nodes
                        .iter()
                        .find(|n| n.meta.id == pid)
                        .and_then(|n| n.parent_id);
                }
                if !covered {
                    roots.extend(build_items(
                        &nodes,
                        node.parent_id,
                        Some(&BTreeSet::from([node.meta.id])),
                    ));
                }
            }
            roots
        }
    };
    let mut out = ImportedCollection {
        name: col.name,
        settings: col.settings,
        items,
        ..ImportedCollection::default()
    };
    if with_environments {
        for env in EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref())
            .list_for_collection(collection_id)
            .await?
        {
            if env.kind == EnvironmentKind::Base {
                out.variables = env.data;
                out.variable_secrets = env.secrets;
            } else {
                out.environments.push(ImportedEnvironment {
                    name: env.name,
                    color: env.color,
                    data: env.data,
                    secrets: env.secrets,
                });
            }
        }
    }
    Ok((out, nodes))
}

fn request_ids(items: &[ImportedItem], out: &mut Vec<String>) {
    for i in items {
        if let ImportedItem::Request { id: Some(id), .. } = i {
            out.push(id.clone());
        }
        request_ids(i.children(), out);
    }
}

pub async fn export_data(
    ctx: &AppContext,
    scope: &ExportScope,
    options: &ExportOptionsInput,
) -> CoreResult<ExportData> {
    let tree = TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
    let with_envs = options.environments.unwrap_or(true);
    let (workspace_id, collections) = match scope {
        ExportScope::Workspace { workspace_id } => {
            let mut out = Vec::new();
            for c in tree.list_collections(*workspace_id).await? {
                out.push(collection_data(ctx, c.meta.id, None, with_envs).await?.0);
            }
            (*workspace_id, out)
        }
        ExportScope::Collection { collection_id } => {
            let col = tree.get_collection(*collection_id).await?;
            (
                col.workspace_id,
                vec![
                    collection_data(ctx, *collection_id, None, with_envs)
                        .await?
                        .0,
                ],
            )
        }
        ExportScope::Nodes { node_ids } => {
            let first = node_ids
                .first()
                .ok_or_else(|| CoreError::new("export.empty", "nothing selected"))?;
            let node = tree.get_node(*first).await?;
            let col = tree.get_collection(node.collection_id).await?;
            let (data, _) =
                collection_data(ctx, node.collection_id, Some(node_ids), with_envs).await?;
            (col.workspace_id, vec![data])
        }
    };
    let workspace = tree
        .list_workspaces()
        .await?
        .into_iter()
        .find(|w| w.meta.id == workspace_id)
        .map(|w| w.name)
        .unwrap_or_default();
    let mut data = ExportData {
        workspace,
        collections,
        ..ExportData::default()
    };
    if with_envs && matches!(scope, ExportScope::Workspace { .. }) {
        for env in EnvironmentRepo::new(&ctx.storage, ctx.clock.as_ref())
            .list_global(workspace_id)
            .await?
        {
            data.environments.push(ImportedEnvironment {
                name: env.name,
                color: env.color,
                data: env.data,
                secrets: env.secrets,
            });
        }
    }
    if options.cookies {
        data.cookies = CookieRepo::new(&ctx.storage, ctx.clock.as_ref())
            .list(workspace_id)
            .await?
            .into_iter()
            .map(|c| ImportedCookie {
                domain: c.domain,
                include_subdomains: c.include_subdomains,
                path: c.path,
                secure: c.secure,
                http_only: c.http_only,
                expires: c.expires,
                name: c.name,
                value: c.value,
            })
            .collect();
    }
    if options.history {
        let history = HistoryRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut ids = Vec::new();
        for c in &data.collections {
            request_ids(&c.items, &mut ids);
        }
        for id in ids {
            let Ok(request_id) = id.parse::<Id>() else {
                continue;
            };
            for summary in history.list_for_request(request_id, 1000).await? {
                let record = history.get(summary.id).await?;
                let body_base64 = match &record.body_hash {
                    Some(hash) => ctx.blobs.read(hash).await.ok().map(|b| STANDARD.encode(b)),
                    None => None,
                };
                data.history.push(ExportedResponse {
                    request_id: id.clone(),
                    record: serde_json::to_value(&record).unwrap_or(Value::Null),
                    body_base64,
                });
            }
        }
    }
    if !options.secrets {
        data.strip_secrets();
    }
    Ok(data)
}

command!(
    ExportRun,
    "export.run",
    ExportRunInput,
    ExportRunOutput,
    |ctx, input| {
        let options = input.options.unwrap_or_default();
        let data = export_data(ctx, &input.scope, &options).await?;
        let stem = match &input.scope {
            ExportScope::Workspace { .. } => data.workspace.clone(),
            _ => data
                .collections
                .first()
                .map(|c| c.name.clone())
                .unwrap_or_default(),
        };
        let source = format!("reqly {}", env!("CARGO_PKG_VERSION"));
        let files = reqly_interop::export(&data, input.format, ctx.clock.now().0, &source, &stem)?;
        Ok(ExportRunOutput { files })
    }
);
