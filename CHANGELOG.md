# 0.1.1 (December 18, 2019)

### Fixed

- Hang when calling `block_on` twice on the same `Runtime` (#11)
- `Runtime::shutdown_on_idle` and `current_thread::Runtime::run` completing
  early when a future has completed previously on that runtime (#12)

# 0.1.0 (December 17, 2019)

- Initial release
