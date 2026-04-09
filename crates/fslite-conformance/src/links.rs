use fslite_core::{CreateOptions, NodeKind, RequestContext, StatOptions, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    symlink_round_trips_and_follows(factory).await;
}

async fn symlink_round_trips_and_follows(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/real").unwrap(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.symlink(
        &ctx,
        &fslite_core::LinkTarget::parse("/real").unwrap(),
        &VirtualPath::parse("/link").unwrap(),
        CreateOptions::default(),
    )
    .await
    .unwrap();

    let target = fs
        .read_link(&ctx, &VirtualPath::parse("/link").unwrap())
        .await
        .unwrap();
    assert_eq!(target.as_str(), "/real");

    let followed = fs
        .stat(
            &ctx,
            &VirtualPath::parse("/link").unwrap(),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(followed.kind, NodeKind::Directory);

    let not_followed = fs
        .stat(
            &ctx,
            &VirtualPath::parse("/link").unwrap(),
            StatOptions::default().follow_symlinks(false),
        )
        .await
        .unwrap();
    assert_eq!(not_followed.kind, NodeKind::Symlink);
}
