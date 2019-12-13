use crate::runtime::compat;
use crate::runtime::current_thread::Runtime;
use std::{
    io,
    sync::{Arc, RwLock},
};
use tokio_02::runtime;
use tokio_timer_02::clock::Clock;

/// Builds a single-threaded compatibility runtime with custom configuration
/// values.
///
/// Methods can be chained in order to set the configuration values. The
/// Runtime is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
/// See function level documentation for details on the various configuration
/// settings.
///
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// use tokio_compat::runtime::current_thread::Builder;
/// use tokio_timer_02::clock::Clock;
///
/// # pub fn main() {
/// // build Runtime
/// let runtime = Builder::new()
///     .clock(Clock::new())
///     .build();
/// // ... call runtime.run(...)
/// # let _ = runtime;
/// # }
/// ```
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "rt-current-thread")))]
pub struct Builder {
    /// The clock to use
    clock: Clock,
    inner: runtime::Builder,
}

impl Builder {
    /// Returns a new compatibility runtime builder initialized with default
    /// configuration values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {
            clock: Clock::system(),
            inner: runtime::Builder::new(),
        }
    }

    /// Set the `Clock` instance that will be used by the runtime.
    pub fn clock(&mut self, clock: Clock) -> &mut Self {
        self.clock = clock;
        self
    }

    /// Create the configured `Runtime`.
    pub fn build(&mut self) -> io::Result<Runtime> {
        let compat_bg = compat::Background::spawn(&self.clock)?;
        let compat_timer = compat_bg.timer().clone();
        let compat_reactor = compat_bg.reactor().clone();
        let compat_sender: Arc<RwLock<Option<compat::CompatSpawner<tokio_02::runtime::Handle>>>> =
            Arc::new(RwLock::new(None));
        let compat_sender2 = compat_sender.clone();
        let mut lock = compat_sender.write().unwrap();

        let runtime = self
            .inner
            .basic_scheduler()
            .enable_all()
            .on_thread_start(move || {
                // We need the threadpool's sender to set up the default tokio
                // 0.1 executor.
                let lock = compat_sender2.read().unwrap();
                let compat_sender = lock
                    .as_ref()
                    .expect("compat executor needs to be set before runtime is run!")
                    .clone();
                compat::set_guards(compat_sender, &compat_timer, &compat_reactor);
            })
            .on_thread_stop(|| {
                compat::unset_guards();
            })
            .build()?;

        let (idle, idle_rx) = super::idle::Idle::new();
        // Set the tokio 0.1 executor to be used by the worker threads.
        *lock = Some(compat::CompatSpawner::new(runtime.handle().clone(), &idle));
        drop(lock);

        Ok(Runtime::new2(runtime, idle, idle_rx, compat_bg))
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
