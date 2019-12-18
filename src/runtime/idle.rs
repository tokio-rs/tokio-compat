use std::future::Future;
use std::sync::{
    atomic::{fence, AtomicUsize, Ordering},
    Arc,
};
use tokio_02::sync::mpsc;

#[derive(Debug)]
pub(super) struct Rx {
    rx: mpsc::UnboundedReceiver<()>,
    spawned: Arc<AtomicUsize>,
}

/// Tracks the number of tasks spawned on a runtime.
///
/// This is required to implement `shutdown_on_idle` and `tokio::run` APIs that
/// exist in `tokio` 0.1, as the `tokio` 0.2 threadpool does not expose a
/// `shutdown_on_idle` API.
#[derive(Clone, Debug)]
pub(super) struct Idle {
    tx: mpsc::UnboundedSender<()>,
    spawned: Arc<AtomicUsize>,
}

/// Wraps a future to decrement the spawned count when it completes.
///
/// This is obtained from `Idle::reserve`.
pub(super) struct Track(Idle);

impl Idle {
    pub(super) fn new() -> (Self, Rx) {
        let (tx, rx) = mpsc::unbounded_channel();
        let this = Self {
            tx,
            spawned: Arc::new(AtomicUsize::new(0)),
        };
        let rx = Rx {
            rx,
            spawned: this.spawned.clone(),
        };
        (this, rx)
    }

    /// Prepare to spawn a task on the runtime, incrementing the spawned count.
    pub(super) fn reserve(&self) -> Track {
        self.spawned.fetch_add(1, Ordering::Relaxed);
        Track(self.clone())
    }
}

impl Rx {
    pub(super) async fn idle(&mut self) {
        while self.spawned.load(Ordering::Acquire) != 0 {
            // Wait to be woken up again.
            let _ = self.rx.recv().await;
        }
    }
}

impl Track {
    /// Run a task, decrementing the spawn count when it completes.
    ///
    /// If the spawned count is now 0, this sends a notification on the idle channel.
    pub(super) async fn with<T>(self, f: impl Future<Output = T>) -> T {
        let result = f.await;
        let spawned = self.0.spawned.fetch_sub(1, Ordering::Release);
        if spawned == 1 {
            fence(Ordering::Acquire);
            let _ = self.0.tx.send(());
        }
        result
    }
}
