use fslite_core::{MutationOptions, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    set_and_remove_attribute_round_trip(factory).await;
}

async fn set_and_remove_attribute_round_trip(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/f").unwrap();

    fs.mkdir(&ctx, &path, Default::default()).await.unwrap();
    let node = fs
        .set_attribute(&ctx, &path, "color", b"blue", MutationOptions::default())
        .await
        .unwrap();
    assert!(node.attributes.contains_key("color"));

    let node = fs
        .remove_attribute(&ctx, &path, "color", MutationOptions::default())
        .await
        .unwrap();
    assert!(!node.attributes.contains_key("color"));
}
