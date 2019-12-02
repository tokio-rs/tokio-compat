use tokio_executor_01::{self as executor_01};
use tokio_reactor_01 as reactor_01;
use tokio_timer_02::{clock as clock_02, timer as timer_02};

use futures_01::sync::oneshot;
use std::{cell::RefCell, io, thread};

#[derive(Debug)]
pub(super) struct Background {
    reactor_handle: reactor_01::Handle,
    timer_handle: timer_02::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
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

// pub(super) fn spawn_err(new: tokio_02::executor::SpawnError) -> executor_01::SpawnError {
//     match new {
//         _ if new.is_shutdown() => executor_01::SpawnError::shutdown(),
//         _ if new.is_at_capacity() => executor_01::SpawnError::at_capacity(),
//         e => unreachable!("weird spawn error {:?}", e),
//     }
// }

// impl<P> park::Park for CompatPark<P>
// where
//     P: park_01::Park,
// {
//     type Unpark = CompatPark<P::Unpark>;
//     type Error = P::Error;

//     fn unpark(&self) -> Self::Unpark {
//         CompatPark(self.0.unpark())
//     }

//     #[inline]
//     fn park(&mut self) -> Result<(), Self::Error> {
//         self.0.park()
//     }

//     #[inline]
//     fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
//         self.0.park_timeout(duration)
//     }
// }

// impl<U> park::Unpark for CompatPark<U>
// where
//     U: park_01::Unpark,
// {
//     #[inline]
//     fn unpark(&self) {
//         self.0.unpark()
//     }
// }
