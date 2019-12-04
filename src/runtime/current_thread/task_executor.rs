use tokio_executor_01::{self as executor_01, Executor as Executor01};

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
        future: Box<Future01<Item = (), Error = ()>>,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local_std(future.compat().map(|_| ()))
    }

    pub fn spawn_local_std(
        &mut self,
        f: impl Future<Output = ()>,
    ) -> Result<(), executor_01::SpawnError> {
        if super::runtime::is_current() {
            tokio_02::task::spawn_local(future);
            Ok(())
        } else {
            Err(executor_01::SpawnError::shutdown())
        }
    }
}

impl tokio_executor::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        self.spawn_local(future)
    }
}

impl<F> tokio_executor::TypedExecutor<F> for TaskExecutor
where
    F: Future<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), SpawnError> {
        self.spawn_local(Box::new(future))
    }
}
