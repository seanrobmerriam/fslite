use fslite_core::{RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    mutation_appends_a_change(factory).await;
}

async fn mutation_appends_a_change(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();

    let page = fs.changes(&ctx, None, Default::default()).await.unwrap();
    assert!(!page.items.is_empty());
}
