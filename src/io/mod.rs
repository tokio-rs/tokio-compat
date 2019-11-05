//! Compatibility between the `tokio::io` `AsyncRead` and `AsyncWrite` traits,
//! and other versions of those traits.
#[cfg(feature = "futures-io")]
pub mod futures_io;
