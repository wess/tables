//! The tokio ↔ gpui bridge.
//!
//! sqlx runs on a tokio runtime; gpui has its own executor. Views dispatch an
//! async `Host` call here: it runs on the shared tokio runtime, and the result
//! is delivered to `done` on the gpui main thread via a runtime-agnostic
//! oneshot. This is the single seam every DB dispatch goes through.

use std::future::Future;
use std::sync::OnceLock;

use gpui::App;
use tokio::runtime::Runtime;

/// The process-wide tokio runtime, created on first use.
pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime")
    })
}

/// Run `fut` on the tokio runtime, then `done` on the gpui main thread.
pub fn run<T: Send + 'static>(
    cx: &mut App,
    fut: impl Future<Output = T> + Send + 'static,
    done: impl FnOnce(T, &mut App) + 'static,
) {
    let (tx, rx) = futures::channel::oneshot::channel();
    runtime().spawn(async move {
        let _ = tx.send(fut.await);
    });
    cx.spawn(async move |cx| {
        if let Ok(result) = rx.await {
            let _ = cx.update(|cx| done(result, cx));
        }
    })
    .detach();
}
