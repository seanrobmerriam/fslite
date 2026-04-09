use fslite_core::{
    CopyOptions, MoveOptions, MutationOptions, RemoveOptions, RequestContext, VirtualPath,
    WriteSource,
};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    copy_creates_an_independent_node(factory).await;
    copy_preserves_attributes(factory).await;
    move_relocates_a_node(factory).await;
    remove_deletes_a_node(factory).await;
}

async fn copy_creates_an_independent_node(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.write(
        &ctx,
        &VirtualPath::parse("/src").unwrap(),
        WriteSource::from_bytes(b"x".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();
    let copied = fs
        .copy(
            &ctx,
            &VirtualPath::parse("/src").unwrap(),
            &VirtualPath::parse("/dst").unwrap(),
            CopyOptions::default(),
        )
        .await
        .unwrap();
    let source = fs
        .stat(
            &ctx,
            &VirtualPath::parse("/src").unwrap(),
            Default::default(),
        )
        .await
        .unwrap();
    assert_ne!(copied.id, source.id);
}

async fn copy_preserves_attributes(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(
        &ctx,
        &VirtualPath::parse("/src").unwrap(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.set_attribute(
        &ctx,
        &VirtualPath::parse("/src").unwrap(),
        "color",
        b"blue",
        MutationOptions::default(),
    )
    .await
    .unwrap();

    fs.copy(
        &ctx,
        &VirtualPath::parse("/src").unwrap(),
        &VirtualPath::parse("/dst").unwrap(),
        CopyOptions::default().recursive(true),
    )
    .await
    .unwrap();

    // set_attribute (and only set_attribute/remove_attribute) populates
    // Node.attributes today, so re-fetch the copy's attributes the same way.
    let refreshed = fs
        .set_attribute(
            &ctx,
            &VirtualPath::parse("/dst").unwrap(),
            "probe",
            b"x",
            MutationOptions::default(),
        )
        .await
        .unwrap();
    assert!(
        refreshed.attributes.contains_key("color"),
        "copy must preserve the source node's custom attributes"
    );
}

async fn move_relocates_a_node(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();
    fs.move_path(
        &ctx,
        &VirtualPath::parse("/a").unwrap(),
        &VirtualPath::parse("/b").unwrap(),
        MoveOptions::default(),
    )
    .await
    .unwrap();

    assert!(
        !fs.exists(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
            .await
            .unwrap()
    );
    assert!(
        fs.exists(&ctx, &VirtualPath::parse("/b").unwrap(), Default::default())
            .await
            .unwrap()
    );
}

async fn remove_deletes_a_node(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);

    fs.mkdir(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
        .await
        .unwrap();
    fs.remove(
        &ctx,
        &VirtualPath::parse("/a").unwrap(),
        RemoveOptions::default(),
    )
    .await
    .unwrap();

    assert!(
        !fs.exists(&ctx, &VirtualPath::parse("/a").unwrap(), Default::default())
            .await
            .unwrap()
    );
}
