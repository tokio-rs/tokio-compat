use super::{
    // background,
    compat,
    Inner,
    Runtime,
};

// use tokio_02::net::driver::{self, Reactor};
use tokio_02::runtime;
// use tokio_02::timer::clock::{self, Clock};
// use tokio_02::timer::timer::{self, Timer};

// #[cfg(feature = "blocking")]
// use tokio_threadpool_01::blocking as blocking_01;
use std::io;
use std::sync::{Arc, RwLock};

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
/// use tokio_02::timer::clock::Clock;
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
#[derive(Debug)]
pub struct Builder {
    // num_threads: usize,
    inner: runtime::Builder,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        // let num_threads = num_cpus::get().max(1);
        Builder {
            // num_threads,
            inner: runtime::Builder::new(),
        }
    }

    // /// Set the `Clock` instance that will be used by the runtime.
    // pub fn clock(&mut self, clock: Clock) -> &mut Self {
    //     self.inner.clock(clock.clone());
    //     self.clock = clock;
    //     self
    // }

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
        let compat_bg = compat::Background::spawn()?;
        let compat_timer = compat_bg.timer().clone();
        let compat_reactor = compat_bg.reactor().clone();
        let compat_sender: Arc<RwLock<Option<super::CompatSpawner<tokio_02::runtime::Handle>>>> =
            Arc::new(RwLock::new(None));
        let compat_sender2 = compat_sender.clone();
        let mut lock = compat_sender.write().unwrap();

        let runtime = self
            .inner
            .threaded_scheduler()
            .enable_all()
            .on_thread_start(move || {
                // We need the threadpool's sender to set up the default tokio
                // 0.1 executor.
                let lock = compat_sender2.read().unwrap();
                let compat_sender = lock
                    .as_ref()
                    .expect("compat executor needs to be set before the pool is run!")
                    .clone();
                compat::set_guards(compat_sender, &compat_timer, &compat_reactor);
            })
            .on_thread_stop(|| {
                compat::unset_guards();
            })
            .build()?;

        let (idle, idle_rx) = super::idle::Idle::new();
        // Set the tokio 0.1 executor to be used by the worker threads.
        *lock = Some(super::CompatSpawner {
            inner: runtime.handle().clone(),
            idle: idle.clone(),
        });
        drop(lock);
        let runtime = Runtime {
            inner: Some(Inner { runtime, compat_bg }),
            idle_rx,
            idle,
        };

        Ok(runtime)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
