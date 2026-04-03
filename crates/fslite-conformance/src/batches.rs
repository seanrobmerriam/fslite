use fslite_core::{BatchOperation, BatchResult, CreateOptions, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    batch_commits_all_or_nothing(factory).await;
}

async fn batch_commits_all_or_nothing(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    let results = fs
        .batch(
            &ctx,
            vec![
                BatchOperation::Mkdir {
                    path: VirtualPath::parse("/a").unwrap(),
                    options: CreateOptions::default(),
                },
                BatchOperation::Mkdir {
                    path: VirtualPath::parse("/b").unwrap(),
                    options: CreateOptions::default(),
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    assert!(matches!(results[0], BatchResult::Node(_)));

    // The second operation duplicates "/a" and must fail, rolling back "/c"
    // from the same batch too.
    fs.batch(
        &ctx,
        vec![
            BatchOperation::Mkdir {
                path: VirtualPath::parse("/c").unwrap(),
                options: CreateOptions::default(),
            },
            BatchOperation::Mkdir {
                path: VirtualPath::parse("/a").unwrap(),
                options: CreateOptions::default(),
            },
        ],
    )
    .await
    .unwrap_err();

    assert!(
        !fs.exists(&ctx, &VirtualPath::parse("/c").unwrap(), Default::default())
            .await
            .unwrap()
    );
}
