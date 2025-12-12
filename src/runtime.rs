//! Runtime metrics implementation.
//!
//! This module contains all the metric registration logic for Tokio runtime
//! metrics. Each metric is implemented as a separate function for clarity and
//! maintainability.

use std::sync::{Once, RwLock};

use opentelemetry::metrics::Meter;
use opentelemetry::{InstrumentationScope, Key, KeyValue};

/// One-time instrument initialization.
static INSTRUMENTS_INITIALIZED: Once = Once::new();

/// Registry of all observed runtimes.
static RUNTIMES: RwLock<Vec<TrackedRuntime>> = RwLock::new(Vec::new());

/// A tracked runtime with its metrics and labels.
struct TrackedRuntime {
    metrics: tokio::runtime::RuntimeMetrics,
    labels: Vec<KeyValue>,
}

/// Track a Tokio runtime for metrics collection.
///
/// This also initializes the instruments on the first call.
pub(crate) fn track_runtime(handle: &tokio::runtime::Handle, labels: &[KeyValue]) {
    // Ensure instruments are initialized (one-time, thread-safe).
    INSTRUMENTS_INITIALIZED.call_once(|| {
        register_all_instruments();
    });

    let tracked_runtime = TrackedRuntime {
        metrics: handle.metrics().clone(),
        labels: build_runtime_labels(handle, labels),
    };

    let mut runtimes = RUNTIMES.write().unwrap();
    runtimes.push(tracked_runtime);
}

/// Build labels for a runtime (user labels + tokio.runtime.id if available).
fn build_runtime_labels(handle: &tokio::runtime::Handle, labels: &[KeyValue]) -> Vec<KeyValue> {
    let mut labels = labels.to_vec();

    // Auto-add tokio.runtime.id when tokio_unstable is available
    #[cfg(tokio_unstable)]
    {
        labels.push(KeyValue::new(
            Key::from_static_str("tokio.runtime.id"),
            handle.id().to_string(),
        ));
    }

    // Silence unused parameter warning when tokio_unstable is not set
    #[cfg(not(tokio_unstable))]
    let _ = handle;

    labels
}

/// Helper to construct a [`KeyValue`] with the worker index.
fn worker_idx_attribute(i: usize) -> KeyValue {
    KeyValue::new(
        Key::from_static_str("tokio.worker.index"),
        i.try_into().unwrap_or(i64::MAX),
    )
}

/// Register all instruments (one-time, called via `Once`).
fn register_all_instruments() {
    let scope = InstrumentationScope::builder(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .build();

    let meter = opentelemetry::global::meter_with_scope(scope);

    // Always-available metrics
    register_workers_gauge(&meter);
    register_global_queue_depth_gauge(&meter);
    register_alive_tasks_gauge(&meter);

    // Metrics requiring 64-bit atomics
    #[cfg(target_has_atomic = "64")]
    {
        register_worker_park_count_counter(&meter);
        register_worker_busy_duration_counter(&meter);
    }

    // Metrics requiring `--cfg tokio_unstable`
    #[cfg(tokio_unstable)]
    {
        register_blocking_threads_gauge(&meter);
        register_idle_blocking_threads_gauge(&meter);
        register_remote_schedules_counter(&meter);
        register_budget_forced_yields_counter(&meter);

        // I/O driver metrics require net feature
        #[cfg(all(not(target_family = "wasm"), target_has_atomic = "64", feature = "net"))]
        {
            register_io_driver_fd_registrations_counter(&meter);
            register_io_driver_fd_deregistrations_counter(&meter);
            register_io_driver_fd_readies_counter(&meter);
        }

        register_spawned_tasks_count_counter(&meter);
        register_blocking_queue_depth_gauge(&meter);
        register_worker_noops_counter(&meter);
        register_worker_task_steals_counter(&meter);
        register_worker_steal_operations_counter(&meter);
        register_worker_polls_counter(&meter);
        register_worker_local_schedules_counter(&meter);
        register_worker_overflows_counter(&meter);
        register_worker_local_queue_depth_gauge(&meter);
        register_worker_mean_poll_time_gauge(&meter);
        register_poll_time_histogram(&meter);
    }
}

// ============================================================================
// Always-available metrics
// ============================================================================

fn register_workers_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.workers")
        .with_description("The number of worker threads used by the runtime")
        .with_unit("{worker}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime.metrics.num_workers().try_into().unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

fn register_global_queue_depth_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.global_queue_depth")
        .with_description("The number of tasks currently scheduled in the runtime's global queue")
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime
                        .metrics
                        .global_queue_depth()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

#[cfg(target_has_atomic = "64")]
fn register_worker_park_count_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.park_count")
        .with_description("The total number of times the given worker thread has parked")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(runtime.metrics.worker_park_count(worker_idx), &attributes);
                }
            }
        })
        .build();
}

#[cfg(target_has_atomic = "64")]
fn register_worker_busy_duration_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.busy_duration")
        .with_description("The amount of time the given worker thread has been busy")
        .with_unit("ms")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(
                        runtime
                            .metrics
                            .worker_total_busy_duration(worker_idx)
                            .as_millis()
                            .try_into()
                            .unwrap_or(u64::MAX),
                        &attributes,
                    );
                }
            }
        })
        .build();
}

fn register_alive_tasks_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.alive_tasks")
        .with_description("The number of active tasks in the runtime")
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime
                        .metrics
                        .num_alive_tasks()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

// ============================================================================
// Metrics requiring tokio_unstable
// ============================================================================

#[cfg(tokio_unstable)]
fn register_blocking_threads_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.blocking_threads")
        .with_description("The number of additional threads spawned by the runtime")
        .with_unit("{thread}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime
                        .metrics
                        .num_blocking_threads()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_idle_blocking_threads_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.idle_blocking_threads")
        .with_description(
            "The number of idle threads, which have spawned by the runtime for `spawn_blocking` calls",
        )
        .with_unit("{thread}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime.metrics
                        .num_idle_blocking_threads()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_remote_schedules_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.remote_schedules")
        .with_description("The number of tasks scheduled from outside the runtime")
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.remote_schedule_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_budget_forced_yields_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.budget_forced_yields")
        .with_description(
            "The number of times that tasks have been forced to yield back to the scheduler after exhausting their task budgets",
        )
        .with_unit("{yield}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.budget_forced_yield_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(all(
    tokio_unstable,
    not(target_family = "wasm"),
    target_has_atomic = "64",
    feature = "net"
))]
fn register_io_driver_fd_registrations_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.io_driver.fd_registrations")
        .with_description(
            "The number of file descriptors that have been registered with the runtime's I/O driver",
        )
        .with_unit("{fd}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.io_driver_fd_registered_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(all(
    tokio_unstable,
    not(target_family = "wasm"),
    target_has_atomic = "64",
    feature = "net"
))]
fn register_io_driver_fd_deregistrations_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.io_driver.fd_deregistrations")
        .with_description(
            "The number of file descriptors that have been deregistered by the runtime's I/O driver",
        )
        .with_unit("{fd}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.io_driver_fd_deregistered_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(all(
    tokio_unstable,
    not(target_family = "wasm"),
    target_has_atomic = "64",
    feature = "net"
))]
fn register_io_driver_fd_readies_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.io_driver.fd_readies")
        .with_description("The number of ready events processed by the runtime's I/O driver")
        .with_unit("{event}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.io_driver_ready_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_spawned_tasks_count_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.spawned_tasks_count")
        .with_description("The number of tasks spawned in this runtime since it was created")
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(runtime.metrics.spawned_tasks_count(), &runtime.labels);
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_blocking_queue_depth_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.blocking_queue_depth")
        .with_description(
            "The number of tasks currently scheduled in the blocking thread pool, spawned using `spawn_blocking`",
        )
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                instrument.observe(
                    runtime.metrics
                        .blocking_queue_depth()
                        .try_into()
                        .unwrap_or(u64::MAX),
                    &runtime.labels,
                );
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_noops_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.noops")
        .with_description(
            "The number of times the given worker thread unparked but performed no work before parking again",
        )
        .with_unit("{operation}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(runtime.metrics.worker_noop_count(worker_idx), &attributes);
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_task_steals_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.task_steals")
        .with_description(
            "The number of tasks the given worker thread stole from another worker thread",
        )
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(runtime.metrics.worker_steal_count(worker_idx), &attributes);
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_steal_operations_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.steal_operations")
        .with_description(
            "The number of times the given worker thread stole tasks from another worker thread",
        )
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(
                        runtime.metrics.worker_steal_operations(worker_idx),
                        &attributes,
                    );
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_polls_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.polls")
        .with_description("The number of tasks the given worker thread has polled")
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(runtime.metrics.worker_poll_count(worker_idx), &attributes);
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_local_schedules_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.local_schedules")
        .with_description(
            "The number of tasks scheduled from **within** the runtime on the given worker's local queue",
        )
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(runtime.metrics.worker_local_schedule_count(worker_idx), &attributes);
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_overflows_counter(meter: &Meter) {
    meter
        .u64_observable_counter("tokio.worker.overflows")
        .with_description("The number of times the given worker thread saturated its local queue")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(
                        runtime.metrics.worker_overflow_count(worker_idx),
                        &attributes,
                    );
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_local_queue_depth_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.worker.local_queue_depth")
        .with_description(
            "The number of tasks currently scheduled in the given worker's local queue",
        )
        .with_unit("{task}")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(
                        runtime
                            .metrics
                            .worker_local_queue_depth(worker_idx)
                            .try_into()
                            .unwrap_or(u64::MAX),
                        &attributes,
                    );
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_worker_mean_poll_time_gauge(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.worker.mean_poll_time")
        .with_description("The mean duration of task polls, in nanoseconds")
        .with_unit("ns")
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut attributes = runtime.labels.clone();
                    attributes.push(worker_idx_attribute(worker_idx));
                    instrument.observe(
                        runtime
                            .metrics
                            .worker_mean_poll_time(worker_idx)
                            .as_nanos()
                            .try_into()
                            .unwrap_or(u64::MAX),
                        &attributes,
                    );
                }
            }
        })
        .build();
}

#[cfg(tokio_unstable)]
fn register_poll_time_histogram(meter: &Meter) {
    meter
        .u64_observable_gauge("tokio.worker.poll_time_bucket")
        .with_description("An histogram of the poll time of tasks, in nanoseconds")
        // We don't set a unit here, as it would add it as a suffix to the metric name
        .with_callback(|instrument| {
            let runtimes = RUNTIMES.read().unwrap();
            for runtime in runtimes.iter() {
                // Skip if Tokio runtime doesn't have histogram collection enabled
                if !runtime.metrics.poll_time_histogram_enabled() {
                    continue;
                }

                // Prepare the key-value pairs for the histogram buckets
                let mut buckets: Box<[_]> = (0..runtime.metrics.poll_time_histogram_num_buckets())
                    .map(|i| {
                        let range = runtime.metrics.poll_time_histogram_bucket_range(i);
                        let value = range.end.as_nanos().try_into().unwrap_or(i64::MAX);
                        let kv = KeyValue::new("le", value);
                        (i, kv)
                    })
                    .collect();

                // Change the last bucket to +Inf
                if let Some(last) = buckets.last_mut() {
                    last.1 = KeyValue::new("le", "+Inf");
                }

                // Emit histogram for each worker
                let num_workers = runtime.metrics.num_workers();
                for worker_idx in 0..num_workers {
                    let mut sum = 0u64;
                    for (bucket_idx, le) in &buckets {
                        let count = runtime
                            .metrics
                            .poll_time_histogram_bucket_count(worker_idx, *bucket_idx);
                        sum += count;

                        // Combine: runtime labels + worker_idx + le
                        let mut attributes = runtime.labels.clone();
                        attributes.push(worker_idx_attribute(worker_idx));
                        attributes.push(le.clone());

                        instrument.observe(sum, &attributes);
                    }
                }
            }
        })
        .build();
}
