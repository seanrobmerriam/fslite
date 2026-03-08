use std::collections::BTreeSet;

use fslite_core::{
    CreateOptions, ErrorCode, NodeKind, PageRequest, RequestContext, TreeOptions, VirtualPath,
};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

#[tokio::test]
async fn mkdir_parents_creates_missing_ancestors() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let made = fs
        .mkdir(
            &ctx,
            &path("/a/b/c"),
            CreateOptions::default().parents(true),
        )
        .await
        .unwrap();
    assert_eq!(made.kind, NodeKind::Directory);
    assert_eq!(made.name, "c");

    assert_eq!(
        fs.stat(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
            .kind,
        NodeKind::Directory
    );
    assert_eq!(
        fs.stat(&ctx, &path("/a/b"), Default::default())
            .await
            .unwrap()
            .kind,
        NodeKind::Directory
    );
}

#[tokio::test]
async fn mkdir_without_parents_fails_when_ancestor_missing() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .mkdir(&ctx, &path("/x/y"), CreateOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn mkdir_existing_directory_without_exist_ok_is_already_exists() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/dup"), Default::default())
        .await
        .unwrap();
    let error = fs
        .mkdir(&ctx, &path("/dup"), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::AlreadyExists);
}

#[tokio::test]
async fn mkdir_existing_directory_with_exist_ok_returns_existing_node() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let first = fs
        .mkdir(&ctx, &path("/dup2"), Default::default())
        .await
        .unwrap();
    let second = fs
        .mkdir(
            &ctx,
            &path("/dup2"),
            CreateOptions::default().exist_ok(true),
        )
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.revision, second.revision);
}

#[tokio::test]
async fn read_dir_lists_children_sorted_by_name() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for name in ["zebra", "apple", "mango"] {
        fs.mkdir(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let page = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    let names: Vec<&str> = page.items.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, vec!["apple", "mango", "zebra"]);
    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn read_dir_cursor_continuation_visits_each_child_once() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let expected: BTreeSet<String> = (0..7).map(|index| format!("child-{index}")).collect();
    for name in &expected {
        fs.mkdir(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let mut seen = BTreeSet::new();
    let mut cursor = None;
    loop {
        let page = fs
            .read_dir(
                &ctx,
                &VirtualPath::root(),
                PageRequest::default().limit(2).cursor(cursor.clone()),
            )
            .await
            .unwrap();
        for node in &page.items {
            assert!(seen.insert(node.name.clone()), "duplicate entry visited");
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(seen, expected);
}

#[tokio::test]
async fn tree_respects_max_depth() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(
        &ctx,
        &path("/a/b/c/d"),
        CreateOptions::default().parents(true),
    )
    .await
    .unwrap();

    let page = fs
        .tree(
            &ctx,
            &VirtualPath::root(),
            TreeOptions::default().max_depth(Some(2)),
            Default::default(),
        )
        .await
        .unwrap();

    let paths: Vec<String> = page
        .items
        .iter()
        .map(|entry| entry.path.as_str().to_string())
        .collect();
    assert!(paths.contains(&"/a".to_string()));
    assert!(paths.contains(&"/a/b".to_string()));
    assert!(!paths.iter().any(|p| p == "/a/b/c" || p == "/a/b/c/d"));
    assert!(page.items.iter().all(|entry| entry.depth <= 2));
}

#[tokio::test]
async fn identical_paths_in_two_workspaces_do_not_leak() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let first = fs.create_workspace(Default::default()).await.unwrap();
    let second = fs.create_workspace(Default::default()).await.unwrap();
    let first_ctx = RequestContext::trusted(first.id);
    let second_ctx = RequestContext::trusted(second.id);

    fs.mkdir(&first_ctx, &path("/shared"), Default::default())
        .await
        .unwrap();
    fs.mkdir(&second_ctx, &path("/shared"), Default::default())
        .await
        .unwrap();

    let first_listing = fs
        .read_dir(&first_ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    let second_listing = fs
        .read_dir(&second_ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();

    assert_eq!(first_listing.items.len(), 1);
    assert_eq!(second_listing.items.len(), 1);
    assert_ne!(first_listing.items[0].id, second_listing.items[0].id);
}

