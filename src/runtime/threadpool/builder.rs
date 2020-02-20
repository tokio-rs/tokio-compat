use super::{compat, Inner, Runtime};

use std::{
    io,
    sync::{Arc, RwLock},
};
use std::fmt;
use tokio_02::runtime;
use tokio_timer_02::clock as clock_02;

type Callback = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Builds a compatibility runtime with custom configuration values.
///
/// This runtime is compatible with code using both the current release version
/// of `tokio` (0.1) and with legacy code using `tokio` 0.1.
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
///
/// use tokio_compat::runtime::Builder;
/// use tokio_timer_02::clock::Clock;
///
/// fn main() {
///     // build Runtime
///     let runtime = Builder::new()
///         .clock(Clock::system())
///         .core_threads(4)
///         .name_prefix("my-custom-name-")
///         .stack_size(3 * 1024 * 1024)
///         .build()
///         .unwrap();
///
///     // use runtime ...
/// }
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "rt-full")))]
pub struct Builder {
    inner: runtime::Builder,
    clock: clock_02::Clock,

    /// Callback to run after each thread starts.
    pub(super) after_start: Option<Callback>,

    /// To run before each worker thread stops
    pub(super) before_stop: Option<Callback>,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {
            clock: clock_02::Clock::system(),
            inner: runtime::Builder::new(),
            // No worker thread callbacks
            after_start: None,
            before_stop: None,
        }
    }

    /// Set the `Clock` instance that will be used by the runtime's legacy timer.
    pub fn clock(&mut self, clock: clock_02::Clock) -> &mut Self {
        self.clock = clock;
        self
    }

    /// Set the maximum number of worker threads for the `Runtime`'s thread pool.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .core_threads(4)
    ///     .build()
    ///     .unwrap();
    /// # }
    /// ```
    pub fn core_threads(&mut self, val: usize) -> &mut Self {
        // the deprecation warning states that this method will be replaced, but
        // the method that will replace it doesn't exist yet...
        #[allow(deprecated)]
        self.inner.num_threads(val);
        self
    }

    /// Set name prefix of threads spawned by the `Runtime`'s thread pool.
    ///
    /// Thread name prefix is used for generating thread names. For example, if
    /// prefix is `my-pool-`, then threads in the pool will get names like
    /// `my-pool-1` etc.
    ///
    /// The default prefix is "tokio-runtime-worker-".
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// # }
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.inner.thread_name(val);
        self
    }

    /// Executes function `f` after each thread is started but before it starts
    /// doing work.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new()
    ///     .on_thread_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn on_thread_start<F>(&mut self, f: F) -> &mut Self
        where
            F: Fn() + Send + Sync + 'static,
    {
        self.after_start = Some(Arc::new(f));
        self
    }

    /// Executes function `f` before each thread stops.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new()
    ///     .on_thread_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn on_thread_stop<F>(&mut self, f: F) -> &mut Self
        where
            F: Fn() + Send + Sync + 'static,
    {
        self.before_stop = Some(Arc::new(f));
        self
    }

    /// Set the stack size (in bytes) for worker threads.
    ///
    /// The actual stack size may be greater than this value if the platform
    /// specifies minimal stack size.
    ///
    /// The default stack size for spawned threads is 2 MiB, though this
    /// particular stack size is subject to change in the future.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.inner.thread_stack_size(val);
        self
    }

    /// Create the configured `Runtime`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_compat::runtime::Builder;
    /// # pub fn main() {
    /// let runtime = Builder::new().build().unwrap();
    /// // ... call runtime.run(...)
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        let compat_bg = compat::Background::spawn(&self.clock)?;
        let compat_timer = compat_bg.timer().clone();
        let compat_reactor = compat_bg.reactor().clone();
        let compat_sender: Arc<RwLock<Option<super::CompatSpawner<tokio_02::runtime::Handle>>>> =
            Arc::new(RwLock::new(None));

        // We need a weak ref here so that when we pass this into the
        // `on_thread_start` closure it's stored as a _weak_ ref and not
        // a strong one. This is important because tokio 0.2's runtime
        // should shut down when the compat Runtime has been dropped. There
        // can be a race condition where we hold onto an extra Arc, which holds
        // a runtime handle beyond the drop. This causes the tokio 0.2 runtime to
        // not shut down and leak fds.
        //
        // Tokio 0.2's threaded_scheduler will spawn threads that each contain an arced
        // ref to the `on_thread_start` fn. If the runtime shuts down but there is still
        // access to a runtime handle, the mio driver will not shutdown. To avoid this we
        // only want the `on_thread_start` to hold a weak ref and attempt to upgrade, since
        // the runtime will still be acitve the upgrade should work. Otherwise, somehow
        // tokio started a new thread after its runtime has been dropped.
        let compat_sender2 = Arc::downgrade(&compat_sender);
        let mut lock = compat_sender.write().unwrap();

        let after_start = self.after_start.clone();
        let before_stop = self.before_stop.clone();

        let runtime = self
            .inner
            .threaded_scheduler()
            .enable_all()
            .on_thread_start(move || {
                match &after_start {
                    Some(callback) => callback(),
                    None => {},
                }
                // We need the threadpool's sender to set up the default tokio
                // 0.1 executor. We also need to upgrade the weak pointer. If the
                // pointer is no longer valid, then the runtime has shut down and the
                // handle is no longer available.
                //
                // This upgrade will only fail if the compat runtime has been dropped.
                let sender = compat_sender2
                    .upgrade()
                    .expect("Runtime dropped but thread started; this is a bug!");
                let lock = sender.read().unwrap();
                let compat_sender = lock
                    .as_ref()
                    .expect("compat executor needs to be set before the pool is run!")
                    .clone();
                compat::set_guards(compat_sender, &compat_timer, &compat_reactor);
            })
            .on_thread_stop(move || {
                match &before_stop {
                    Some(callback) => callback(),
                    None => {},
                }
                compat::unset_guards();
            })
            .build()?;

        let (idle, idle_rx) = super::idle::Idle::new();
        // Set the tokio 0.1 executor to be used by the worker threads.
        *lock = Some(super::CompatSpawner::new(runtime.handle().clone(), &idle));
        drop(lock);
        let runtime = Runtime {
            inner: Some(Inner { runtime, compat_bg }),
            idle_rx,
            idle,
            compat_sender,
        };

        Ok(runtime)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("inner", &self.inner)
            .field("clock", &self.clock)
            .finish()
    }
}