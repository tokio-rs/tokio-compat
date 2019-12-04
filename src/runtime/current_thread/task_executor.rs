use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, FutureExt};
use std::future::Future;
use tokio_executor_01::{self as executor_01, Executor as Executor01};

#[derive(Debug)]
pub struct TaskExecutor {
    _p: (),
}

// ===== impl TaskExecutor =====

impl TaskExecutor {
    /// Returns an executor that executes futures on the current thread.
    ///
    /// The user of `TaskExecutor` must ensure that when a future is submitted,
    /// that it is done within the context of a call to `run`.
    ///
    /// For more details, see the [module level](index.html) documentation.
    pub fn current() -> TaskExecutor {
        TaskExecutor { _p: () }
    }

    /// Spawn a future onto the current `CurrentThread` instance.
    pub fn spawn_local(
        &mut self,
        future: Box<dyn Future01<Item = (), Error = ()>>,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local_std(future.compat().map(|_| ()))
    }

    pub fn spawn_local_std(
        &mut self,
        future: impl Future<Output = ()> + 'static,
    ) -> Result<(), executor_01::SpawnError> {
        if super::runtime::is_current() {
            tokio_02::task::spawn_local(future);
            Ok(())
        } else {
            Err(executor_01::SpawnError::shutdown())
        }
    }
}

impl tokio_executor_01::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<Future01<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local(future)
    }
}

impl<F> tokio_executor_01::TypedExecutor<F> for TaskExecutor
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), executor_01::SpawnError> {
        self.spawn_local(Box::new(future))
    }
}
