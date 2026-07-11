//! Session-scoped resource cleanup registry (WebSocket sessions, caches, etc.).

use std::sync::{Arc, Mutex, OnceLock};

type SessionResourceCleanup = Arc<dyn Fn(Option<&str>) + Send + Sync>;

fn cleanups() -> &'static Mutex<Vec<SessionResourceCleanup>> {
    static CLEANUPS: OnceLock<Mutex<Vec<SessionResourceCleanup>>> = OnceLock::new();
    CLEANUPS.get_or_init(|| Mutex::new(Vec::new()))
}

fn lock_cleanups() -> std::sync::MutexGuard<'static, Vec<SessionResourceCleanup>> {
    // Prefer recovering a poisoned registry over permanently losing cleanup hooks.
    // DeepWiki/Tokio guidance: std::sync::Mutex is fine for short critical sections that
    // never hold across `.await`; poison recovery keeps session teardown reliable.
    match cleanups().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// RAII handle that unregisters a cleanup callback when dropped.
pub struct SessionResourceCleanupRegistration {
    cleanup: Option<SessionResourceCleanup>,
}

impl SessionResourceCleanupRegistration {
    /// Unregister early (same as drop).
    pub fn unregister(mut self) {
        self.unregister_inner();
    }

    fn unregister_inner(&mut self) {
        let Some(cleanup) = self.cleanup.take() else {
            return;
        };
        let mut guard = lock_cleanups();
        guard.retain(|item| !Arc::ptr_eq(item, &cleanup));
    }
}

impl Drop for SessionResourceCleanupRegistration {
    fn drop(&mut self) {
        self.unregister_inner();
    }
}

/// Register a cleanup callback. Drop the returned registration to unregister.
pub fn register_session_resource_cleanup<F>(cleanup: F) -> SessionResourceCleanupRegistration
where
    F: Fn(Option<&str>) + Send + Sync + 'static,
{
    let cleanup: SessionResourceCleanup = Arc::new(cleanup);
    {
        let mut guard = lock_cleanups();
        guard.push(Arc::clone(&cleanup));
    }
    SessionResourceCleanupRegistration { cleanup: Some(cleanup) }
}

/// Run all registered session resource cleanups.
///
/// When `session_id` is `Some`, only resources for that session should be cleaned.
/// When `None`, clean everything.
///
/// Callbacks are snapshotted under the lock and invoked after release so a cleanup
/// that re-enters the registry cannot deadlock.
pub fn cleanup_session_resources(session_id: Option<&str>) {
    let callbacks: Vec<SessionResourceCleanup> = {
        let guard = lock_cleanups();
        guard.clone()
    };
    for cleanup in callbacks {
        cleanup(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cleanup_invokes_registered_callbacks() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_cb = Arc::clone(&count);
        let registration = register_session_resource_cleanup(move |_| {
            count_cb.fetch_add(1, Ordering::SeqCst);
        });
        cleanup_session_resources(Some("s1"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(registration);
        // After unregister, further cleanups must not see this callback.
        let before = count.load(Ordering::SeqCst);
        cleanup_session_resources(Some("s2"));
        assert_eq!(count.load(Ordering::SeqCst), before);
    }
}
