//! A prelude for `tokio` 0.1/0.2 compatibility.
#[cfg(feature = "sink")]
#[doc(no_inline)]
pub use futures_util::compat::Sink01CompatExt as _;
#[doc(no_inline)]
pub use futures_util::compat::{Future01CompatExt as _, Stream01CompatExt as _};
