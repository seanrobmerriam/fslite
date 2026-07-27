use std::sync::Arc;

use async_trait::async_trait;
use fslite_core::{FileSystem, FsResult, RequestContext};
use futures::StreamExt;

use crate::executor::Executor;
use crate::{Command, CommandOutput};

/// Executes commands directly against an in-process `FileSystem` backend.
pub struct LocalExecutor {
    fs: Arc<dyn FileSystem>,
}

impl LocalExecutor {
    /// Wraps a backend for local, in-process execution.
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self { fs }
    }
}

async fn drain(stream: fslite_core::ByteStream) -> FsResult<Vec<u8>> {
    let mut stream = stream;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput> {
        Ok(match command {
            Command::WorkspaceUsage => CommandOutput::Usage(self.fs.workspace_usage(ctx).await?),
            Command::Stat { path, options } => {
                CommandOutput::Node(self.fs.stat(ctx, &path, options).await?)
            }
            Command::Exists { path, options } => {
                CommandOutput::Exists(self.fs.exists(ctx, &path, options).await?)
            }
            Command::ReadDir { path, page } => {
                CommandOutput::Nodes(self.fs.read_dir(ctx, &path, page).await?)
            }
            Command::Tree {
                path,
                options,
                page,
            } => CommandOutput::Tree(self.fs.tree(ctx, &path, options, page).await?),
            Command::Mkdir { path, options } => {
                CommandOutput::Node(self.fs.mkdir(ctx, &path, options).await?)
            }
            Command::Read { path, options } => {
                let file = self.fs.read(ctx, &path, options).await?;
                let logical_length = file.logical_length;
                let revision = file.revision;
                let range = file.range;
                let bytes = drain(file.into_stream()).await?;
                CommandOutput::Content {
                    logical_length,
                    revision,
                    range,
                    bytes,
                }
            }
            Command::Write {
                path,
                bytes,
                options,
            } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(self.fs.write(ctx, &path, source, options).await?)
            }
            Command::WriteAt {
                path,
                offset,
                bytes,
                options,
            } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(
                    self.fs
                        .write_at(ctx, &path, offset, source, options)
                        .await?,
                )
            }
            Command::Append {
                path,
                bytes,
                options,
            } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(self.fs.append(ctx, &path, source, options).await?)
            }
            Command::Truncate {
                path,
                length,
                options,
            } => CommandOutput::Node(self.fs.truncate(ctx, &path, length, options).await?),
            Command::Touch { path, options } => {
                CommandOutput::Node(self.fs.touch(ctx, &path, options).await?)
            }
            Command::Copy { from, to, options } => {
                CommandOutput::Node(self.fs.copy(ctx, &from, &to, options).await?)
            }
            Command::Move { from, to, options } => {
                CommandOutput::Node(self.fs.move_path(ctx, &from, &to, options).await?)
            }
            Command::Remove { path, options } => {
                self.fs.remove(ctx, &path, options).await?;
                CommandOutput::Unit
            }
            Command::Symlink {
                target,
                link,
                options,
            } => CommandOutput::Node(self.fs.symlink(ctx, &target, &link, options).await?),
            Command::ReadLink { path } => {
                CommandOutput::LinkTarget(self.fs.read_link(ctx, &path).await?)
            }
            Command::Trash { path, options } => {
                CommandOutput::Trash(self.fs.trash(ctx, &path, options).await?)
            }
            Command::ListTrash { page } => {
                CommandOutput::TrashList(self.fs.list_trash(ctx, page).await?)
            }
            Command::Restore {
                trash,
                destination,
                options,
            } => CommandOutput::Node(
                self.fs
                    .restore(ctx, trash, destination.as_ref(), options)
                    .await?,
            ),
            Command::Purge { trash } => {
                self.fs.purge(ctx, trash).await?;
                CommandOutput::Unit
            }
            Command::SetAttribute {
                path,
                key,
                value,
                options,
            } => CommandOutput::Node(
                self.fs
                    .set_attribute(ctx, &path, &key, &value, options)
                    .await?,
            ),
            Command::RemoveAttribute { path, key, options } => {
                CommandOutput::Node(self.fs.remove_attribute(ctx, &path, &key, options).await?)
            }
            Command::Glob { pattern, page } => {
                CommandOutput::Nodes(self.fs.glob(ctx, &pattern, page).await?)
            }
            Command::Find { query, page } => {
                CommandOutput::Nodes(self.fs.find(ctx, query, page).await?)
            }
            Command::SearchContent { query, page } => {
                CommandOutput::SearchMatches(self.fs.search_content(ctx, query, page).await?)
            }
            Command::Changes { after, page } => {
                CommandOutput::Changes(self.fs.changes(ctx, after, page).await?)
            }
            Command::Batch(operations) => {
                CommandOutput::Batch(self.fs.batch(ctx, operations).await?)
            }
        })
    }
}
