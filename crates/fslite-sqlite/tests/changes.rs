use fslite_core::{ErrorCode, PageRequest, RequestContext, VirtualPath};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

#[tokio::test]
async fn change_sequences_strictly_increase() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for name in ["a", "b", "c"] {
        fs.mkdir(&ctx, &path(&format!("/{name}")), Default::default())
            .await
            .unwrap();
    }

    let page = fs.changes(&ctx, None, Default::default()).await.unwrap();
    assert_eq!(page.items.len(), 3);
    let sequences: Vec<u64> = page.items.iter().map(|c| c.sequence).collect();
    let mut sorted = sequences.clone();
    sorted.sort_unstable();
    assert_eq!(sequences, sorted, "sequences must already be increasing");
    assert!(sequences.windows(2).all(|w| w[1] > w[0]));
}

#[tokio::test]
async fn changes_paginate_without_duplicates_or_gaps() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    for i in 0..7 {
        fs.mkdir(&ctx, &path(&format!("/n{i}")), Default::default())
            .await
            .unwrap();
    }

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = fs
            .changes(&ctx, cursor.clone(), PageRequest::default().limit(2))
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|c| c.sequence));
        cursor = page.next_cursor.map(fslite_core::ChangeCursor::new);
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(seen.len(), 7);
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 7, "no duplicate sequences");
}

#[tokio::test]
async fn a_cursor_from_one_workspace_is_rejected_in_another() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let first = fs.create_workspace(Default::default()).await.unwrap();
    let second = fs.create_workspace(Default::default()).await.unwrap();
    let first_ctx = RequestContext::trusted(first.id);
    let second_ctx = RequestContext::trusted(second.id);

    for i in 0..3 {
        fs.mkdir(&first_ctx, &path(&format!("/n{i}")), Default::default())
            .await
            .unwrap();
        fs.mkdir(&second_ctx, &path(&format!("/n{i}")), Default::default())
            .await
            .unwrap();
    }

    let first_page = fs
        .changes(&first_ctx, None, PageRequest::default().limit(1))
        .await
        .unwrap();
    let cursor = first_page.next_cursor.expect("more pages remain");

    let error = fs
        .changes(
            &second_ctx,
            Some(fslite_core::ChangeCursor::new(cursor)),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidCursor);
}

#[tokio::test]
async fn malformed_change_cursor_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .changes(
            &ctx,
            Some(fslite_core::ChangeCursor::new("not-a-real-cursor")),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidCursor);
}
