# OpenTelemetry instrumentation for Tokio runtime metrics.

This crate provides observability for Tokio runtimes by exposing runtime metrics through OpenTelemetry.

[![Crates.io](https://img.shields.io/crates/v/opentelemetry-instrumentation-tokio)](https://crates.io/crates/opentelemetry-instrumentation-tokio)
[![Documentation](https://docs.rs/opentelemetry-instrumentation-tokio/badge.svg)](https://docs.rs/opentelemetry-instrumentation-tokio)
![License](https://img.shields.io/crates/l/opentelemetry-instrumentation-tokio)

## Installation

```toml
[dependencies]
opentelemetry-instrumentation-tokio = "0.1"
opentelemetry = "0.31"
opentelemetry_sdk = "0.31"
tokio = { version = "1", features = ["rt-multi-thread"] }
```

## Quick Start

```rust
use opentelemetry_sdk::metrics::SdkMeterProvider;

#[tokio::main]
async fn main() {
    // Setup your meter provider
    let provider = SdkMeterProvider::builder().build();
    opentelemetry::global::set_meter_provider(provider);
    
    // Instrument the current runtime
    opentelemetry_instrumentation_tokio::observe_current_runtime();
    
    // Your application code
}
```

## Configuration

### Explicit Runtime Handle

```rust
let handle = tokio::runtime::Handle::current();
opentelemetry_instrumentation_tokio::observe_runtime(&handle);
```

### Multiple Runtimes

Use custom labels to distinguish metrics from different runtimes. Labels are merged with the automatically added `tokio.runtime.id` (when available) so you can disambiguate runtimes without manual guards or deduplication.

```rust
use opentelemetry_instrumentation_tokio::Config;

Config::new()
    .with_label("runtime.name", "api")
    .observe_current_runtime();

let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

runtime.block_on(async {
    Config::new()
        .with_label("runtime.name", "worker")
        .observe_current_runtime();
});
```

```rust
use opentelemetry::KeyValue;

Config::new()
    .with_labels([
        KeyValue::new("service", "api"),
        KeyValue::new("env", "production"),
    ])
    .observe_current_runtime();
```

## Metrics

### Always Available (4 metrics)

These metrics work without any special configuration:

- `tokio.workers` - Number of worker threads
- `tokio.global_queue_depth` - Tasks in global queue
- `tokio.worker.park_count` - Worker park operations (per-worker)
- `tokio.worker.busy_duration` - Worker busy time in ms (per-worker)

### Requires `tokio_unstable` (19 additional metrics)

Most metrics require building with the `tokio_unstable` cfg flag:

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo build
```

See the [Tokio documentation](https://docs.rs/tokio/latest/tokio/#unstable-features) for more details.

Available metrics with `tokio_unstable`:

**Runtime-level metrics:**
- `tokio.blocking_threads` - Blocking thread pool size
- `tokio.active_tasks` - Number of alive tasks
- `tokio.idle_blocking_threads` - Idle blocking threads
- `tokio.remote_schedules` - Remote task schedules
- `tokio.budget_forced_yields` - Budget-forced yields
- `tokio.spawned_tasks_count` - Total spawned tasks
- `tokio.blocking_queue_depth` - Blocking queue depth

**I/O driver metrics:**
- `tokio.io_driver.fd_registrations` - FD registrations
- `tokio.io_driver.fd_deregistrations` - FD deregistrations
- `tokio.io_driver.fd_readies` - Ready events processed

**Per-worker metrics** (all with `tokio.worker.index` attribute):
- `tokio.worker.noops` - No-op wake-ups
- `tokio.worker.task_steals` - Tasks stolen
- `tokio.worker.steal_operations` - Steal operations
- `tokio.worker.polls` - Task polls
- `tokio.worker.local_schedules` - Local task schedules
- `tokio.worker.overflows` - Local queue overflows
- `tokio.worker.local_queue_depth` - Local queue depth
- `tokio.worker.mean_poll_time` - Mean poll duration (ns)
- `tokio.worker.poll_time_bucket` - Poll time histogram (requires config + runtime support)

## License

Licensed under the Apache License, Version 2.0.
