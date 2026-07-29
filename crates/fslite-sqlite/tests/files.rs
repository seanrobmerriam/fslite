use bytes::Bytes;
use fslite_core::{
    ByteRange, ErrorCode, MutationOptions, NodeKind, ReadOptions, RequestContext, Revision,
    TouchOptions, VirtualPath, WriteOptions, WriteSource,
};
use fslite_sqlite::{SqliteFileSystem, WorkspaceOptions};
use futures::StreamExt;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

fn source(bytes: &[u8]) -> WriteSource {
    WriteSource::from_bytes(bytes.to_vec())
}

fn source_with_error(chunks: Vec<Bytes>) -> WriteSource {
    let stream = futures::stream::iter(chunks.into_iter().map(Ok).chain(std::iter::once(Err(
        fslite_core::FsError::internal_storage_failure("simulated stream failure"),
    ))));
    WriteSource::new(stream)
}

async fn collect(stream: fslite_core::ByteStream) -> Vec<u8> {
    let mut stream = stream;
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        out.extend_from_slice(&chunk.unwrap());
    }
    out
}

#[tokio::test]
async fn write_creates_an_empty_file() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let node = fs
        .write(&ctx, &path("/empty"), source(b""), Default::default())
        .await
        .unwrap();
    assert_eq!(node.kind, NodeKind::File);
    assert_eq!(node.logical_size, 0);
    assert_eq!(node.revision, Revision::INITIAL);

    let content = collect(
        fs.read(&ctx, &path("/empty"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert!(content.is_empty());
}

#[tokio::test]
async fn write_replaces_existing_content() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let first = fs
        .write(&ctx, &path("/f"), source(b"old"), Default::default())
        .await
        .unwrap();
    let second = fs
        .write(
            &ctx,
            &path("/f"),
            source(b"newvalue"),
            WriteOptions::replace(),
        )
        .await
        .unwrap();

    assert!(second.revision.get() > first.revision.get());
    assert_eq!(second.logical_size, 8);
    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"newvalue");
}

#[tokio::test]
async fn write_without_create_fails_when_missing() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .write(
            &ctx,
            &path("/missing"),
            source(b"x"),
            WriteOptions::replace(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn append_extends_existing_content() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"abc"), Default::default())
        .await
        .unwrap();
    let appended = fs
        .append(&ctx, &path("/f"), source(b"def"), Default::default())
        .await
        .unwrap();
    assert_eq!(appended.logical_size, 6);

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"abcdef");
}

#[tokio::test]
async fn append_creates_file_when_absent_and_create_is_true() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let node = fs
        .append(&ctx, &path("/new"), source(b"hello"), Default::default())
        .await
        .unwrap();
    assert_eq!(node.logical_size, 5);
}

#[tokio::test]
async fn write_at_overwrites_a_byte_range() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"abcdefgh"), Default::default())
        .await
        .unwrap();
    fs.write_at(&ctx, &path("/f"), 2, source(b"XY"), Default::default())
        .await
        .unwrap();

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"abXYefgh");
}

#[tokio::test]
async fn write_at_beyond_eof_zero_fills_the_gap() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"ab"), Default::default())
        .await
        .unwrap();
    let node = fs
        .write_at(&ctx, &path("/f"), 5, source(b"Z"), Default::default())
        .await
        .unwrap();
    assert_eq!(node.logical_size, 6);

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"ab\0\0\0Z");
}

#[tokio::test]
async fn truncate_shrinks_a_file() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"abcdef"), Default::default())
        .await
        .unwrap();
    let node = fs
        .truncate(&ctx, &path("/f"), 3, Default::default())
        .await
        .unwrap();
    assert_eq!(node.logical_size, 3);

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"abc");
}

#[tokio::test]
async fn truncate_grows_a_file_with_zero_fill() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"ab"), Default::default())
        .await
        .unwrap();
    let node = fs
        .truncate(&ctx, &path("/f"), 5, Default::default())
        .await
        .unwrap();
    assert_eq!(node.logical_size, 5);

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"ab\0\0\0");
}

#[tokio::test]
async fn read_range_is_inclusive_start_exclusive_end() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"0123456789"), Default::default())
        .await
        .unwrap();

    let read = fs
        .read(
            &ctx,
            &path("/f"),
            ReadOptions::default().range(Some(ByteRange::new(2, 5))),
        )
        .await
        .unwrap();
    assert_eq!(read.range, ByteRange::new(2, 5));
    let content = collect(read.into_stream()).await;
    assert_eq!(content, b"234");
}

#[tokio::test]
async fn write_expected_revision_conflict_is_rejected() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    fs.write(&ctx, &path("/f"), source(b"a"), Default::default())
        .await
        .unwrap();

    let bogus_revision = Revision::new(999).unwrap();
    let error = fs
        .write(
            &ctx,
            &path("/f"),
            source(b"b"),
            WriteOptions::replace().expected_revision(Some(bogus_revision)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RevisionConflict);
}

#[tokio::test]
async fn write_rejects_content_over_the_file_quota() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let mut options = WorkspaceOptions::default();
    options.max_file_bytes = 4;
    let workspace = fs.create_workspace(options).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .write(
            &ctx,
            &path("/big"),
            source(b"way too big"),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::QuotaExceeded);

    let exists = fs
        .exists(&ctx, &path("/big"), Default::default())
        .await
        .unwrap();
    assert!(!exists, "a rejected write must not leave a partial node");
}

#[tokio::test]
async fn interrupted_write_preserves_previous_content_and_revision() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let first = fs
        .write(&ctx, &path("/f"), source(b"old"), Default::default())
        .await
        .unwrap();

    let failure = source_with_error(vec![
        Bytes::from_static(b"new"),
        Bytes::from_static(b"partial"),
    ]);
    assert!(
        fs.write(&ctx, &path("/f"), failure, WriteOptions::replace())
            .await
            .is_err()
    );

    let content = collect(
        fs.read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"old");
    assert_eq!(
        fs.stat(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .revision,
        first.revision
    );
}

#[tokio::test]
async fn content_persists_after_reopen() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    fs.write(&ctx, &path("/f"), source(b"durable"), Default::default())
        .await
        .unwrap();
    drop(fs);

    let reopened = SqliteFileSystem::open(file.path(), Default::default())
        .await
        .unwrap();
    let content = collect(
        reopened
            .read(&ctx, &path("/f"), Default::default())
            .await
            .unwrap()
            .into_stream(),
    )
    .await;
    assert_eq!(content, b"durable");
}

#[tokio::test]
async fn touch_creates_an_empty_file_and_bumps_revision_when_present() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let created = fs
        .touch(&ctx, &path("/f"), TouchOptions::default())
        .await
        .unwrap();
    assert_eq!(created.logical_size, 0);
    assert_eq!(created.revision, Revision::INITIAL);

    let touched_again = fs
        .touch(&ctx, &path("/f"), TouchOptions::default())
        .await
        .unwrap();
    assert!(touched_again.revision.get() > created.revision.get());
    assert_eq!(touched_again.logical_size, 0);
}

#[tokio::test]
async fn touch_without_create_fails_when_missing() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .touch(&ctx, &path("/f"), TouchOptions::default().create(false))
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn truncate_on_missing_file_is_not_found() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let error = fs
        .truncate(&ctx, &path("/missing"), 10, MutationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::NotFound);
}
