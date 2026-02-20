use fslite_core::{
    CopyOptions, ErrorCode, MoveOptions, RemoveOptions, RequestContext, Revision, VirtualPath,
    WriteOptions, WriteSource,
};
use fslite_sqlite::SqliteFileSystem;
use futures::StreamExt;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn source(bytes: &[u8]) -> WriteSource {
    WriteSource::from_bytes(bytes.to_vec())
}

async fn read_all(fs: &SqliteFileSystem, ctx: &RequestContext, path: &VirtualPath) -> Vec<u8> {
    let mut stream = fs
        .read(ctx, path, Default::default())
        .await
        .unwrap()
        .into_stream();
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk.unwrap());
    }
    out
}

#[tokio::test]
async fn copy_file_creates_independent_content() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/src"), source(b"original"), Default::default())
        .await
        .unwrap();
    let copied = fs
        .copy(&ctx, &path("/src"), &path("/dst"), CopyOptions::default())
        .await
        .unwrap();
    assert_eq!(read_all(&fs, &ctx, &path("/dst")).await, b"original");

    // Mutating the destination must not affect the source: independent
    // content generations, not a shared reference.
    fs.write(
        &ctx,
        &path("/dst"),
        source(b"changed"),
        WriteOptions::replace(),
    )
    .await
    .unwrap();
    assert_eq!(read_all(&fs, &ctx, &path("/src")).await, b"original");
    assert_eq!(read_all(&fs, &ctx, &path("/dst")).await, b"changed");
    assert_ne!(
        copied.id,
        fs.stat(&ctx, &path("/src"), Default::default())
            .await
            .unwrap()
            .id
    );
}

#[tokio::test]
async fn copy_recursive_directory_copies_children() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(
        &ctx,
        &path("/src/a"),
        fslite_core::CreateOptions::default().parents(true),
    )
    .await
    .unwrap();
    fs.write(
        &ctx,
        &path("/src/file"),
        source(b"contents"),
        Default::default(),
    )
    .await
    .unwrap();

    fs.copy(
        &ctx,
        &path("/src"),
        &path("/dst"),
        CopyOptions::default().recursive(true),
    )
    .await
    .unwrap();

    assert!(
        fs.exists(&ctx, &path("/dst/a"), Default::default())
            .await
            .unwrap()
    );
    assert_eq!(read_all(&fs, &ctx, &path("/dst/file")).await, b"contents");
}

#[tokio::test]
async fn copy_directory_without_recursive_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/src"), Default::default())
        .await
        .unwrap();
    let error = fs
        .copy(&ctx, &path("/src"), &path("/dst"), CopyOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::WrongNodeType);
}

#[tokio::test]
async fn copy_without_overwrite_rejects_an_existing_destination() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/src"), source(b"a"), Default::default())
        .await
        .unwrap();
    fs.write(&ctx, &path("/dst"), source(b"b"), Default::default())
        .await
        .unwrap();

    let error = fs
        .copy(&ctx, &path("/src"), &path("/dst"), CopyOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyExists);
}

#[tokio::test]
async fn copy_with_explicit_overwrite_replaces_the_destination() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/src"), source(b"a"), Default::default())
        .await
        .unwrap();
    fs.write(&ctx, &path("/dst"), source(b"b"), Default::default())
        .await
        .unwrap();

    fs.copy(
        &ctx,
        &path("/src"),
        &path("/dst"),
        CopyOptions::default().overwrite(true),
    )
    .await
    .unwrap();
    assert_eq!(read_all(&fs, &ctx, &path("/dst")).await, b"a");
}

#[tokio::test]
async fn copy_expected_revision_conflict_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/src"), source(b"a"), Default::default())
        .await
        .unwrap();
    let bogus = Revision::new(999).unwrap();
    let error = fs
        .copy(
            &ctx,
            &path("/src"),
            &path("/dst"),
            CopyOptions::default().expected_revision(Some(bogus)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RevisionConflict);
}

#[tokio::test]
async fn move_renames_within_the_workspace() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/src"), source(b"x"), Default::default())
        .await
        .unwrap();
    fs.move_path(&ctx, &path("/src"), &path("/dst"), MoveOptions::default())
        .await
        .unwrap();

    assert_eq!(read_all(&fs, &ctx, &path("/dst")).await, b"x");
    let error = fs
        .stat(&ctx, &path("/src"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn move_rejects_a_destination_inside_the_source_subtree() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    let error = fs
        .move_path(&ctx, &path("/a"), &path("/a/b"), MoveOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn move_root_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .move_path(
            &ctx,
            &VirtualPath::root(),
            &path("/dst"),
            MoveOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn remove_non_recursive_directory_with_children_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(
        &ctx,
        &path("/a/b"),
        fslite_core::CreateOptions::default().parents(true),
    )
    .await
    .unwrap();
    let error = fs
        .remove(&ctx, &path("/a"), RemoveOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::DirectoryNotEmpty);
}

#[tokio::test]
async fn remove_recursive_deletes_the_whole_subtree() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(
        &ctx,
        &path("/a/b"),
        fslite_core::CreateOptions::default().parents(true),
    )
    .await
    .unwrap();
    fs.write(&ctx, &path("/a/file"), source(b"data"), Default::default())
        .await
        .unwrap();

    fs.remove(&ctx, &path("/a"), RemoveOptions::default().recursive(true))
        .await
        .unwrap();

    assert!(
        !fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );
    assert!(
        !fs.exists(&ctx, &path("/a/b"), Default::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn remove_root_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .remove(&ctx, &VirtualPath::root(), RemoveOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn remove_expected_revision_conflict_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"a"), Default::default())
        .await
        .unwrap();
    let bogus = Revision::new(999).unwrap();
    let error = fs
        .remove(
            &ctx,
            &path("/f"),
            RemoveOptions::default().expected_revision(Some(bogus)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RevisionConflict);
}

/// Regression test: `mkdir` colliding with an existing non-directory node
/// must report `WrongNodeType`. This was untestable in Task 5 (no way yet
/// to create a non-directory node); `touch` from Task 6 closes that gap.
#[tokio::test]
async fn mkdir_over_an_existing_file_is_wrong_node_type() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.touch(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();
    let error = fs
        .mkdir(&ctx, &path("/f"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::WrongNodeType);
}

