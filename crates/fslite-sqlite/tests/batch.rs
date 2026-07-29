use fslite_core::{
    BatchOperation, BatchResult, Capability, CreateOptions, ErrorCode, RequestContext, VirtualPath,
};
use fslite_sqlite::SqliteFileSystem;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

#[tokio::test]
async fn batch_operations_see_each_others_writes() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let results = fs
        .batch(
            &ctx,
            vec![
                BatchOperation::Mkdir {
                    path: path("/a"),
                    options: CreateOptions::default(),
                },
                BatchOperation::Touch {
                    path: path("/a/f"),
                    options: Default::default(),
                },
            ],
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    for result in &results {
        assert!(matches!(result, BatchResult::Node(_)));
    }
    assert!(
        fs.exists(&ctx, &path("/a/f"), Default::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn batch_rolls_back_entirely_when_the_last_operation_fails() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .batch(
            &ctx,
            vec![
                BatchOperation::Mkdir {
                    path: path("/a"),
                    options: CreateOptions::default(),
                },
                BatchOperation::Mkdir {
                    path: path("/b"),
                    options: CreateOptions::default(),
                },
                // Duplicate: fails and must undo both prior creates.
                BatchOperation::Mkdir {
                    path: path("/a"),
                    options: CreateOptions::default(),
                },
            ],
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::AlreadyExists);
    assert_eq!(error.details()["index"], 2);

    assert!(
        !fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );
    assert!(
        !fs.exists(&ctx, &path("/b"), Default::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn batch_reports_the_failing_operation_index_mid_batch() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .batch(
            &ctx,
            vec![
                BatchOperation::Mkdir {
                    path: path("/a"),
                    options: CreateOptions::default(),
                },
                BatchOperation::Mkdir {
                    path: path("/missing-parent/child"),
                    options: CreateOptions::default(),
                },
                BatchOperation::Mkdir {
                    path: path("/never-reached"),
                    options: CreateOptions::default(),
                },
            ],
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::NotFound);
    assert_eq!(error.details()["index"], 1);
    assert!(
        !fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );
    assert!(
        !fs.exists(&ctx, &path("/never-reached"), Default::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn batch_enforces_capability_per_operation() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let read_only_ctx = RequestContext::new(workspace.id, Default::default(), [Capability::Read]);

    let error = fs
        .batch(
            &read_only_ctx,
            vec![BatchOperation::Mkdir {
                path: path("/a"),
                options: CreateOptions::default(),
            }],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn batch_can_trash_and_restore_a_node() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    fs.mkdir(&ctx, &path("/a"), Default::default())
        .await
        .unwrap();

    let results = fs
        .batch(
            &ctx,
            vec![BatchOperation::Trash {
                path: path("/a"),
                options: Default::default(),
            }],
        )
        .await
        .unwrap();

    let BatchResult::Trash(entry) = &results[0] else {
        panic!("expected a trash result");
    };
    assert!(
        !fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );

    let restore_results = fs
        .batch(
            &ctx,
            vec![BatchOperation::Restore {
                trash: entry.id,
                destination: None,
                options: Default::default(),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(restore_results[0], BatchResult::Node(_)));
    assert!(
        fs.exists(&ctx, &path("/a"), Default::default())
            .await
            .unwrap()
    );
}
