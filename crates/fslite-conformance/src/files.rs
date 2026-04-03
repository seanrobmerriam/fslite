use bytes::Bytes;
use fslite_core::{FsError, RequestContext, VirtualPath, WriteOptions, WriteSource};
use futures::StreamExt;

use crate::ConformanceFactory;

pub(crate) async fn run(factory: &dyn ConformanceFactory) {
    write_and_read_round_trip(factory).await;
    replacement_bumps_revision(factory).await;
    interrupted_write_preserves_prior_content(factory).await;
}

async fn write_and_read_round_trip(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/f").unwrap();

    fs.write(
        &ctx,
        &path,
        WriteSource::from_bytes(b"hello".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    let read = fs.read(&ctx, &path, Default::default()).await.unwrap();
    let mut stream = read.into_stream();
    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        collected.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(collected, b"hello");
}

async fn replacement_bumps_revision(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/f").unwrap();

    let first = fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"a".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    let second = fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"bb".to_vec()),
            WriteOptions::replace(),
        )
        .await
        .unwrap();

    assert!(second.revision.get() > first.revision.get());
    assert_eq!(second.logical_size, 2);
}

async fn interrupted_write_preserves_prior_content(factory: &dyn ConformanceFactory) {
    let fs = factory.fresh().await;
    let workspace_id = factory.workspace(&*fs).await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/f").unwrap();

    let first = fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"old".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let failing = WriteSource::new(futures::stream::iter([
        Ok(Bytes::from_static(b"new")),
        Err(FsError::internal_storage_failure(
            "simulated stream failure",
        )),
    ]));
    assert!(
        fs.write(&ctx, &path, failing, WriteOptions::replace())
            .await
            .is_err()
    );

    let node = fs.stat(&ctx, &path, Default::default()).await.unwrap();
    assert_eq!(node.revision, first.revision);
}
