use futures_01::{future::Future as Future01, sync::oneshot};
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::{cell::RefCell, io, thread};
use tokio_02::runtime::Handle;
use tokio_executor_01 as executor_01;
use tokio_reactor_01 as reactor_01;
use tokio_timer_02::{clock as clock_02, timer as timer_02};

use super::idle;

#[derive(Debug)]
pub(super) struct Background {
    reactor_handle: reactor_01::Handle,
    timer_handle: timer_02::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Clone, Debug)]
pub(super) struct CompatSpawner<S> {
    pub(super) inner: S,
    pub(super) idle: idle::Idle,
}

struct CompatGuards {
    _executor: executor_01::DefaultGuard,
    _timer: timer_02::DefaultGuard,
    _reactor: reactor_01::DefaultGuard,
}

thread_local! {
    static COMPAT_GUARDS: RefCell<Option<CompatGuards>> = RefCell::new(None);
}

pub(super) fn set_guards(
    executor: impl executor_01::Executor + 'static,
    timer: &timer_02::Handle,
    reactor: &reactor_01::Handle,
) {
    let guards = CompatGuards {
        _executor: executor_01::set_default(executor),
        _timer: timer_02::set_default(timer),
        _reactor: reactor_01::set_default(reactor),
    };

    COMPAT_GUARDS.with(move |current| {
        let prev = current.borrow_mut().replace(guards);
        assert!(
            prev.is_none(),
            "default tokio 0.1 runtime set twice; this is a bug"
        );
    });
}

pub(super) fn unset_guards() {
    let _ = COMPAT_GUARDS.try_with(move |current| {
        drop(current.borrow_mut().take());
    });
}

impl Background {
    pub(super) fn spawn(clock: &clock_02::Clock) -> io::Result<Self> {
        let reactor = reactor_01::Reactor::new()?;
        let reactor_handle = reactor.handle();

        let timer = timer_02::Timer::new_with_now(reactor, clock.clone());
        let timer_handle = timer.handle();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_tx = Some(shutdown_tx);

        let thread = thread::spawn(move || {
            let mut rt = tokio_current_thread_01::CurrentThread::new_with_park(timer);
            let _ = rt.block_on(shutdown_rx);
        });
        let thread = Some(thread);

        Ok(Self {
            reactor_handle,
            timer_handle,
            thread,
            shutdown_tx,
        })
    }

    pub(super) fn reactor(&self) -> &reactor_01::Handle {
        &self.reactor_handle
    }

    pub(super) fn timer(&self) -> &timer_02::Handle {
        &self.timer_handle
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().unwrap().send(());
        let _ = self.thread.take().unwrap().join();
    }
}

// === impl CompatSpawner ===

impl<T> CompatSpawner<T> {
    pub(super) fn new(inner: T, idle: &idle::Idle) -> Self {
        Self {
            inner,
            idle: idle.clone(),
        }
    }
}

impl<'a> executor_01::Executor for CompatSpawner<&'a Handle> {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        let future = future.compat().map(|_| ());
        let idle = self.idle.reserve();
        self.inner.spawn(idle.with(future));
        Ok(())
    }
}

impl<'a, T> executor_01::TypedExecutor<T> for CompatSpawner<&'a Handle>
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        let idle = self.idle.reserve();
        let future = Box::pin(idle.with(future.compat().map(|_| ())));
        // Use the `tokio` 0.2 `TypedExecutor` impl so we don't have to box the
        // future twice (once to spawn it using `Executor01::spawn` and a second
        // time to pin the compat future).
        self.inner.spawn(future);
        Ok(())
    }
}

impl executor_01::Executor for CompatSpawner<Handle> {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        let future = future.compat().map(|_| ());
        let idle = self.idle.reserve();
        self.inner.spawn(idle.with(future));
        Ok(())
    }
}

impl<T> executor_01::TypedExecutor<T> for CompatSpawner<Handle>
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        let idle = self.idle.reserve();
        let future = Box::pin(idle.with(future.compat().map(|_| ())));
        // Use the `tokio` 0.2 `TypedExecutor` impl so we don't have to box the
        // future twice (once to spawn it using `Executor01::spawn` and a second
        // time to pin the compat future).
        self.inner.spawn(future);
        Ok(())
    }
}
