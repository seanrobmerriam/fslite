//! Shared test-only helpers for tests that mutate process-global env vars
//! (`FSLITE_CONFIG_DIR`, `XDG_CONFIG_HOME`).
//!
//! `paths.rs`, `registry.rs`, and `context.rs` each have unit tests that
//! call `std::env::set_var`/`remove_var` on these vars so they can point
//! `config_dir()` at an isolated temp directory. Cargo runs every test in
//! this binary's single test binary in parallel threads by default, and
//! env vars are process-global state — so without serialization, one
//! test's `set_var`/`remove_var` races with another test's, and a test can
//! observe a config dir it never set (intermittent, order-dependent
//! failures).
//!
//! [`lock`] hands back a guard on a single process-wide mutex; callers must
//! hold it for the *entire* test body (not just around the `set_var`
//! calls), since the whole point is serializing anything that reads
//! config-dir-derived state while another test's env vars are mid-flight.
//! [`with_temp_config_dir`] is the common case: acquire the lock, point
//! `FSLITE_CONFIG_DIR` at a fresh temp directory for the duration of `f`,
//! then clean up.

use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_mutex() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquires the process-wide env-var lock, recovering from a poisoned lock
/// (a panic in another test while it held the guard) instead of cascading
/// that panic into every other env-var test.
pub fn lock() -> MutexGuard<'static, ()> {
    env_mutex()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Runs `f` while `FSLITE_CONFIG_DIR` is set to a fresh temp directory,
/// holding the env-var lock for the full duration so no other
/// env-var-mutating test can interleave.
pub fn with_temp_config_dir<T>(f: impl FnOnce() -> T) -> T {
    let _guard = lock();
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: serialized by the lock above — no other test can observe or
    // mutate FSLITE_CONFIG_DIR/XDG_CONFIG_HOME while this guard is held.
    unsafe {
        std::env::set_var("FSLITE_CONFIG_DIR", dir.path());
    }
    let result = f();
    unsafe {
        std::env::remove_var("FSLITE_CONFIG_DIR");
    }
    result
}
