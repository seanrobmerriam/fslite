use std::sync::Arc;

use fslite_command::{Command, CommandOutput, Executor, LocalExecutor};
use fslite_core::{RequestContext, VirtualPath, WriteOptions};
use fslite_sqlite::SqliteFileSystem;

async fn fixture() -> (LocalExecutor, RequestContext) {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    (LocalExecutor::new(Arc::new(fs)), ctx)
}

#[tokio::test]
async fn mkdir_then_stat_round_trips_through_the_codec() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/docs").unwrap();

    let created = executor
        .execute(
            &ctx,
            Command::Mkdir {
                path: path.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(created, CommandOutput::Node(_)));

    let stat = executor
        .execute(
            &ctx,
            Command::Stat {
                path,
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    match stat {
        CommandOutput::Node(node) => assert_eq!(node.kind, fslite_core::NodeKind::Directory),
        other => panic!("expected Node, got {other:?}"),
    }
}

#[tokio::test]
async fn write_then_read_round_trips_bytes() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/a.txt").unwrap();

    executor
        .execute(
            &ctx,
            Command::Write {
                path: path.clone(),
                bytes: b"hello".to_vec(),
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    let output = executor
        .execute(
            &ctx,
            Command::Read {
                path,
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    match output {
        CommandOutput::Content {
            bytes,
            logical_length,
            ..
        } => {
            assert_eq!(bytes, b"hello");
            assert_eq!(logical_length, 5);
        }
        other => panic!("expected Content, got {other:?}"),
    }
}

#[tokio::test]
async fn remove_returns_unit() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/a.txt").unwrap();
    executor
        .execute(
            &ctx,
            Command::Write {
                path: path.clone(),
                bytes: b"x".to_vec(),
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    let output = executor
        .execute(
            &ctx,
            Command::Remove {
                path,
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(output, CommandOutput::Unit);
}

#[tokio::test]
async fn batch_returns_batch_results() {
    let (executor, ctx) = fixture().await;
    let ops = vec![fslite_core::BatchOperation::Mkdir {
        path: VirtualPath::parse("/a").unwrap(),
        options: Default::default(),
    }];
    let output = executor.execute(&ctx, Command::Batch(ops)).await.unwrap();
    match output {
        CommandOutput::Batch(results) => assert_eq!(results.len(), 1),
        other => panic!("expected Batch, got {other:?}"),
    }
}
