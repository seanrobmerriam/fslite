use fslite_core::{CreateOptions, NodeKind, PageRequest, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    mkdir_and_stat_round_trip(factory).await;
    nested_parent_creation(factory).await;
    read_dir_lists_children(factory).await;
}

async fn mkdir_and_stat_round_trip(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    let made = fs
        .mkdir(
            &ctx,
            &VirtualPath::parse("/a").unwrap(),
            CreateOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(made.kind, NodeKind::Directory);

    let node = fs
        .stat(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();
    assert_eq!(node.id, made.id);
}

async fn nested_parent_creation(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/x/y/z").unwrap(),
        CreateOptions::default().parents(true),
    )
    .await
    .unwrap();

    assert!(
        fs.exists(
            &ctx,
            &VirtualPath::parse("/x/y").unwrap(),
            Default::default()
        )
        .await
        .unwrap()
    );
}

async fn read_dir_lists_children(factory: &dyn ConformanceFactory) {
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

    let page = fs
        .read_dir(&ctx, &VirtualPath::root(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
}
