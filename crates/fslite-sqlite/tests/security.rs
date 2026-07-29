use fslite_core::{Capability, ErrorCode, PageRequest, RequestContext, VirtualPath};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn read_only_ctx(workspace_id: fslite_core::WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, Default::default(), [Capability::Read])
}

#[tokio::test]
async fn mkdir_requires_write_capability() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = read_only_ctx(workspace.id);

    let error = fs
        .mkdir(&ctx, &path("/blocked"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn stat_requires_read_capability() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::new(workspace.id, Default::default(), []);

    let error = fs
        .stat(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn read_dir_requires_read_capability() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::new(workspace.id, Default::default(), []);

    let error = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn malformed_cursor_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .read_dir(
            &ctx,
            &VirtualPath::root(),
            PageRequest::default().cursor(Some("not-a-real-cursor".to_string())),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidCursor);
}

#[tokio::test]
async fn cursor_from_one_workspace_is_rejected_in_another() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let first = fs.create_workspace(Default::default()).await.unwrap();
    let second = fs.create_workspace(Default::default()).await.unwrap();
    let first_ctx = RequestContext::trusted(first.id);
    let second_ctx = RequestContext::trusted(second.id);

    for name in ["a", "b", "c"] {
        fs.mkdir(&first_ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
        fs.mkdir(&second_ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let first_page = fs
        .read_dir(
            &first_ctx,
            &VirtualPath::root(),
            PageRequest::default().limit(1),
        )
        .await
        .unwrap();
    let cursor = first_page.next_cursor.expect("more pages remain");

    let error = fs
        .read_dir(
            &second_ctx,
            &VirtualPath::root(),
            PageRequest::default().cursor(Some(cursor)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidCursor);
}
