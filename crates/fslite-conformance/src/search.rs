use fslite_core::{CreateOptions, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    glob_matches_a_wildcard(factory).await;
}

async fn glob_matches_a_wildcard(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/a").unwrap(),
        CreateOptions::default(),
    )
    .await
    .unwrap();
    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/b").unwrap(),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let page = fs.glob(&ctx, "/*", Default::default()).await.unwrap();
    assert_eq!(page.items.len(), 2);
}
