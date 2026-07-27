//! Translates each [`Command`] into an HTTP request against `fslite-server`'s
//! actual route table (see `crates/fslite-server/src/routes/*.rs`) and parses
//! the response back into a typed [`CommandOutput`].
//!
//! `fslite-server`'s `GET /content/{*path}` route does not echo a file's
//! [`Revision`] in any response header, so [`Command::Read`] cannot recover
//! it from the content response alone. `fslite-server` already went through
//! its own full review cycle and fix wave before this task started, so
//! rather than reopening it to add a new header, this executor issues an
//! extra `stat` call before every `read` to populate
//! `CommandOutput::Content::revision`, at the cost of one additional round
//! trip per remote read.

use async_trait::async_trait;
use base64::Engine;
use fslite_core::{
    BatchResult, ByteRange, ErrorCode, FsError, FsResult, LinkTarget, Node, Page, PageRequest,
    RequestContext, Revision, SearchMatch, VirtualPath, WorkspaceId,
};
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;

use crate::executor::Executor;
use crate::{Command, CommandOutput};

/// Executes commands against a running `fslite-server` over HTTP.
pub struct RemoteExecutor {
    base_url: String,
    token: String,
    client: Client,
}

impl RemoteExecutor {
    /// Points a new executor at `base_url` (e.g. `http://127.0.0.1:8080`),
    /// authenticating every request with a bearer `token`.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            client: Client::new(),
        }
    }

    fn url(&self, workspace_id: WorkspaceId, suffix: &str) -> String {
        format!("{}/v1/workspaces/{workspace_id}{suffix}", self.base_url)
    }

    /// Attaches the bearer token, sends `builder`, and maps a non-2xx
    /// response into a typed [`FsError`] via [`Self::error_from_response`].
    async fn send_checked(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> FsResult<reqwest::Response> {
        let response = builder
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(map_reqwest_err)?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(Self::error_from_response(response).await)
        }
    }

    /// Sends `builder` and deserializes a successful JSON response body.
    async fn send_json<T: DeserializeOwned>(&self, builder: reqwest::RequestBuilder) -> FsResult<T> {
        let response = self.send_checked(builder).await?;
        response.json::<T>().await.map_err(map_reqwest_err)
    }

    /// Issues a `stat` call directly. Used both for `Command::Stat` and as
    /// the extra round trip `Command::Read` needs to recover `Revision`.
    async fn stat(&self, ctx: &RequestContext, path: &VirtualPath, follow_symlinks: bool) -> FsResult<Node> {
        let builder = self
            .client
            .get(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
            .query(&[("follow_symlinks", follow_symlinks.to_string())]);
        self.send_json(builder).await
    }

    async fn error_from_response(response: reqwest::Response) -> FsError {
        #[derive(serde::Deserialize)]
        struct Envelope {
            error: ErrorBody,
        }
        #[derive(serde::Deserialize)]
        struct ErrorBody {
            code: String,
            message: String,
            details: serde_json::Value,
        }

        let status = response.status();
        match response.json::<Envelope>().await {
            Ok(envelope) => {
                let code = code_from_str(&envelope.error.code);
                FsError::new(code, envelope.error.message, envelope.error.details)
            }
            Err(_) => FsError::internal_storage_failure(format!(
                "unrecognized error response, status {status}"
            )),
        }
    }
}

fn map_reqwest_err(err: reqwest::Error) -> FsError {
    FsError::internal_storage_failure(err.to_string())
}

fn code_from_str(raw: &str) -> ErrorCode {
    // Deserialize through `ErrorCode`'s own `Deserialize` impl (snake_case
    // variant names) rather than hand-maintaining a second name table.
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .unwrap_or(ErrorCode::InternalStorageFailure)
}

fn revision_query(expected_revision: Option<Revision>) -> Vec<(&'static str, String)> {
    match expected_revision {
        Some(revision) => vec![("expected_revision", revision.get().to_string())],
        None => Vec::new(),
    }
}

fn page_query(page: &PageRequest) -> Vec<(&'static str, String)> {
    let mut pairs = vec![("limit", page.limit.to_string())];
    if let Some(cursor) = &page.cursor {
        pairs.push(("cursor", cursor.clone()));
    }
    pairs
}

fn page_body(page: &PageRequest) -> serde_json::Value {
    serde_json::json!({ "cursor": page.cursor, "limit": page.limit })
}

/// Parses a `Content-Range: bytes {start}-{end}/{total}` response header
/// into an inclusive-start, exclusive-end `(start, end)` pair.
fn parse_content_range_bounds(header: &str) -> Option<(u64, u64)> {
    let range = header.strip_prefix("bytes ")?;
    let (range, _total) = range.split_once('/')?;
    let (start, end_inclusive) = range.split_once('-')?;
    let start: u64 = start.parse().ok()?;
    let end_inclusive: u64 = end_inclusive.parse().ok()?;
    Some((start, end_inclusive + 1))
}

/// The wire shape of `GET /fs/{*path}/link-target`'s response body.
#[derive(serde::Deserialize)]
struct LinkTargetWire {
    target: String,
}

/// The wire shape of one item in `POST /search/content`'s response page
/// (mirrors `fslite_server::dto::SearchMatchDto`, which is not exported
/// outside that crate).
#[derive(serde::Deserialize)]
struct SearchMatchWire {
    node: Node,
    path: VirtualPath,
    range: ByteRange,
    preview_base64: String,
}

/// The wire shape of `POST /batch`'s response body.
#[derive(serde::Deserialize)]
struct BatchResponse {
    results: Vec<BatchResult>,
}

#[async_trait]
impl Executor for RemoteExecutor {
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput> {
        match command {
            Command::WorkspaceUsage => {
                let builder = self.client.get(self.url(ctx.workspace_id, "/usage"));
                Ok(CommandOutput::Usage(self.send_json(builder).await?))
            }

            Command::Stat { path, options } => Ok(CommandOutput::Node(
                self.stat(ctx, &path, options.follow_symlinks).await?,
            )),

            Command::Exists { path, options } => {
                let response = self
                    .client
                    .head(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&[("follow_symlinks", options.follow_symlinks.to_string())])
                    .bearer_auth(&self.token)
                    .send()
                    .await
                    .map_err(map_reqwest_err)?;
                match response.status() {
                    StatusCode::NOT_FOUND => Ok(CommandOutput::Exists(false)),
                    status if status.is_success() => Ok(CommandOutput::Exists(true)),
                    // HEAD responses carry no body, so there is no JSON
                    // error envelope to parse here (unlike every other
                    // arm); this constructs a generic error from the status
                    // code alone.
                    status => Err(FsError::internal_storage_failure(format!(
                        "exists check failed with status {status}"
                    ))),
                }
            }

            Command::ReadDir { path, page } => {
                let builder = self
                    .client
                    .get(self.url(
                        ctx.workspace_id,
                        &format!("/directories{}/children", path.as_str()),
                    ))
                    .query(&page_query(&page));
                Ok(CommandOutput::Nodes(self.send_json(builder).await?))
            }

            Command::Tree {
                path,
                options,
                page,
            } => {
                let mut query = page_query(&page);
                if let Some(max_depth) = options.max_depth {
                    query.push(("max_depth", max_depth.to_string()));
                }
                query.push(("follow_symlinks", options.follow_symlinks.to_string()));
                let builder = self
                    .client
                    .get(self.url(
                        ctx.workspace_id,
                        &format!("/directories{}/tree", path.as_str()),
                    ))
                    .query(&query);
                Ok(CommandOutput::Tree(self.send_json(builder).await?))
            }

            Command::Mkdir { path, options } => {
                let body = serde_json::json!({
                    "parents": options.parents,
                    "exist_ok": options.exist_ok,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .put(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&[("type", "directory")])
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Read { path, options } => {
                let node = self.stat(ctx, &path, options.follow_symlinks).await?;
                let mut builder = self.client.get(self.url(
                    ctx.workspace_id,
                    &format!("/content{}", path.as_str()),
                ));
                if let Some(range) = options.range {
                    builder = builder.header(
                        reqwest::header::RANGE,
                        format!("bytes={}-{}", range.start, range.end.saturating_sub(1)),
                    );
                }
                let response = self.send_checked(builder).await?;
                let content_range_bounds = response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_content_range_bounds);
                let bytes = response.bytes().await.map_err(map_reqwest_err)?.to_vec();
                let range = content_range_bounds
                    .map(|(start, end)| ByteRange::new(start, end))
                    .unwrap_or_else(|| ByteRange::new(0, bytes.len() as u64));
                Ok(CommandOutput::Content {
                    logical_length: node.logical_size,
                    revision: node.revision,
                    range,
                    bytes,
                })
            }

            Command::Write {
                path,
                bytes,
                options,
            } => {
                let mut query = vec![("create", options.create.to_string())];
                query.extend(revision_query(options.expected_revision));
                let builder = self
                    .client
                    .put(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .query(&query)
                    .body(bytes);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::WriteAt {
                path,
                offset,
                bytes,
                options,
            } => {
                let mut query = vec![("offset", offset.to_string())];
                query.extend(revision_query(options.expected_revision));
                let builder = self
                    .client
                    .patch(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .query(&query)
                    .body(bytes);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Append {
                path,
                bytes,
                options,
            } => {
                let mut query = vec![("action", "append".to_string())];
                query.extend(revision_query(options.expected_revision));
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .query(&query)
                    .body(bytes);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Truncate {
                path,
                length,
                options,
            } => {
                let body = serde_json::json!({
                    "length": length,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .query(&[("action", "truncate")])
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Touch { path, options } => {
                let body = serde_json::json!({
                    "op": "touch",
                    "create": options.create,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .patch(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Copy { from, to, options } => {
                let body = serde_json::json!({
                    "to": to.as_str(),
                    "recursive": options.recursive,
                    "overwrite": options.overwrite,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/fs{}", from.as_str())))
                    .query(&[("action", "copy")])
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Move { from, to, options } => {
                let body = serde_json::json!({
                    "to": to.as_str(),
                    "overwrite": options.overwrite,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/fs{}", from.as_str())))
                    .query(&[("action", "move")])
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Remove { path, options } => {
                let mut query = vec![("recursive", options.recursive.to_string())];
                query.extend(revision_query(options.expected_revision));
                let builder = self
                    .client
                    .delete(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&query);
                self.send_checked(builder).await?;
                Ok(CommandOutput::Unit)
            }

            Command::Symlink {
                target,
                link,
                options,
            } => {
                let body = serde_json::json!({
                    "target": target.as_str(),
                    "parents": options.parents,
                    "exist_ok": options.exist_ok,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .put(self.url(ctx.workspace_id, &format!("/fs{}", link.as_str())))
                    .query(&[("type", "symlink")])
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::ReadLink { path } => {
                let builder = self.client.get(self.url(
                    ctx.workspace_id,
                    &format!("/fs{}/link-target", path.as_str()),
                ));
                let wire: LinkTargetWire = self.send_json(builder).await?;
                let target = LinkTarget::parse(&wire.target)
                    .map_err(|err| FsError::internal_storage_failure(err.message().to_string()))?;
                Ok(CommandOutput::LinkTarget(target))
            }

            Command::Trash { path, options } => {
                let body = serde_json::json!({
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&[("action", "trash")])
                    .json(&body);
                Ok(CommandOutput::Trash(self.send_json(builder).await?))
            }

            Command::ListTrash { page } => {
                let builder = self
                    .client
                    .get(self.url(ctx.workspace_id, "/trash"))
                    .query(&page_query(&page));
                Ok(CommandOutput::TrashList(self.send_json(builder).await?))
            }

            Command::Restore {
                trash,
                destination,
                options,
            } => {
                let body = serde_json::json!({
                    "destination": destination.as_ref().map(VirtualPath::as_str),
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, &format!("/trash/{trash}/restore")))
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Purge { trash } => {
                let builder = self
                    .client
                    .delete(self.url(ctx.workspace_id, &format!("/trash/{trash}")));
                self.send_checked(builder).await?;
                Ok(CommandOutput::Unit)
            }

            Command::SetAttribute {
                path,
                key,
                value,
                options,
            } => {
                let body = serde_json::json!({
                    "op": "set_attribute",
                    "key": key,
                    "value_base64": base64::engine::general_purpose::STANDARD.encode(&value),
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .patch(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::RemoveAttribute { path, key, options } => {
                let body = serde_json::json!({
                    "op": "remove_attribute",
                    "key": key,
                    "expected_revision": options.expected_revision.map(Revision::get),
                });
                let builder = self
                    .client
                    .patch(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .json(&body);
                Ok(CommandOutput::Node(self.send_json(builder).await?))
            }

            Command::Glob { pattern, page } => {
                let mut query = vec![("pattern", pattern)];
                query.extend(page_query(&page));
                let builder = self
                    .client
                    .get(self.url(ctx.workspace_id, "/search/glob"))
                    .query(&query);
                Ok(CommandOutput::Nodes(self.send_json(builder).await?))
            }

            Command::Find { query, page } => {
                let body = serde_json::json!({ "query": query, "page": page_body(&page) });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, "/search/find"))
                    .json(&body);
                Ok(CommandOutput::Nodes(self.send_json(builder).await?))
            }

            Command::SearchContent { query, page } => {
                let body = serde_json::json!({
                    "root": query.root,
                    "needle_base64": base64::engine::general_purpose::STANDARD.encode(&query.needle),
                    "page": page_body(&page),
                });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, "/search/content"))
                    .json(&body);
                let wire: Page<SearchMatchWire> = self.send_json(builder).await?;
                let items = wire
                    .items
                    .into_iter()
                    .map(|item| -> FsResult<SearchMatch> {
                        let preview = base64::engine::general_purpose::STANDARD
                            .decode(&item.preview_base64)
                            .map_err(|err| FsError::internal_storage_failure(err.to_string()))?;
                        Ok(SearchMatch {
                            node: item.node,
                            path: item.path,
                            range: item.range,
                            preview,
                        })
                    })
                    .collect::<FsResult<Vec<_>>>()?;
                Ok(CommandOutput::SearchMatches(Page::new(
                    items,
                    wire.next_cursor,
                )))
            }

            Command::Changes { after, page } => {
                let mut query = page_query(&page);
                if let Some(after) = &after {
                    query.push(("after", after.as_str().to_string()));
                }
                let builder = self
                    .client
                    .get(self.url(ctx.workspace_id, "/changes"))
                    .query(&query);
                Ok(CommandOutput::Changes(self.send_json(builder).await?))
            }

            Command::Batch(operations) => {
                let body = serde_json::json!({ "operations": operations });
                let builder = self
                    .client
                    .post(self.url(ctx.workspace_id, "/batch"))
                    .json(&body);
                let response: BatchResponse = self.send_json(builder).await?;
                Ok(CommandOutput::Batch(response.results))
            }
        }
    }
}
