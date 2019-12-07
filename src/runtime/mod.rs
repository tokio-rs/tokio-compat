//! Runtimes compatible with both `tokio` 0.1 and `tokio` 0.2 futures.
//!
//! This module is similar to the [`tokio::runtime`] module, with one
//! key difference: the runtimes in this crate are capable of executing
//! both `futures` 0.1 futures that use the `tokio` 0.1 runtime services
//! (i.e. `timer`, `reactor`, and `executor`), **and** `std::future`
//! futures that use the `tokio` 0.2 runtime services.
//!
//! The `futures` crate's [`compat` module][futures-compat] provides
//! interoperability between `futures` 0.1 and `std::future` _future types_
//! (e.g. implementing `std::future::Future` for a type that implements the
//! `futures` 0.1 `Future` trait). However, this on its own is insufficient to
//! run code written against `tokio` 0.1 on a `tokio` 0.2 runtime, if that code
//! also relies on `tokio`'s runtime services. If legacy tasks are executed that
//! rely on `tokio::timer`, perform IO using `tokio`'s reactor, or call
//! `tokio::spawn`, those API calls will fail unless there is also a runtime
//! compatibility layer.
//!
//! `tokio-compat`'s `runtime` module contains modified versions of the `tokio`
//! 0.2 `Runtime` and `current_thread::Runtime` that are capable of providing
//! `tokio` 0.1 and `tokio` 0.2-compatible runtime services.
//!
//! Creating a [`Runtime`] does the following:
//!
//! * Start a [thread pool] for executing futures.
//! * Run [resource drivers] for the timer and I/O resources.
//! * Run a **single** `tokio` 0.1 [`Reactor`][reactor-01] and
//!   [`Timer`][timer-01] on a background thread, for legacy tasks.
//!
//! Legacy `futures` 0.1 tasks will be executed by the `tokio` 0.2 thread pool
//! workers, alongside `std::future` tasks. However, they will use the timer and
//! reactor provided by the compatibility background thread.
//!
//! ## Using the Compatibility Runtime
//!
//! ### Spawning
//!
//! In order to allow drop-in compatibility for legacy codebases using
//! `tokio` 0.1, the  [`run`], [`spawn`], and [`block_on`] methods provided by the
//! compatibility runtimes take `futures` 0.1 futures. This allows the
//! compatibility runtimes to replace the `tokio` 0.1 runtimes in those codebases
//! without requiring changes to unrelated code. The compatibility runtimes
//! _also_ provide `std::future`-compatible versions of these methods, named
//! [`run_std`], [`spawn_std`], and [`block_on_std`].
//!
//! ### Shutting Down
//!
//! Tokio 0.1 and Tokio 0.2 provide subtly different behavior around spawning
//! tasks and shutting down runtimes. In 0.1, the threadpool and current-thread
//! runtimes provide [`shutdown_on_idle`][on_idle_01] and [`run`][run_01] methods,
//! respectively, which will shut the runtime down when when there are no more
//! tasks currently executing. In Tokio 0.2, these methods no longer exist.
//! Instead, 0.2's runtime will shut down when the future passed to `block_on`
//! finishes, and  the `spawn` function now returns a [`JoinHandle`], which can
//! be used to await  the completion of the spawned future. By awaiting these
//! `JoinHandle`s, the  0.2 user now has much more fine-grained control over
//! which tasks keep the runtime active. Additionally, 0.2's runtimes can avoid
//! the overhead of tracking the global number of running tasks for determining
//! idleness.
//!
//! How, then, do we bridge the gaps between these two models? The
//! `tokio-compat` runtimes provide separate APIs for spawning both Tokio 0.1
//! and `Tokio` 0.2 tasks with with _and_ without `JoinHandle`s. For example,
//! the `Runtime` type has the following methods:
//!
//! * [`Runtime::spawn`] spawns a `futures` 0.1 future, and does not return a
//!   `JoinHandle` (like `tokio` 0.1).
//! * [`Runtime::spawn_std`] spawns a `std::future` future, and does not return
//!   a `JoinHandle`.
//! * [`Runtime::spawn_handle`] spawns a `futures` 0.1 future, and returns a
//!   `JoinHandle`.
//! * [`Runtime::spawn_handle_std`] spawns a `std::future` future, and returns a
//!   `JoinHandle` (like `tokio` 0.2).
//!
//! Other types which spawn futures, like [`TaskExecutor`], provide similar APIs.
//!
//! The `tokio-compat` current-thread and thread pool runtimes also provide
//! their own versions of `run` and `shutdown_on_idle`, respectively. However,
//! **only tasks spawned without `JoinHandle`s** "count" against idleness. This
//! means that if a task is spawned by a function returning a `JoinHandle`, it
//! will **not** keep the runtime active if no other tasks are running.
//!
//! Why is this the case? There are two primary reasons:
//!
//! 1. The `shutdown_on_idle` and `run` methods no longer exist in `tokio` 0.2.
//!    Therefore, codebases transitioning from 0.1 to 0.2 will no longer be able
//!    to use these APIs when they are fully transitioned to 0.2. Therefore,
//!    the compatibility runtime provides them to enable incremental transitions
//!    to the new APIs, but ideally, code using them should *eventually* be
//!    rewritten to use the new style.
//! 2. In order to implement idleness tracking on the compatibility runtimes, it
//!    is necessary to override the behavior of `tokio::spawn`. This is possible
//!    in Tokio 0.1, using [`executor::with_default`], so we are able to track
//!    idleness of futures spawned via 0.1's `tokio::spawn`. However, Tokio 0.2
//!    does not allow overriding spawn's behavior, so we are **not** able to
//!    track the idleness of futures spawned by 0.2's `tokio::spawn`. Therefore,
//!    in order to provide a consistent API, `tokio-compat` makes a distinction
//!    between tasks spawned with and without `JoinHandle`s.
//!
//! For reference, spawning tasks via the following functions **will** count to
//! keep the runtime active in `run` or `shutdown_on_idle`:
//!
//! * [`Runtime::spawn`] and [`current_thread::Runtime::spawn`]
//! * [`Runtime::spawn_std`] and [`current_thread::Runtime::spawn_std`]
//! * [`TaskExecutor::spawn`] and [`current_thread::TaskExecutor::spawn_local`]
//! * [`TaskExecutor::spawn_std`] and [`current_thread::TaskExecutor::spawn_local_std`]
//! * [`current_thread::Handle::spawn`] and [`current_thread::Handle::spawn_std`]
//! * All implementations of `tokio` 0.1's [`Executor`] and [`TypedExecutor`]
//!   traits for types defined in `tokio-compat`.
//! * [`tokio::spawn`][spawn_01] **from the 0.1 version of `tokio`**
//! * `tokio` 0.1's [`DefaultExecutor`]
//!
//! Meanwhile, the tasks spawned by the these functions will **not** count to
//! keep the runtime active, and return `JoinHandle`s that must be `await`ed
//! instead:
//!
//! * [`Runtime::spawn_handle`] and [`current_thread::Runtime::spawn_handle`]
//! * [`Runtime::spawn_handle_std`] and [`current_thread::Runtime::spawn_handle_std`]
//! * [`TaskExecutor::spawn_handle`] and [`current_thread::TaskExecutor::spawn_handle`]
//! * [`TaskExecutor::spawn_handle`] and [`current_thread::TaskExecutor::spawn_handle_std`]
//! * [`current_thread::Handle::spawn_handle`] and [`current_thread::Handle::spawn_handle_std`]
//! * [`tokio::spawn`][spawn_02], [`tokio::task::spawn_local`], and
//!   [`tokio::task::spawn_blocking`] **from the 0.2 version of `tokio`**
//!
//! [`JoinHandle`]: https://docs.rs/tokio/0.2.4/tokio/task/struct.JoinHandle.html
//! [`Runtime::spawn`]: struct.Runtime.html#method.spawn
//! [`Runtime::spawn_std`]: struct.Runtime.html#method.spawn_std
//! [`Runtime::spawn_handle`]: struct.Runtime.html#method.spawn_handle
//! [`Runtime::spawn_handle_std`]: struct.Runtime.html#method.spawn_handle_std
//! [`TaskExecutor`]: struct.TaskExecutor.html
//! [`TaskExecutor::spawn`]: struct.TaskExecutor.html#method.spawn
//! [`TaskExecutor::spawn_std`]: struct.TaskExecutor.html#method.spawn_std
//! [`TaskExecutor::spawn_handle`]: struct.TaskExecutor.html#method.spawn_handle
//! [`current_thread::Runtime::spawn`]: current_thread/struct.Runtime.html#method.spawn
//! [`current_thread::Runtime::spawn_std`]: current_thread/struct.Runtime.html#method.spawn_std
//! [`current_thread::Runtime::spawn_handle`]: current_thread/struct.Runtime.html#method.spawn_handle
//! [`current_thread::Runtime::spawn_handle_std`]: current_thread/struct.Runtime.html#method.spawn_handle_std
//! [`current_thread::TaskExecutor::spawn_local`]: current_thread/struct.TaskExecutor.html#method.spawn_local_std
//! [`current_thread::TaskExecutor::spawn_local_std`]: current_thread/struct.TaskExecutor.html#method.spawn_local_std
//! [`current_thread::TaskExecutor::spawn_handle`]: current_thread/struct.TaskExecutor.html#method.spawn_handle
//! [`current_thread::TaskExecutor::spawn_handle_std`]: current_thread/struct.TaskExecutor.html#method.spawn_handle_std
//! [`current_thread::Handle::spawn`]: current_thread/struct.Handle.html#method.spawn
//! [`current_thread::Handle::spawn_std`]: current_thread/struct.Handle.html#method.spawn_std
//! [`current_thread::Handle::spawn_handle`]: current_thread/struct.Handle.html#method.spawn_handle
//! [`current_thread::Handle::spawn_handle_std`]: current_thread/struct.Handle.html#method.spawn_handle_std
//! [`Executor`]: https://docs.rs/tokio/0.1.22/tokio/executor/trait.Executor.html
//! [`TypedExecutor`]: https://docs.rs/tokio/0.1.22/tokio/executor/trait.TypedExecutor.html
//! [`DefaultExecutor`]:  https://docs.rs/tokio/0.1.22/tokio/executor/struct.DefaultExecutor.html
//! [spawn_01]: https://docs.rs/tokio/0.1.22/tokio/executor/fn.spawn.html
//! [spawn_02]: https://docs.rs/tokio/0.2.4/tokio/fn.spawn.html
//! [run_01]: https://docs.rs/tokio/0.1.22/tokio/runtime/current_thread/struct.Runtime.html#method.run.html
//! [on_idle_01]: https://docs.rs/tokio/0.1.22/tokio/runtime/struct.Runtime.html#method.shutdown_on_idle.html
//! [`tokio::task::spawn_local`]: https://docs.rs/tokio/0.2.4/tokio/task/fn.spawn_local.html
//! [`tokio::task::spawn_blocking`]: https://docs.rs/tokio/0.2.4/tokio/task/fn.spawn_blocking.html
//! [`executor::with_default`]: https://docs.rs/tokio-executor/0.1.9/tokio_executor/fn.with_default.html
//!
//! ### Blocking
//!
//! The compatibility thread pool runtime does **not** currently support the
//! `tokio` 0.1 [`tokio_threadpool::blocking`][blocking] API. Calls to the
//! legacy version of `blocking` made on the compatibilityruntime will currently
//! fail. In the future, `tokio-compat` will allow transparently replacing
//! legacy `blocking` with the `tokio` 0.2 blocking APIs, but in the meantime,
//! it will be necessary to convert this code to call into the `tokio` 0.2
//! [`task::block_in_place`] and [`task::spawn_blocking`] APIs instead.
//!
//! [blocking]: https://docs.rs/tokio-threadpool/0.1.16/tokio_threadpool/fn.blocking.html
//! [`task::block_in_place`]: https://docs.rs/tokio/0.2.4/tokio/task/fn.block_in_place.html
//! [`task::spawn_blocking`]: https://docs.rs/tokio/0.2.4/tokio/task/fn.spawn_blocking.html
//!
//! ## Examples
//!
//! Spawning both `tokio` 0.1 and `tokio` 0.2 futures:
//!
//! ```rust
//! use futures_01::future::lazy;
//!
//! tokio_compat::run(lazy(|| {
//!     // spawn a `futures` 0.1 future using the `spawn` function from the
//!     // `tokio` 0.1 crate:
//!     tokio_01::spawn(lazy(|| {
//!         println!("hello from tokio 0.1!");
//!         Ok(())
//!     }));
//!
//!     // spawn an `async` block future on the same runtime using `tokio`
//!     // 0.2's `spawn`:
//!     tokio_02::spawn(async {
//!         println!("hello from tokio 0.2!");
//!     });
//!
//!     Ok(())
//! }))
//! ```
//!
//! Futures on the compat runtime can use `timer` APIs from both 0.1 and 0.2
//! versions of `tokio`:
//!
//! ```rust
//! # use std::time::{Duration, Instant};
//! use tokio_compat::prelude::*;
//!
//! tokio_compat::run_std(async {
//!     // Wait for a `tokio` 0.1 `Delay`...
//!     let when = Instant::now() + Duration::from_millis(10);
//!     tokio_01::timer::Delay::new(when)
//!         // convert the delay future into a `std::future` that we can `await`.
//!         .compat()
//!         .await
//!         .expect("tokio 0.1 timer should work!");
//!     println!("10 ms have elapsed");
//!
//!     // Wait for a `tokio` 0.2 `Delay`...
//!     tokio_02::time::delay_for(Duration::from_millis(20)).await;
//!     println!("20 ms have elapsed");
//! });
//! ```
//!
//! [`tokio::runtime`]: https://docs.rs/tokio/0.2.4/tokio/runtime/index.html
//! [futures-compat]: https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.19/futures/compat/index.html
//! [`Runtime`]: struct.Runtime.html
//! [resource drivers]: https://docs.rs/tokio/0.2.4/tokio/runtime/index.html#resource-drivers
//! [thread pool]: https://docs.rs/tokio/0.2.4/tokio/runtime/index.html#threaded-scheduler
//! [timer-01]: https://docs.rs/tokio/0.1.22/tokio/timer/index.html
//! [reactor-01]: https://docs.rs/tokio/0.1.22/tokio/reactor/struct.Reactor.html
//! [`run`]: struct.Runtime.html#method.run
//! [`spawn`]: struct.Runtime.html#method.spawn
//! [`block_on`]: struct.Runtime.html#method.block_on
//! [`run_std`]: struct.Runtime.html#method.run_std
//! [`spawn_std`]: struct.Runtime.html#method.spawn_std
//! [`block_on_std`]: struct.Runtime.html#method.spawn_std
mod compat;
pub mod current_thread;
mod idle;
#[cfg(feature = "rt-full")]
mod threadpool;

#[cfg(feature = "rt-full")]
pub use threadpool::{run, run_std, Builder, Runtime, TaskExecutor};
