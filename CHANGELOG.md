# 0.1.4 (January 8, 2019)

### Fixed

- Compiler warnings in the threadpool runtime (#21)

# 0.1.3 (January 7, 2019)

### Fixed

- Removed unnecessary `'static` bounds on the `current_thread` runtime's
  `block_on`/`block_on_std` functions that broke API compatibility with Tokio
  0.1 (#17)

### Added

- Implementation of `futures` 0.1 `Executor` trait for `TaskExecutor`s (#18)

# 0.1.2 (December 18, 2019)

### Fixed

- Dependencies on `futures-core-preview` and `futures-util-preview` updated to
  release `futures` 0.3 versions (#14)

# 0.1.1 (December 18, 2019)

### Fixed

- Hang when calling `block_on` twice on the same `Runtime` (#11)
- `Runtime::shutdown_on_idle` and `current_thread::Runtime::run` completing
  early when a future has completed previously on that runtime (#12)

# 0.1.0 (December 17, 2019)

- Initial release
