//! Atomic multi-operation batches via `SqliteFileSystem::batch`: every
//! operation in a batch commits together, or none do.
//!
//! Run with `cargo run --example batch`.

use fslite_core::{
    BatchOperation, BatchResult, CreateOptions, RemoveOptions, RequestContext, VirtualPath,
};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let workspace = fs.create_workspace(Default::default()).await?;
    let ctx = RequestContext::trusted(workspace.id);

    // This batch fails partway: /reports/2026 is created, but removing
    // /reports non-recursively fails because it still has that child.
    // Nothing commits — not even the first Mkdir.
    let doomed_ops = vec![
        BatchOperation::Mkdir {
            path: VirtualPath::parse("/reports")?,
            options: CreateOptions::default(),
        },
        BatchOperation::Mkdir {
            path: VirtualPath::parse("/reports/2026")?,
            options: CreateOptions::default(),
        },
        BatchOperation::Remove {
            path: VirtualPath::parse("/reports")?,
            options: RemoveOptions::default(),
        },
    ];
    match fs.batch(&ctx, doomed_ops).await {
        Ok(_) => println!("unexpected: doomed batch committed"),
        Err(err) => println!("batch aborted as expected: {err}"),
    }

    let reports_exists = fs
        .exists(&ctx, &VirtualPath::parse("/reports")?, Default::default())
        .await?;
    println!("/reports exists after the aborted batch: {reports_exists}");

    // A batch that only creates directories always commits as a whole.
    let ops = vec![
        BatchOperation::Mkdir {
            path: VirtualPath::parse("/reports")?,
            options: CreateOptions::default(),
        },
        BatchOperation::Mkdir {
            path: VirtualPath::parse("/reports/2026")?,
            options: CreateOptions::default(),
        },
    ];
    let results = fs.batch(&ctx, ops).await?;
    for result in &results {
        match result {
            BatchResult::Node(node) => println!("created directory: {}", node.name),
            BatchResult::Trash(entry) => println!("trashed: {}", entry.original_path),
            BatchResult::Unit => println!("ok"),
        }
    }

    let reports_exists = fs
        .exists(
            &ctx,
            &VirtualPath::parse("/reports/2026")?,
            Default::default(),
        )
        .await?;
    println!("/reports/2026 exists after the committed batch: {reports_exists}");

    Ok(())
}
