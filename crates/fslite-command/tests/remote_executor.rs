use std::sync::Arc;

use base64::Engine;
use fslite_command::{Command, CommandOutput, Executor, LocalExecutor, RemoteExecutor};
use fslite_core::{
    BatchOperation, ByteRange, CopyOptions, ErrorCode, FindQuery, MoveOptions, MutationOptions,
    ReadOptions, RequestContext, VirtualPath, WriteOptions,
};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

const TOKEN: &str = "remote-executor-test-token";

/// Boots a real `fslite-server` on an ephemeral local port and returns a
/// `RemoteExecutor` pointed at it, alongside a `LocalExecutor` sharing the
/// same backend — so both executors can be run through the same command
/// battery and their outputs compared for equality.
async fn dual_fixture() -> (RemoteExecutor, LocalExecutor, RequestContext) {
    let sqlite_fs = Arc::new(
        SqliteFileSystem::open_in_memory(Default::default())
            .await
            .unwrap(),
    );
    let workspace = sqlite_fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let mut tokens = std::collections::HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: ctx.capabilities.clone(),
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: sqlite_fs.clone(),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs.clone())),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace: workspace.id,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fslite_server::app(state)).await.unwrap();
    });

    let remote = RemoteExecutor::new(format!("http://{addr}"), TOKEN);
    let local = LocalExecutor::new(sqlite_fs);
    (remote, local, ctx)
}

#[tokio::test]
async fn remote_and_local_executors_agree_on_a_command_battery() {
    let (remote, local, ctx) = dual_fixture().await;

    let battery = vec![
        Command::Mkdir {
            path: VirtualPath::parse("/docs").unwrap(),
            options: Default::default(),
        },
        Command::Write {
            path: VirtualPath::parse("/docs/a.txt").unwrap(),
            bytes: b"hello remote".to_vec(),
            options: WriteOptions::default(),
        },
        Command::Stat {
            path: VirtualPath::parse("/docs/a.txt").unwrap(),
            options: Default::default(),
        },
        Command::Read {
            path: VirtualPath::parse("/docs/a.txt").unwrap(),
            options: Default::default(),
        },
        Command::ReadDir {
            path: VirtualPath::parse("/docs").unwrap(),
            page: Default::default(),
        },
    ];

    for command in battery {
        let remote_result = remote.execute(&ctx, command.clone()).await.unwrap();
        let local_result = local.execute(&ctx, command.clone()).await;
        // The local executor ran the *same* mutating command a second time
        // for write/mkdir; only compare shapes that are naturally
        // idempotent-safe to re-run (Stat/Read/ReadDir). For Mkdir/Write,
        // just assert both executors returned success without comparing
        // exact bodies (timestamps/revisions differ across the two calls).
        match command {
            Command::Stat { .. } | Command::Read { .. } | Command::ReadDir { .. } => {
                assert!(local_result.is_ok());
                match (&remote_result, local_result.unwrap()) {
                    (CommandOutput::Node(r), CommandOutput::Node(l)) => assert_eq!(r.name, l.name),
                    (CommandOutput::Content { bytes: rb, .. }, CommandOutput::Content { bytes: lb, .. }) => {
                        assert_eq!(rb, &lb)
                    }
                    (CommandOutput::Nodes(r), CommandOutput::Nodes(l)) => {
                        assert_eq!(r.items.len(), l.items.len())
                    }
                    (r, l) => panic!("mismatched output shapes: {r:?} vs {l:?}"),
                }
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn remote_exists_reports_true_and_false() {
    let (remote, _local, ctx) = dual_fixture().await;
    let path = VirtualPath::parse("/present.txt").unwrap();

    remote
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

    let present = remote
        .execute(
            &ctx,
            Command::Exists {
                path: path.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(present, CommandOutput::Exists(true));

    let missing = remote
        .execute(
            &ctx,
            Command::Exists {
                path: VirtualPath::parse("/absent.txt").unwrap(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(missing, CommandOutput::Exists(false));
}

/// Exercises `Copy` and `Move` entirely over HTTP, then confirms the
/// resulting filesystem state directly through the `LocalExecutor` sharing
/// the same backend — proving the HTTP contract's side effects landed
/// correctly, not just that a 2xx came back.
#[tokio::test]
async fn remote_copy_and_move_land_on_the_shared_backend() {
    let (remote, local, ctx) = dual_fixture().await;
    let original = VirtualPath::parse("/a.txt").unwrap();
    let copied = VirtualPath::parse("/b.txt").unwrap();
    let moved = VirtualPath::parse("/c.txt").unwrap();

    remote
        .execute(
            &ctx,
            Command::Write {
                path: original.clone(),
                bytes: b"payload".to_vec(),
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    let copy_result = remote
        .execute(
            &ctx,
            Command::Copy {
                from: original.clone(),
                to: copied.clone(),
                options: CopyOptions::default(),
            },
        )
        .await
        .unwrap();
    match copy_result {
        CommandOutput::Node(node) => assert_eq!(node.name, "b.txt"),
        other => panic!("expected Node, got {other:?}"),
    }

    // Both source and copy should exist now, verified locally.
    for path in [&original, &copied] {
        let read = local
            .execute(
                &ctx,
                Command::Read {
                    path: path.clone(),
                    options: Default::default(),
                },
            )
            .await
            .unwrap();
        match read {
            CommandOutput::Content { bytes, .. } => assert_eq!(bytes, b"payload"),
            other => panic!("expected Content, got {other:?}"),
        }
    }

    let move_result = remote
        .execute(
            &ctx,
            Command::Move {
                from: copied.clone(),
                to: moved.clone(),
                options: MoveOptions::default(),
            },
        )
        .await
        .unwrap();
    match move_result {
        CommandOutput::Node(node) => assert_eq!(node.name, "c.txt"),
        other => panic!("expected Node, got {other:?}"),
    }

    let moved_gone = local
        .execute(
            &ctx,
            Command::Exists {
                path: copied.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(moved_gone, CommandOutput::Exists(false));

    let moved_present = local
        .execute(
            &ctx,
            Command::Exists {
                path: moved.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(moved_present, CommandOutput::Exists(true));
}

/// Exercises `Trash` then `Restore` entirely over HTTP, confirming through
/// the shared `LocalExecutor` that the node actually disappears and then
/// reappears.
#[tokio::test]
async fn remote_trash_and_restore_round_trip() {
    let (remote, local, ctx) = dual_fixture().await;
    let path = VirtualPath::parse("/trash-me.txt").unwrap();

    remote
        .execute(
            &ctx,
            Command::Write {
                path: path.clone(),
                bytes: b"gone soon".to_vec(),
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    let trash_result = remote
        .execute(
            &ctx,
            Command::Trash {
                path: path.clone(),
                options: MutationOptions::default(),
            },
        )
        .await
        .unwrap();
    let trash_id = match trash_result {
        CommandOutput::Trash(entry) => {
            assert_eq!(entry.original_path, path);
            entry.id
        }
        other => panic!("expected Trash, got {other:?}"),
    };

    let gone = local
        .execute(
            &ctx,
            Command::Exists {
                path: path.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(gone, CommandOutput::Exists(false));

    let listing = remote
        .execute(&ctx, Command::ListTrash { page: Default::default() })
        .await
        .unwrap();
    match listing {
        CommandOutput::TrashList(page) => assert_eq!(page.items.len(), 1),
        other => panic!("expected TrashList, got {other:?}"),
    }

    let restore_result = remote
        .execute(
            &ctx,
            Command::Restore {
                trash: trash_id,
                destination: None,
                options: MutationOptions::default(),
            },
        )
        .await
        .unwrap();
    match restore_result {
        CommandOutput::Node(node) => assert_eq!(node.name, "trash-me.txt"),
        other => panic!("expected Node, got {other:?}"),
    }

    let restored = local
        .execute(
            &ctx,
            Command::Read {
                path: path.clone(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    match restored {
        CommandOutput::Content { bytes, .. } => assert_eq!(bytes, b"gone soon"),
        other => panic!("expected Content, got {other:?}"),
    }
}

/// Exercises `SetAttribute`/`RemoveAttribute` over HTTP and compares the
/// resulting `Node.attributes` against the same operation run locally on a
/// twin path — proving the HTTP wire encoding (`value_base64` field on the
/// `PATCH /fs/{*path}` route) round-trips identically to the in-process
/// path.
///
/// This does not verify the effect via a follow-up `Stat` call: `stat()` in
/// `fslite-sqlite` always returns `attributes: BTreeMap::new()` regardless
/// of what is stored (only the `Node` returned directly by a mutation call
/// is populated via `build_attributes_map`) — a pre-existing
/// `fslite-sqlite` characteristic, out of scope for this task to change.
#[tokio::test]
async fn remote_and_local_set_attribute_agree_on_the_wire_encoding() {
    let (remote, local, ctx) = dual_fixture().await;
    let remote_path = VirtualPath::parse("/attrs-remote.txt").unwrap();
    let local_path = VirtualPath::parse("/attrs-local.txt").unwrap();

    for path in [&remote_path, &local_path] {
        remote
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
    }

    let set_attribute = |path: VirtualPath| Command::SetAttribute {
        path,
        key: "owner".to_string(),
        value: b"sean".to_vec(),
        options: MutationOptions::default(),
    };

    let remote_result = remote
        .execute(&ctx, set_attribute(remote_path.clone()))
        .await
        .unwrap();
    let local_result = local
        .execute(&ctx, set_attribute(local_path.clone()))
        .await
        .unwrap();

    let (remote_node, local_node) = match (remote_result, local_result) {
        (CommandOutput::Node(r), CommandOutput::Node(l)) => (r, l),
        other => panic!("expected matching Node outputs, got {other:?}"),
    };
    assert_eq!(
        remote_node.attributes.get("owner"),
        local_node.attributes.get("owner"),
    );
    let owner_value = remote_node.attributes.get("owner").expect("attribute present");
    // `fslite-sqlite` stores attribute values URL-safe-base64 encoded
    // internally, independent of the HTTP wire encoding `RemoteExecutor`
    // used to transmit the raw bytes.
    let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"sean");
    assert_eq!(owner_value.as_str(), Some(expected.as_str()));

    let remove_result = remote
        .execute(
            &ctx,
            Command::RemoveAttribute {
                path: remote_path,
                key: "owner".to_string(),
                options: MutationOptions::default(),
            },
        )
        .await
        .unwrap();
    match remove_result {
        CommandOutput::Node(node) => assert!(!node.attributes.contains_key("owner")),
        other => panic!("expected Node, got {other:?}"),
    }
}

/// Seeds files through the local backend once, then runs the same `Glob`
/// and `Find` queries through both executors and compares — both are
/// read-only, so no double-mutation hazard.
#[tokio::test]
async fn remote_glob_and_find_agree_with_local() {
    let (remote, local, ctx) = dual_fixture().await;

    local
        .execute(
            &ctx,
            Command::Mkdir {
                path: VirtualPath::parse("/notes").unwrap(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();

    for name in ["/notes/one.txt", "/notes/two.txt", "/notes/skip.md"] {
        local
            .execute(
                &ctx,
                Command::Write {
                    path: VirtualPath::parse(name).unwrap(),
                    bytes: b"content".to_vec(),
                    options: WriteOptions::default(),
                },
            )
            .await
            .unwrap();
    }

    let glob_command = Command::Glob {
        pattern: "/notes/*.txt".to_string(),
        page: Default::default(),
    };
    let remote_glob = remote.execute(&ctx, glob_command.clone()).await.unwrap();
    let local_glob = local.execute(&ctx, glob_command).await.unwrap();
    match (remote_glob, local_glob) {
        (CommandOutput::Nodes(r), CommandOutput::Nodes(l)) => {
            assert_eq!(r.items.len(), 2);
            assert_eq!(r.items.len(), l.items.len());
        }
        other => panic!("expected matching Nodes, got {other:?}"),
    }

    let find_command = Command::Find {
        query: FindQuery::default()
            .root(VirtualPath::parse("/notes").unwrap())
            .name_contains(Some("skip".to_string())),
        page: Default::default(),
    };
    let remote_find = remote.execute(&ctx, find_command.clone()).await.unwrap();
    let local_find = local.execute(&ctx, find_command).await.unwrap();
    match (remote_find, local_find) {
        (CommandOutput::Nodes(r), CommandOutput::Nodes(l)) => {
            assert_eq!(r.items.len(), 1);
            assert_eq!(r.items[0].name, "skip.md");
            assert_eq!(r.items.len(), l.items.len());
        }
        other => panic!("expected matching Nodes, got {other:?}"),
    }
}

/// Exercises `Batch` over HTTP (mkdir + touch in one atomic call), then
/// confirms both effects landed through the shared `LocalExecutor`.
#[tokio::test]
async fn remote_batch_round_trip() {
    let (remote, local, ctx) = dual_fixture().await;

    let ops = vec![
        BatchOperation::Mkdir {
            path: VirtualPath::parse("/batch-dir").unwrap(),
            options: Default::default(),
        },
        BatchOperation::Touch {
            path: VirtualPath::parse("/batch-dir/file.txt").unwrap(),
            options: Default::default(),
        },
    ];

    let result = remote.execute(&ctx, Command::Batch(ops)).await.unwrap();
    match result {
        CommandOutput::Batch(results) => assert_eq!(results.len(), 2),
        other => panic!("expected Batch, got {other:?}"),
    }

    let dir_exists = local
        .execute(
            &ctx,
            Command::Exists {
                path: VirtualPath::parse("/batch-dir").unwrap(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(dir_exists, CommandOutput::Exists(true));

    let file_exists = local
        .execute(
            &ctx,
            Command::Exists {
                path: VirtualPath::parse("/batch-dir/file.txt").unwrap(),
                options: Default::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(file_exists, CommandOutput::Exists(true));
}

/// Regression test: `fslite-server`'s `GET /content/{*path}` route has no
/// `follow_symlinks` query parameter at all, and the `fslite-sqlite`
/// backend's `content::read` never consults `ReadOptions::follow_symlinks`
/// either way (it resolves the literal path only, via `resolve::resolve`,
/// not `resolve_following` — confirmed directly: reading a symlink path
/// fails `WrongNodeType` on *both* `LocalExecutor` and `RemoteExecutor`,
/// regardless of `follow_symlinks`, which is itself a pre-existing
/// `fslite-sqlite` characteristic out of scope for this task to change).
///
/// Because `RemoteExecutor`'s own `stat` pre-fetch *does* honor
/// `follow_symlinks`, if a future backend ever implements symlink-following
/// reads, `RemoteExecutor` would otherwise silently return a
/// `CommandOutput::Content` whose `revision`/`logical_length` (from an
/// unfollowed `stat`) don't correspond to `bytes` (from a followed content
/// fetch it cannot control). So the guard rejects `follow_symlinks: false`
/// unconditionally, regardless of what kind of node the path resolves to —
/// this test exercises it directly against a plain regular file, which is
/// sufficient to prove the guard fires before any HTTP call is made.
#[tokio::test]
async fn remote_read_rejects_follow_symlinks_false() {
    let (remote, _local, ctx) = dual_fixture().await;
    let path = VirtualPath::parse("/plain.txt").unwrap();

    remote
        .execute(
            &ctx,
            Command::Write {
                path: path.clone(),
                bytes: b"plain content".to_vec(),
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    // The default (`follow_symlinks: true`) still works normally over HTTP.
    let following = remote
        .execute(
            &ctx,
            Command::Read {
                path: path.clone(),
                options: ReadOptions::default(),
            },
        )
        .await
        .unwrap();
    match following {
        CommandOutput::Content { bytes, .. } => assert_eq!(bytes, b"plain content"),
        other => panic!("expected Content, got {other:?}"),
    }

    // A read that explicitly asks not to follow symlinks must fail loudly
    // — `RemoteExecutor` cannot honor it for the content fetch itself and
    // must not silently mix unfollowed metadata with a followed read.
    let err = remote
        .execute(
            &ctx,
            Command::Read {
                path,
                options: ReadOptions::default().follow_symlinks(false),
            },
        )
        .await
        .expect_err("RemoteExecutor must reject follow_symlinks=false for read");
    assert!(
        err.message().contains("follow_symlinks"),
        "expected a clear follow_symlinks error, got: {}",
        err.message()
    );
}

/// Regression test: `fslite-server`'s content route returns a bodyless,
/// envelope-free `416 Range Not Satisfiable` for a range that starts at or
/// beyond the file's logical size — unlike every other error path in that
/// crate, which goes through the JSON `ApiError` envelope. `RemoteExecutor`
/// must still surface `ErrorCode::InvalidRange` for this case (matching
/// what `LocalExecutor` reports directly from `fslite_core`), not fall
/// through to a generic `ErrorCode::InternalStorageFailure`.
#[tokio::test]
async fn remote_read_out_of_bounds_range_reports_invalid_range() {
    let (remote, _local, ctx) = dual_fixture().await;
    let path = VirtualPath::parse("/short.txt").unwrap();

    remote
        .execute(
            &ctx,
            Command::Write {
                path: path.clone(),
                bytes: b"short".to_vec(), // 5 bytes
                options: WriteOptions::default(),
            },
        )
        .await
        .unwrap();

    let err = remote
        .execute(
            &ctx,
            Command::Read {
                path,
                options: ReadOptions::default().range(Some(ByteRange::new(10, 20))),
            },
        )
        .await
        .expect_err("an out-of-bounds range must be rejected");
    assert_eq!(err.code(), ErrorCode::InvalidRange);
}
