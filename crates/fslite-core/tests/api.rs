use std::collections::BTreeMap;

use async_trait::async_trait;
use fslite_core::{
    BatchOperation, BatchResult, ByteRange, Capability, Change, ChangeCursor, ContentQuery,
    CopyOptions, CreateOptions, DEFAULT_PAGE_LIMIT, FileRead, FileSystem, FindQuery, FsError,
    FsResult, LinkTarget, MoveOptions, MutationOptions, Node, NodeId, NodeKind, Page, PageRequest,
    ReadOptions, RemoveOptions, RequestContext, Revision, SearchMatch, StatOptions, TouchOptions,
    TrashEntry, TrashId, TreeEntry, TreeOptions, VirtualPath, WorkspaceId, WorkspaceUsage,
    WriteOptions, WriteSource,
};

#[derive(Default)]
struct Fake {
    link_target: tokio::sync::Mutex<Option<LinkTarget>>,
}

#[async_trait]
impl FileSystem for Fake {
    async fn workspace_usage(&self, _ctx: &RequestContext) -> FsResult<WorkspaceUsage> {
        unimplemented!()
    }

    async fn stat(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: StatOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn exists(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: StatOptions,
    ) -> FsResult<bool> {
        Ok(false)
    }

    async fn read_dir(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _page: PageRequest,
    ) -> FsResult<Page<Node>> {
        unimplemented!()
    }

    async fn tree(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: TreeOptions,
        _page: PageRequest,
    ) -> FsResult<Page<TreeEntry>> {
        unimplemented!()
    }

    async fn mkdir(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: CreateOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn read(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: ReadOptions,
    ) -> FsResult<FileRead> {
        unimplemented!()
    }

    async fn write(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _source: WriteSource,
        _options: WriteOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn write_at(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _offset: u64,
        _source: WriteSource,
        _options: WriteOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn append(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _source: WriteSource,
        _options: WriteOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn truncate(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _length: u64,
        _options: MutationOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn touch(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: TouchOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn copy(
        &self,
        _ctx: &RequestContext,
        _from: &VirtualPath,
        _to: &VirtualPath,
        _options: CopyOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn move_path(
        &self,
        _ctx: &RequestContext,
        _from: &VirtualPath,
        _to: &VirtualPath,
        _options: MoveOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn remove(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: RemoveOptions,
    ) -> FsResult<()> {
        unimplemented!()
    }

    async fn symlink(
        &self,
        ctx: &RequestContext,
        target: &LinkTarget,
        link: &VirtualPath,
        _options: CreateOptions,
    ) -> FsResult<Node> {
        *self.link_target.lock().await = Some(target.clone());
        Ok(Node {
            workspace_id: ctx.workspace_id,
            id: NodeId::new(),
            parent_id: None,
            name: link.name().unwrap_or_default().to_owned(),
            kind: NodeKind::Symlink,
            logical_size: 0,
            created_at_ms: 0,
            modified_at_ms: 0,
            accessed_at_ms: 0,
            revision: Revision::INITIAL,
            attributes: BTreeMap::new(),
        })
    }

    async fn read_link(&self, _ctx: &RequestContext, path: &VirtualPath) -> FsResult<LinkTarget> {
        self.link_target
            .lock()
            .await
            .clone()
            .ok_or_else(|| FsError::not_found(path))
    }

    async fn trash(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _options: MutationOptions,
    ) -> FsResult<TrashEntry> {
        unimplemented!()
    }

    async fn list_trash(
        &self,
        _ctx: &RequestContext,
        _page: PageRequest,
    ) -> FsResult<Page<TrashEntry>> {
        unimplemented!()
    }

    async fn restore(
        &self,
        _ctx: &RequestContext,
        _trash: TrashId,
        _destination: Option<&VirtualPath>,
        _options: MutationOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn purge(&self, _ctx: &RequestContext, _trash: TrashId) -> FsResult<()> {
        unimplemented!()
    }

    async fn set_attribute(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _key: &str,
        _value: &[u8],
        _options: MutationOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn remove_attribute(
        &self,
        _ctx: &RequestContext,
        _path: &VirtualPath,
        _key: &str,
        _options: MutationOptions,
    ) -> FsResult<Node> {
        unimplemented!()
    }

    async fn glob(
        &self,
        _ctx: &RequestContext,
        _pattern: &str,
        _page: PageRequest,
    ) -> FsResult<Page<Node>> {
        unimplemented!()
    }

    async fn find(
        &self,
        _ctx: &RequestContext,
        _query: FindQuery,
        _page: PageRequest,
    ) -> FsResult<Page<Node>> {
        unimplemented!()
    }

    async fn search_content(
        &self,
        _ctx: &RequestContext,
        _query: ContentQuery,
        _page: PageRequest,
    ) -> FsResult<Page<SearchMatch>> {
        unimplemented!()
    }

    async fn batch(
        &self,
        _ctx: &RequestContext,
        _operations: Vec<BatchOperation>,
    ) -> FsResult<Vec<BatchResult>> {
        unimplemented!()
    }

    async fn changes(
        &self,
        _ctx: &RequestContext,
        _after: Option<ChangeCursor>,
        _page: PageRequest,
    ) -> FsResult<Page<Change>> {
        unimplemented!()
    }
}

async fn accepts_any_filesystem(fs: &dyn FileSystem) {
    let ctx = RequestContext::trusted(WorkspaceId::new());
    let path = VirtualPath::parse("/notes.txt").unwrap();
    let _ = fs.exists(&ctx, &path, Default::default()).await;
}

#[tokio::test]
async fn filesystem_is_object_safe_and_has_the_canonical_method_inventory() {
    accepts_any_filesystem(&Fake::default()).await;
}

#[test]
fn operation_defaults_are_conservative_and_explicit() {
    let create = CreateOptions::default();
    let copy = CopyOptions::default();
    let move_path = MoveOptions::default();
    let mutation = MutationOptions::default();
    let read = ReadOptions::default();
    let remove = RemoveOptions::default();
    let stat = StatOptions::default();
    let touch = TouchOptions::default();
    let tree = TreeOptions::default();
    let write = WriteOptions::default();
    let page = PageRequest::default();

    assert!(
        [
            !create.parents,
            !create.exist_ok,
            create.expected_revision.is_none(),
            !copy.recursive,
            !copy.overwrite,
            copy.expected_revision.is_none(),
            !move_path.overwrite,
            move_path.expected_revision.is_none(),
            mutation.expected_revision.is_none(),
            read.range.is_none(),
            read.follow_symlinks,
            !remove.recursive,
            remove.expected_revision.is_none(),
            stat.follow_symlinks,
            touch.create,
            touch.expected_revision.is_none(),
            tree.max_depth.is_none(),
            !tree.follow_symlinks,
            write.create,
            write.expected_revision.is_none(),
            page.cursor.is_none(),
            page.limit == DEFAULT_PAGE_LIMIT,
        ]
        .into_iter()
        .all(std::convert::identity)
    );
}

#[test]
fn trusted_context_grants_every_capability_explicitly() {
    let ctx = RequestContext::trusted(WorkspaceId::new());

    assert!(
        [
            Capability::Read,
            Capability::Write,
            Capability::Delete,
            Capability::TrashRestore,
            Capability::WorkspaceAdmin,
        ]
        .into_iter()
        .all(|capability| ctx.has_capability(capability))
    );
}

#[test]
fn non_exhaustive_operation_options_are_constructible_with_builders() {
    let revision = Revision::INITIAL;
    let range = ByteRange::new(2, 8);
    let page = PageRequest::default()
        .cursor(Some("next".to_owned()))
        .limit(25);
    let stat = StatOptions::default().follow_symlinks(false);
    let tree = TreeOptions::default()
        .max_depth(Some(3))
        .follow_symlinks(true);
    let create = CreateOptions::default()
        .parents(true)
        .exist_ok(true)
        .expected_revision(Some(revision));
    let read = ReadOptions::default()
        .range(Some(range))
        .follow_symlinks(false);
    let write = WriteOptions::default()
        .create(false)
        .expected_revision(Some(revision));
    let mutation = MutationOptions::default().expected_revision(Some(revision));
    let touch = TouchOptions::default()
        .create(false)
        .expected_revision(Some(revision));
    let copy = CopyOptions::default()
        .recursive(true)
        .overwrite(true)
        .expected_revision(Some(revision));
    let move_path = MoveOptions::default()
        .overwrite(true)
        .expected_revision(Some(revision));
    let remove = RemoveOptions::default()
        .recursive(true)
        .expected_revision(Some(revision));

    assert!(
        [
            page.cursor.as_deref() == Some("next"),
            page.limit == 25,
            !stat.follow_symlinks,
            tree.max_depth == Some(3),
            tree.follow_symlinks,
            create.parents,
            create.exist_ok,
            create.expected_revision == Some(revision),
            read.range == Some(range),
            !read.follow_symlinks,
            !write.create,
            write.expected_revision == Some(revision),
            mutation.expected_revision == Some(revision),
            !touch.create,
            touch.expected_revision == Some(revision),
            copy.recursive,
            copy.overwrite,
            copy.expected_revision == Some(revision),
            move_path.overwrite,
            move_path.expected_revision == Some(revision),
            remove.recursive,
            remove.expected_revision == Some(revision),
        ]
        .into_iter()
        .all(std::convert::identity)
    );
}

#[test]
fn non_exhaustive_queries_are_constructible_with_builders() {
    let root = VirtualPath::parse("/docs").unwrap();
    let attributes = BTreeMap::from([("language".to_owned(), serde_json::json!("rust"))]);
    let find = FindQuery::default()
        .root(root.clone())
        .name_contains(Some("guide".to_owned()))
        .kind(Some(NodeKind::File))
        .min_logical_size(Some(10))
        .max_logical_size(Some(1_000))
        .modified_after_ms(Some(100))
        .modified_before_ms(Some(200))
        .attributes(attributes.clone());
    let content = ContentQuery::default()
        .root(root.clone())
        .needle(b"filesystem".to_vec());

    assert!(
        [
            find.root == root,
            find.name_contains.as_deref() == Some("guide"),
            find.kind == Some(NodeKind::File),
            find.min_logical_size == Some(10),
            find.max_logical_size == Some(1_000),
            find.modified_after_ms == Some(100),
            find.modified_before_ms == Some(200),
            find.attributes == attributes,
            content.root == root,
            content.needle == b"filesystem",
        ]
        .into_iter()
        .all(std::convert::identity)
    );
}

#[test]
fn every_batch_operation_is_reviewed_as_metadata_or_namespace() {
    let path = VirtualPath::parse("/entry").unwrap();
    let destination = VirtualPath::parse("/destination").unwrap();
    let operations = [
        BatchOperation::Mkdir {
            path: path.clone(),
            options: CreateOptions::default(),
        },
        BatchOperation::Touch {
            path: path.clone(),
            options: TouchOptions::default(),
        },
        BatchOperation::Copy {
            from: path.clone(),
            to: destination.clone(),
            options: CopyOptions::default(),
        },
        BatchOperation::Move {
            from: path.clone(),
            to: destination.clone(),
            options: MoveOptions::default(),
        },
        BatchOperation::Remove {
            path: path.clone(),
            options: RemoveOptions::default(),
        },
        BatchOperation::Symlink {
            target: LinkTarget::parse("../target").unwrap(),
            link: path.clone(),
            options: CreateOptions::default(),
        },
        BatchOperation::Trash {
            path: path.clone(),
            options: MutationOptions::default(),
        },
        BatchOperation::Restore {
            trash: TrashId::new(),
            destination: Some(destination),
            options: MutationOptions::default(),
        },
        BatchOperation::Purge {
            trash: TrashId::new(),
        },
        BatchOperation::SetAttribute {
            path: path.clone(),
            key: "key".to_owned(),
            value: b"value".to_vec(),
            options: MutationOptions::default(),
        },
        BatchOperation::RemoveAttribute {
            path,
            key: "key".to_owned(),
            options: MutationOptions::default(),
        },
    ];

    assert!(
        operations
            .iter()
            .all(is_metadata_or_namespace_batch_operation)
    );
}

fn is_metadata_or_namespace_batch_operation(operation: &BatchOperation) -> bool {
    match operation {
        BatchOperation::Mkdir { .. } => true,
        BatchOperation::Touch { .. } => true,
        BatchOperation::Copy { .. } => true,
        BatchOperation::Move { .. } => true,
        BatchOperation::Remove { .. } => true,
        BatchOperation::Symlink { .. } => true,
        BatchOperation::Trash { .. } => true,
        BatchOperation::Restore { .. } => true,
        BatchOperation::Purge { .. } => true,
        BatchOperation::SetAttribute { .. } => true,
        BatchOperation::RemoveAttribute { .. } => true,
    }
}

#[tokio::test]
async fn symlink_and_read_link_round_trip_a_relative_link_target() {
    let fake = Fake::default();
    let fs: &dyn FileSystem = &fake;
    let ctx = RequestContext::trusted(WorkspaceId::new());
    let link = VirtualPath::parse("/docs/current").unwrap();
    let target = LinkTarget::parse("../shared/guide.md").unwrap();

    fs.symlink(&ctx, &target, &link, CreateOptions::default())
        .await
        .unwrap();
    let stored = fs.read_link(&ctx, &link).await.unwrap();

    assert_eq!(stored, target);
}
