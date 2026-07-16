//! The tokio ↔ gpui bridge.
//!
//! sqlx runs on a tokio runtime; gpui has its own executor. Views dispatch an
//! async `Host` call here: it runs on the shared tokio runtime, and the result
//! is delivered to `done` on the gpui main thread via a runtime-agnostic
//! oneshot. This is the single seam every DB dispatch goes through.

use std::future::Future;
use std::sync::OnceLock;

use futures::channel::mpsc;
use futures::StreamExt;
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

/// Run a streaming producer on the tokio runtime, delivering each item to
/// `on_item` on the gpui main thread as it arrives, then `on_done` when the
/// producer finishes. The producer is handed the sender and streams items into
/// it. Used by the assistant panel to render tokens live.
pub fn stream<T, Fut>(
    cx: &mut App,
    producer: impl FnOnce(mpsc::UnboundedSender<T>) -> Fut + Send + 'static,
    mut on_item: impl FnMut(T, &mut App) + 'static,
    on_done: impl FnOnce(&mut App) + 'static,
) where
    T: Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = mpsc::unbounded();
    runtime().spawn(producer(tx));
    cx.spawn(async move |cx| {
        while let Some(item) = rx.next().await {
            if cx.update(|cx| on_item(item, cx)).is_err() {
                return;
            }
        }
        let _ = cx.update(|cx| on_done(cx));
    })
    .detach();
}
