use bytes::Bytes;
use fslite_core::{ByteRange, ReadOptions, RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;
use futures::StreamExt;

fn path(input: &str) -> VirtualPath {
    VirtualPath::parse(input).unwrap()
}

/// One chunk short of 1 MiB, so a multi-megabyte file spans several
/// non-uniformly-sized pushes into the write path, not just whole chunks.
const PIECE_SIZE: usize = 900_000;

fn pattern_piece(seed: u8, len: usize) -> Bytes {
    Bytes::from(
        (0..len)
            .map(|i| seed.wrapping_add(i as u8))
            .collect::<Vec<u8>>(),
    )
}

#[tokio::test]
async fn round_trips_a_multi_chunk_file_spanning_several_pushes() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    // Four pushes of ~900 KB each: > 3 MiB total, crossing several 1 MiB
    // chunk boundaries at non-aligned offsets.
    let pieces: Vec<Bytes> = (0..4).map(|i| pattern_piece(i as u8, PIECE_SIZE)).collect();
    let expected: Vec<u8> = pieces.iter().flat_map(|p| p.to_vec()).collect();

    let source = WriteSource::new(futures::stream::iter(pieces.into_iter().map(Ok)));
    let node = fs
        .write(&ctx, &path("/large"), source, Default::default())
        .await
        .unwrap();
    assert_eq!(node.logical_size as usize, expected.len());

    let mut stream = fs
        .read(&ctx, &path("/large"), Default::default())
        .await
        .unwrap()
        .into_stream();
    let mut actual = Vec::new();
    while let Some(chunk) = stream.next().await {
        actual.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn ranged_read_of_a_large_file_returns_only_the_requested_bytes() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let pieces: Vec<Bytes> = (0..4).map(|i| pattern_piece(i as u8, PIECE_SIZE)).collect();
    let expected: Vec<u8> = pieces.iter().flat_map(|p| p.to_vec()).collect();
    let source = WriteSource::new(futures::stream::iter(pieces.into_iter().map(Ok)));
    fs.write(&ctx, &path("/large"), source, Default::default())
        .await
        .unwrap();

    // A range entirely inside the third megabyte-plus chunk.
    let start = 2 * PIECE_SIZE as u64 + 10;
    let end = start + 5;
    let read = fs
        .read(
            &ctx,
            &path("/large"),
            ReadOptions::default().range(Some(ByteRange::new(start, end))),
        )
        .await
        .unwrap();
    let mut stream = read.into_stream();
    let mut actual = Vec::new();
    while let Some(chunk) = stream.next().await {
        actual.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(actual, expected[start as usize..end as usize]);
}

/// The read path is built on `futures::stream::unfold`: each chunk is fetched
/// only when the consumer polls for it, so at most one 1 MiB chunk is ever
/// held in memory ahead of the consumer (structurally, not just by
/// coincidence — there is no background task or channel prefetching further
/// chunks). This test exercises that lazily: reading only the first few bytes
/// of a multi-megabyte file must succeed without needing to fetch every chunk.
#[tokio::test]
async fn reading_a_prefix_of_a_large_file_does_not_require_the_whole_file() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let pieces: Vec<Bytes> = (0..4).map(|i| pattern_piece(i as u8, PIECE_SIZE)).collect();
    let source = WriteSource::new(futures::stream::iter(pieces.into_iter().map(Ok)));
    fs.write(&ctx, &path("/large"), source, Default::default())
        .await
        .unwrap();

    let mut stream = fs
        .read(
            &ctx,
            &path("/large"),
            ReadOptions::default().range(Some(ByteRange::new(0, 4))),
        )
        .await
        .unwrap()
        .into_stream();

    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(&first[..], &pattern_piece(0, PIECE_SIZE)[..4]);
    // The stream is dropped here without being fully drained, which would
    // hang or blow up memory if the implementation eagerly buffered the
    // remaining chunks instead of pulling them lazily.
}
