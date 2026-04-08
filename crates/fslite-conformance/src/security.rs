use fslite_core::{Capability, CreateOptions, ErrorCode, RequestContext, VirtualPath};

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    read_only_context_cannot_write(factory).await;
}

async fn read_only_context_cannot_write(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let read_only = RequestContext::new(workspace_id, Default::default(), [Capability::Read]);

    let error = fs
        .mkdir(
            &read_only,
            &VirtualPath::parse("/a").unwrap(),
            CreateOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PermissionDenied);
}
