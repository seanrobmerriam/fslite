use fslite_sqlite::SqliteFileSystem;

#[tokio::test]
async fn schema_version_matches_latest_after_open() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    assert_eq!(
        fs.schema_version().await.unwrap(),
        SqliteFileSystem::latest_schema_version()
    );
}

#[tokio::test]
async fn schema_version_survives_reopen_of_a_file_backed_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("health.db");
    {
        let fs = SqliteFileSystem::open(&path, Default::default())
            .await
            .unwrap();
        assert_eq!(
            fs.schema_version().await.unwrap(),
            SqliteFileSystem::latest_schema_version()
        );
    }
    let fs = SqliteFileSystem::open(&path, Default::default())
        .await
        .unwrap();
    assert_eq!(
        fs.schema_version().await.unwrap(),
        SqliteFileSystem::latest_schema_version()
    );
}

#[tokio::test]
async fn integrity_check_reports_no_problems_on_a_healthy_database() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    assert_eq!(fs.integrity_check().await.unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn integrity_check_reports_problems_on_a_corrupted_database() {
    use fslite_core::{RequestContext, VirtualPath, WriteSource};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    {
        // Write enough content to span many 4 KiB pages, then let the
        // connection drop (WAL auto-checkpoints on last-connection-close),
        // so every page's real content lives in the main file, not the WAL.
        let fs = SqliteFileSystem::open(&path, Default::default())
            .await
            .unwrap();
        let workspace = fs.create_workspace(Default::default()).await.unwrap();
        let ctx = RequestContext::trusted(workspace.id);
        fs.write(
            &ctx,
            &VirtualPath::parse("/big.bin").unwrap(),
            WriteSource::from_bytes(vec![7u8; 200_000]),
            Default::default(),
        )
        .await
        .unwrap();
    }

    // `fs`'s connection runs in WAL mode, where new pages are written to a
    // separate `-wal` file first and only merged into the main `.db` file
    // at checkpoint time; when that checkpoint happens relative to the
    // async connection's drop above is not guaranteed. Force it explicitly
    // through a fresh, synchronous connection so the corruption below
    // always lands in the main file, deterministically.
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "wal_checkpoint", "TRUNCATE")
        .unwrap();

    // Overwrite one whole page well past the header/schema pages with
    // garbage, corrupting that page's B-tree structure. Basic queries
    // (like the `sqlite_master`/`schema_migrations` reads `open` performs)
    // only touch early pages, so `open` below still succeeds — this
    // exercises `integrity_check`'s full-database walk specifically,
    // as opposed to `open` failing outright on a header-level corruption.
    let mut bytes = std::fs::read(&path).unwrap();
    const PAGE_SIZE: usize = 4096;
    let page_count = bytes.len() / PAGE_SIZE;
    let target_page = page_count * 2 / 3;
    let start = target_page * PAGE_SIZE;
    for byte in bytes.iter_mut().skip(start).take(PAGE_SIZE) {
        *byte = 0xFF;
    }
    std::fs::write(&path, bytes).unwrap();

    let fs = SqliteFileSystem::open(&path, Default::default())
        .await
        .unwrap();
    assert!(!fs.integrity_check().await.unwrap().is_empty());
}
