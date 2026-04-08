use fslite_core::{ErrorCode, MutationOptions, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    trash_hides_and_restore_recovers(factory).await;
    purge_is_permanent(factory).await;
}

async fn trash_hides_and_restore_recovers(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();
    let entry = fs
        .trash(
            &ctx,
            &VirtualPath::parse("/a").unwrap(),
            MutationOptions::default(),
        )
        .await
        .unwrap();
    assert!(
        !fs.exists(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
            .await
            .unwrap()
    );

    fs.restore(&ctx, entry.id, None, MutationOptions::default())
        .await
        .unwrap();
    assert!(
        fs.exists(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
            .await
            .unwrap()
    );
}

async fn purge_is_permanent(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();
    let entry = fs
        .trash(
            &ctx,
            &VirtualPath::parse("/a").unwrap(),
            MutationOptions::default(),
        )
        .await
        .unwrap();
    fs.purge(&ctx, entry.id).await.unwrap();

    let error = fs
        .restore(&ctx, entry.id, None, MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}
