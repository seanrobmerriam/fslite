use fslite_core::{
    ContentQuery, CreateOptions, ErrorCode, FindQuery, NodeKind, PageRequest, RequestContext,
    VirtualPath, WriteSource,
};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn names(nodes: &[fslite_core::Node]) -> Vec<&str> {
    let mut names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    names.sort_unstable();
    names
}

#[tokio::test]
async fn glob_star_matches_within_one_segment() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for name in ["cat.txt", "car.txt", "dog.txt"] {
        fs.touch(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let page = fs.glob(&ctx, "/ca*.txt", Default::default()).await.unwrap();
    assert_eq!(names(&page.items), vec!["car.txt", "cat.txt"]);
}

#[tokio::test]
async fn glob_question_mark_matches_exactly_one_character() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for name in ["a.txt", "ab.txt"] {
        fs.touch(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let page = fs.glob(&ctx, "/?.txt", Default::default()).await.unwrap();
    assert_eq!(names(&page.items), vec!["a.txt"]);
}

#[tokio::test]
async fn glob_double_star_matches_across_segments() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(
        &ctx,
        &path("/a/b/c"),
        CreateOptions::default().parents(true),
    )
    .await
    .unwrap();
    fs.touch(&ctx, &path("/a/target.txt"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/a/b/target.txt"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/a/b/c/target.txt"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/a/other.txt"), Default::default())
        .await
        .unwrap();

    let page = fs
        .glob(&ctx, "/a/**/target.txt", Default::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
}

#[tokio::test]
async fn glob_on_a_missing_prefix_returns_an_empty_page() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let page = fs
        .glob(&ctx, "/missing/*.txt", Default::default())
        .await
        .unwrap();
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn find_filters_by_kind() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.mkdir(&ctx, &path("/dir"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/file"), Default::default())
        .await
        .unwrap();

    let page = fs
        .find(
            &ctx,
            FindQuery::default().kind(Some(NodeKind::Directory)),
            Default::default(),
        )
        .await
        .unwrap();
    // The search root itself ("/", name "") is a candidate too, matching
    // `find`'s own Unix semantics (e.g. `find / -type d` includes `/`).
    assert_eq!(names(&page.items), vec!["", "dir"]);
}

#[tokio::test]
async fn find_filters_by_size_range() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/small"),
        WriteSource::from_bytes(b"a".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();
    fs.write(
        &ctx,
        &path("/big"),
        WriteSource::from_bytes(vec![0u8; 1000]),
        Default::default(),
    )
    .await
    .unwrap();

    let page = fs
        .find(
            &ctx,
            FindQuery::default().min_logical_size(Some(10)),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(names(&page.items), vec!["big"]);
}

#[tokio::test]
async fn find_filters_by_modified_time() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.touch(&ctx, &path("/f"), Default::default())
        .await
        .unwrap();

    let far_future = fs
        .find(
            &ctx,
            FindQuery::default().modified_after_ms(Some(i64::MAX - 1)),
            Default::default(),
        )
        .await
        .unwrap();
    assert!(far_future.items.is_empty());

    let far_past = fs
        .find(
            &ctx,
            FindQuery::default().modified_after_ms(Some(0)),
            Default::default(),
        )
        .await
        .unwrap();
    assert!(far_past.items.iter().any(|n| n.name == "f"));
}

#[tokio::test]
async fn find_filters_by_name_contains() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.touch(&ctx, &path("/report-final.txt"), Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/draft.txt"), Default::default())
        .await
        .unwrap();

    let page = fs
        .find(
            &ctx,
            FindQuery::default().name_contains(Some("final".to_string())),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(names(&page.items), vec!["report-final.txt"]);
}

#[tokio::test]
async fn find_filters_by_attribute() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.touch(&ctx, &path("/tagged"), Default::default())
        .await
        .unwrap();
    let tagged = fs
        .set_attribute(&ctx, &path("/tagged"), "color", b"blue", Default::default())
        .await
        .unwrap();
    fs.touch(&ctx, &path("/untagged"), Default::default())
        .await
        .unwrap();

    let mut required = std::collections::BTreeMap::new();
    required.insert(
        "color".to_string(),
        tagged.attributes.get("color").cloned().unwrap(),
    );

    let page = fs
        .find(
            &ctx,
            FindQuery::default().attributes(required),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(names(&page.items), vec!["tagged"]);
}

#[tokio::test]
async fn find_on_a_missing_root_is_not_found() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .find(
            &ctx,
            FindQuery::default().root(path("/missing")),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn search_content_finds_a_literal_match() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(
        &ctx,
        &path("/f"),
        WriteSource::from_bytes(b"the quick brown fox".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    let page = fs
        .search_content(
            &ctx,
            ContentQuery::default().needle(b"brown".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].path, path("/f"));
    assert_eq!(page.items[0].range, fslite_core::ByteRange::new(10, 15));
}

#[tokio::test]
async fn search_content_finds_a_match_spanning_a_chunk_boundary() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    const CHUNK_SIZE: usize = 1024 * 1024;
    let needle = b"STRADDLE";
    // Place the needle so it starts 3 bytes before the chunk boundary.
    let split_point = CHUNK_SIZE - 3;
    let mut content = vec![b'x'; split_point];
    content.extend_from_slice(needle);
    content.extend(vec![b'y'; 100]);

    fs.write(
        &ctx,
        &path("/big"),
        WriteSource::from_bytes(content.clone()),
        Default::default(),
    )
    .await
    .unwrap();

    let page = fs
        .search_content(
            &ctx,
            ContentQuery::default().needle(needle.to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].range.start as usize, split_point);
}

#[tokio::test]
async fn search_content_rejects_an_empty_needle() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .search_content(&ctx, ContentQuery::default(), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidRange);
}

#[tokio::test]
async fn search_content_result_limit_is_respected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for i in 0..5 {
        fs.write(
            &ctx,
            &path(&format!("/f{i}")),
            WriteSource::from_bytes(b"needle-here".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    }

    let page = fs
        .search_content(
            &ctx,
            ContentQuery::default().needle(b"needle".to_vec()),
            PageRequest::default().limit(2),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
}
