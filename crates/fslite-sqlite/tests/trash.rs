use fslite_core::{
    CreateOptions, ErrorCode, MutationOptions, PageRequest, RequestContext, VirtualPath,
    WriteSource,
};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

#[tokio::test]
async fn trashing_a_node_makes_it_invisible() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    let entry = fs
        .trash(&ctx, &path("/a"), MutationOptions::default())
        .await
        .unwrap();
    assert_eq!(entry.original_path, path("/a"));

    let error = fs
        .stat(&ctx, &path("/a"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);

    let listing = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert!(listing.items.is_empty());
}

#[tokio::test]
async fn trashing_a_directory_preserves_its_subtree_for_restore() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a/b"), CreateOptions::default().parents(true))
        .await
        .unwrap();
    fs.write(
        &ctx,
        &path("/a/file"),
        WriteSource::from_bytes(b"data".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    let entry = fs
        .trash(&ctx, &path("/a"), MutationOptions::default())
        .await
        .unwrap();

    fs.restore(&ctx, entry.id, None, MutationOptions::default())
        .await
        .unwrap();

    assert!(
        fs.exists(&ctx, &path("/a/b"), Default::default())
            .await
            .unwrap()
    );
    assert!(
        fs.exists(&ctx, &path("/a/file"), Default::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn restore_to_an_alternate_destination() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    let entry = fs
        .trash(&ctx, &path("/a"), MutationOptions::default())
        .await
        .unwrap();

    let restored = fs
        .restore(
            &ctx,
            entry.id,
            Some(&path("/elsewhere")),
            MutationOptions::default(),
        )
        .await
        .unwrap();

    assert!(
        fs.exists(&ctx, &path("/elsewhere"), Default::default())
            .await
            .unwrap()
    );
    assert!(
        !fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );
    assert_eq!(restored.name, "elsewhere");
}

#[tokio::test]
async fn restore_collision_at_the_destination_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    let entry = fs
        .trash(&ctx, &path("/a"), MutationOptions::default())
        .await
        .unwrap();
    // Something new now occupies the original location.
    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();

    let error = fs
        .restore(&ctx, entry.id, None, MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyExists);
}

#[tokio::test]
async fn purge_permanently_removes_a_trashed_node_and_reclaims_content() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/f"),
        WriteSource::from_bytes(vec![7u8; 4096]),
        Default::default(),
    )
    .await
    .unwrap();
    let before = fs.workspace_usage(&ctx).await.unwrap();
    assert!(before.active_logical_bytes >= 4096);

    let entry = fs
        .trash(&ctx, &path("/f"), MutationOptions::default())
        .await
        .unwrap();
    fs.purge(&ctx, entry.id).await.unwrap();

    let after = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(after.active_logical_bytes, 0);
    assert_eq!(after.trashed_logical_bytes, 0);

    // The trash record itself is gone; restoring or purging again fails.
    let error = fs
        .restore(&ctx, entry.id, None, MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn list_trash_paginates_without_duplicates() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let mut trashed_ids = Vec::new();
    for name in ["a", "b", "c", "d", "e"] {
        fs.mkdir(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
        let entry = fs
            .trash(&ctx, &path(&format!("/{name}")), MutationOptions::default())
            .await
            .unwrap();
        trashed_ids.push(entry.id);
    }

    let mut seen = std::collections::HashSet::new();
    let mut cursor = None;
    loop {
        let page = fs
            .list_trash(&ctx, PageRequest::default().limit(2).cursor(cursor.clone()))
            .await
            .unwrap();
        for entry in &page.items {
            assert!(seen.insert(entry.id), "duplicate trash entry visited");
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(seen.len(), trashed_ids.len());
}

#[tokio::test]
async fn trash_root_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .trash(&ctx, &VirtualPath::root(), MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn restoring_an_unknown_trash_id_is_not_found() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let bogus = fslite_core::TrashId::new();
    let error = fs
        .restore(&ctx, bogus, None, MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

