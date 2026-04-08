use fslite_core::{CreateOptions, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    normalizes_equivalent_paths_to_the_same_node(factory).await;
}

async fn normalizes_equivalent_paths_to_the_same_node(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/a/b").unwrap(),
        CreateOptions::default().parents(true),
    )
    .await
    .unwrap();

    // "/a/./b/../b" normalizes to the same node as "/a/b".
    let equivalent = VirtualPath::parse("/a/./b/../b").unwrap();
    let node = fs
        .stat(&ctx, &equivalent, Default::default())
        .await
        .unwrap();
    assert_eq!(node.name, "b");
}
