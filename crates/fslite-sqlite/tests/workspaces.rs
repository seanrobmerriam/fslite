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

