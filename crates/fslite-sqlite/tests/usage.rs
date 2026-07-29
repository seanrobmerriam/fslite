use fslite_core::{MutationOptions, RequestContext, VirtualPath, WriteOptions, WriteSource};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

#[tokio::test]
async fn usage_reflects_replacement() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/f"),
        WriteSource::from_bytes(vec![0u8; 1000]),
        Default::default(),
    )
    .await
    .unwrap();
    let after_first = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(after_first.active_logical_bytes, 1000);

    fs.write(
        &ctx,
        &path("/f"),
        WriteSource::from_bytes(vec![0u8; 250]),
        WriteOptions::replace(),
    )
    .await
    .unwrap();
    let after_replace = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(after_replace.active_logical_bytes, 250);
}

#[tokio::test]
async fn usage_reflects_purge() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/f"),
        WriteSource::from_bytes(vec![0u8; 512]),
        Default::default(),
    )
    .await
    .unwrap();

    let entry = fs
        .trash(&ctx, &path("/f"), MutationOptions::default())
        .await
        .unwrap();
    let while_trashed = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(while_trashed.active_logical_bytes, 0);
    assert_eq!(while_trashed.trashed_logical_bytes, 512);

    fs.purge(&ctx, entry.id).await.unwrap();
    let after_purge = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(after_purge.active_logical_bytes, 0);
    assert_eq!(after_purge.trashed_logical_bytes, 0);
}

#[tokio::test]
async fn usage_counts_nodes_including_root() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.active_nodes, 1);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/b"), Default::default())
        .await
        .unwrap();

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.active_nodes, 3);
}

#[tokio::test]
async fn usage_respects_configured_quotas() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let mut options = fslite_sqlite::WorkspaceOptions::default();
    options.max_bytes = 4096;
    options.max_nodes = 10;
    options.max_file_bytes = 1024;
    let workspace = fs.create_workspace(options).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.max_logical_bytes, 4096);
    assert_eq!(usage.max_nodes, 10);
    assert_eq!(usage.max_file_bytes, 1024);
}
