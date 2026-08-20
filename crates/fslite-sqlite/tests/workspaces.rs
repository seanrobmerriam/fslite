use fslite_core::{ErrorCode, NodeKind, RequestContext, VirtualPath};
use fslite_sqlite::SqliteFileSystem;

#[tokio::test]
async fn creates_root_and_reopens_workspace() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    drop(fs);

    let reopened = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    let root = reopened
        .stat(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert_eq!(root.kind, NodeKind::Directory);
}

#[tokio::test]
async fn one_workspace_cannot_observe_another_root_child() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let first = fs.create_workspace(Default::default()).await.unwrap();
    let second = fs.create_workspace(Default::default()).await.unwrap();
    let first_ctx = RequestContext::trusted(first.id);
    let second_ctx = RequestContext::trusted(second.id);
    fs.mkdir(
        &first_ctx,
        &VirtualPath::parse("/private").unwrap(),
        Default::default(),
    )
    .await
    .unwrap();
    let error = fs
        .stat(
            &second_ctx,
            &VirtualPath::parse("/private").unwrap(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn reopening_a_database_is_idempotent() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    drop(fs);

    for _ in 0..3 {
        let reopened = SqliteFileSystem::open(file.path(), Default::default())
            .await
            .unwrap();
        let ctx = RequestContext::trusted(workspace.id);
        reopened
            .stat(&ctx, &VirtualPath::root(), Default::default())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn deleted_workspace_is_no_longer_visible() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    fs.delete_workspace(workspace.id).await.unwrap();

    let error = fs
        .stat(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn workspace_usage_reports_active_nodes() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/docs").unwrap(),
        Default::default(),
    )
    .await
    .unwrap();

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.active_nodes, 2);
    assert_eq!(usage.trashed_nodes, 0);
}

#[tokio::test]
async fn reset_workspace_preserves_identity_and_quotas_but_clears_state() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let mut limits = fslite_sqlite::WorkspaceOptions::default();
    limits.max_bytes = 10 * 1024 * 1024;
    limits.max_nodes = 250;
    limits.max_file_bytes = 1024 * 1024;
    let workspace = fs.create_workspace(limits).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    let docs = VirtualPath::parse("/docs").unwrap();
    let file = VirtualPath::parse("/docs/readme.md").unwrap();

    fs.mkdir(&ctx, &docs, Default::default()).await.unwrap();
    fs.write(
        &ctx,
        &file,
        fslite_core::WriteSource::from_bytes(b"hello".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();
    fs.set_attribute(&ctx, &file, "demo", b"yes", Default::default())
        .await
        .unwrap();
    fs.trash(&ctx, &file, Default::default()).await.unwrap();

    fs.reset_workspace(workspace.id).await.unwrap();

    let root = fs
        .stat(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert_eq!(root.kind, NodeKind::Directory);
    let page = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert!(page.items.is_empty());
    assert!(
        fs.list_trash(&ctx, Default::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        fs.changes(&ctx, None, Default::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.workspace_id, workspace.id);
    assert_eq!(usage.active_nodes, 1);
    assert_eq!(usage.trashed_nodes, 0);
    assert_eq!(usage.active_logical_bytes, 0);
    assert_eq!(usage.staged_bytes, 0);
    assert_eq!(usage.max_logical_bytes, limits.max_bytes);
    assert_eq!(usage.max_nodes, limits.max_nodes);
    assert_eq!(usage.max_file_bytes, limits.max_file_bytes);
}

#[tokio::test]
async fn reset_workspace_survives_reopen() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    fs.write(
        &ctx,
        &VirtualPath::parse("/transient.txt").unwrap(),
        fslite_core::WriteSource::from_bytes(b"transient".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    fs.reset_workspace(workspace.id).await.unwrap();
    drop(fs);

    let reopened = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let page = reopened
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn reset_missing_workspace_returns_not_found() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();

    let error = fs
        .reset_workspace(fslite_core::WorkspaceId::new())
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn failed_reset_rolls_back_the_original_workspace() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    let path = VirtualPath::parse("/keep.txt").unwrap();
    fs.write(
        &ctx,
        &path,
        fslite_core::WriteSource::from_bytes(b"keep".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    let raw = rusqlite::Connection::open(file.path()).unwrap();
    raw.execute_batch(
        "CREATE TRIGGER fail_reset_root BEFORE INSERT ON nodes
         WHEN NEW.parent_id IS NULL
         BEGIN SELECT RAISE(ABORT, 'forced reset failure'); END;",
    )
    .unwrap();

    assert!(fs.reset_workspace(workspace.id).await.is_err());
    assert!(fs.exists(&ctx, &path, Default::default()).await.unwrap());
}
