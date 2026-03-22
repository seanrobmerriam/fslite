use fslite_core::{
    CreateOptions, ErrorCode, LinkTarget, NodeKind, RequestContext, StatOptions, VirtualPath,
    WriteSource,
};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn target(input: &str) -> LinkTarget {
    LinkTarget::parse(input).unwrap()
}

#[tokio::test]
async fn symlink_and_read_link_round_trip_an_absolute_target() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let node = fs
        .symlink(
            &ctx,
            &target("/real"),
            &path("/link"),
            CreateOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(node.kind, NodeKind::Symlink);

    let read_back = fs.read_link(&ctx, &path("/link")).await.unwrap();
    assert_eq!(read_back.as_str(), "/real");
    assert!(read_back.is_absolute());
}

#[tokio::test]
async fn symlink_and_read_link_round_trip_a_relative_target() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.symlink(
        &ctx,
        &target("../shared/file"),
        &path("/a/link"),
        CreateOptions::default(),
    )
    .await
    .unwrap_err(); // /a does not exist yet

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    fs.symlink(
        &ctx,
        &target("../shared/file"),
        &path("/a/link"),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let read_back = fs.read_link(&ctx, &path("/a/link")).await.unwrap();
    assert_eq!(read_back.as_str(), "../shared/file");
    assert!(!read_back.is_absolute());
}

#[tokio::test]
async fn read_link_on_a_non_symlink_is_wrong_node_type() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/dir"), Default::default())
        .await
        .unwrap();
    let error = fs.read_link(&ctx, &path("/dir")).await.unwrap_err();
    assert_eq!(error.code(), ErrorCode::WrongNodeType);
}

#[tokio::test]
async fn stat_follows_a_symlink_by_default_but_not_when_asked_not_to() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/real"),
        WriteSource::from_bytes(b"hi".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();
    fs.symlink(
        &ctx,
        &target("/real"),
        &path("/link"),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let followed = fs
        .stat(&ctx, &path("/link"), Default::default())
        .await
        .unwrap();
    assert_eq!(followed.kind, NodeKind::File);

    let not_followed = fs
        .stat(
            &ctx,
            &path("/link"),
            StatOptions::default().follow_symlinks(false),
        )
        .await
        .unwrap();
    assert_eq!(not_followed.kind, NodeKind::Symlink);
}

#[tokio::test]
async fn stat_on_a_broken_symlink_reports_broken_link_only_when_following() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.symlink(
        &ctx,
        &target("/missing"),
        &path("/link"),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let error = fs
        .stat(&ctx, &path("/link"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::BrokenLink);

    let lstat = fs
        .stat(
            &ctx,
            &path("/link"),
            StatOptions::default().follow_symlinks(false),
        )
        .await
        .unwrap();
    assert_eq!(lstat.kind, NodeKind::Symlink);
}

#[tokio::test]
async fn a_symlink_cycle_is_detected_as_a_link_loop() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.symlink(&ctx, &target("/b"), &path("/a"), CreateOptions::default())
        .await
        .unwrap();
    fs.symlink(&ctx, &target("/a"), &path("/b"), CreateOptions::default())
        .await
        .unwrap();

    let error = fs
        .stat(&ctx, &path("/a"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LinkLoop);
}

#[tokio::test]
async fn a_relative_symlink_resolves_against_its_containing_directory() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();
    fs.mkdir(&ctx, &path("/b"), Default::default())
        .await
        .unwrap();
    fs.write(
        &ctx,
        &path("/b/file"),
        WriteSource::from_bytes(b"hi".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    // /a/link -> ../b/file, resolved relative to /a (the link's containing
    // directory), not relative to /a/link itself.
    fs.symlink(
        &ctx,
        &target("../b/file"),
        &path("/a/link"),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let resolved = fs
        .stat(&ctx, &path("/a/link"), Default::default())
        .await
        .unwrap();
    let direct = fs
        .stat(&ctx, &path("/b/file"), Default::default())
        .await
        .unwrap();
    assert_eq!(resolved.id, direct.id);
}

#[tokio::test]
async fn a_symlink_chain_within_the_hop_limit_resolves() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/real"),
        WriteSource::from_bytes(b"hi".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    // link-39 -> real, link-38 -> link-39, ..., link-0 -> link-1: 39 hops to
    // the file, comfortably within the 40-hop limit.
    let mut next = "/real".to_string();
    for i in (0..39).rev() {
        let name = format!("/link-{i}");
        fs.symlink(&ctx, &target(&next), &path(&name), CreateOptions::default())
            .await
            .unwrap();
        next = name;
    }

    let resolved = fs
        .stat(&ctx, &path("/link-0"), Default::default())
        .await
        .unwrap();
    assert_eq!(resolved.kind, NodeKind::File);
}

#[tokio::test]
async fn a_symlink_chain_beyond_the_hop_limit_is_a_link_loop() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/real"),
        WriteSource::from_bytes(b"hi".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    // 45 hops to the file: past the 40-hop limit.
    let mut next = "/real".to_string();
    for i in (0..45).rev() {
        let name = format!("/link-{i}");
        fs.symlink(&ctx, &target(&next), &path(&name), CreateOptions::default())
            .await
            .unwrap();
        next = name;
    }

    let error = fs
        .stat(&ctx, &path("/link-0"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LinkLoop);
}

#[tokio::test]
async fn symlink_requires_write_capability_and_exists_ok_returns_existing() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let first = fs
        .symlink(
            &ctx,
            &target("/real"),
            &path("/link"),
            CreateOptions::default(),
        )
        .await
        .unwrap();
    let error = fs
        .symlink(
            &ctx,
            &target("/other"),
            &path("/link"),
            CreateOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyExists);

    let second = fs
        .symlink(
            &ctx,
            &target("/real"),
            &path("/link"),
            CreateOptions::default().exist_ok(true),
        )
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
}

