#![doc = include_str!("../README.md")]
#![deny(clippy::all, clippy::pedantic)]

use opentelemetry::{Key, KeyValue, Value};

mod runtime;

/// Configuration for Tokio runtime instrumentation.
///
/// ## Multiple Runtimes with Custom Labels
///
/// ```no_run
/// use opentelemetry::KeyValue;
/// use opentelemetry_instrumentation_tokio::Config;
///
/// let rt1 = Runtime::new().unwrap();
/// let rt2 = Runtime::new().unwrap();
///
/// // Add custom labels to distinguish runtimes
/// Config::new()
///     .with_label("runtime.name", "api-server")
///     .observe_runtime(rt1.handle());
/// Config::new()
///     .with_label("runtime.name", "worker")
///     .observe_runtime(rt2.handle());
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    labels: Vec<KeyValue>,
}

impl Config {
    /// Create a new configuration with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { labels: Vec::new() }
    }

    /// Add custom labels to this runtime's metrics.
    ///
    /// Labels help distinguish metrics from different runtimes when observing
    /// multiple runtimes in the same process.
    ///
    /// When `tokio_unstable` is enabled, a `tokio.runtime.id` label is
    /// automatically added in addition to any custom labels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use opentelemetry::KeyValue;
    /// use opentelemetry_instrumentation_tokio::Config;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// Config::new()
    ///     .with_labels([
    ///         KeyValue::new("runtime.name", "worker-pool"),
    ///         KeyValue::new("env", "production"),
    ///     ])
    ///     .observe_current_runtime();
    /// # }
    /// ```
    #[must_use]
    pub fn with_labels(mut self, labels: impl IntoIterator<Item = KeyValue>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Add a single custom label to this runtime's metrics.
    ///
    /// This method can be chained to add multiple labels.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use opentelemetry_instrumentation_tokio::Config;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// Config::new()
    ///     .with_label("runtime.name", "api-server")
    ///     .with_label("runtime.purpose", "http-requests")
    ///     .observe_current_runtime();
    /// # }
    /// ```
    #[must_use]
    pub fn with_label(mut self, key: impl Into<Key>, value: impl Into<Value>) -> Self {
        self.labels.push(KeyValue::new(key, value));
        self
    }

    /// Observe metrics for the current Tokio runtime.
    ///
    /// This is a convenience method that calls [`Self::observe_runtime`] with
    /// the current runtime handle.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a Tokio runtime context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use opentelemetry_instrumentation_tokio::Config;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// Config::new().observe_current_runtime();
    /// # }
    /// ```
    pub fn observe_current_runtime(self) {
        let handle = tokio::runtime::Handle::current();
        self.observe_runtime(&handle);
    }

    /// Observe metrics for a specific Tokio runtime.
    ///
    /// Registers OpenTelemetry observable instruments that expose Tokio runtime
    /// metrics. The metrics are collected on-demand by the configured meter
    /// provider.
    ///
    /// This function can be called multiple times to observe multiple runtimes.
    /// Each runtime's metrics will be distinguished by the labels configured
    /// via [`Self::with_labels`] or [`Self::with_label`].
    ///
    /// When `tokio_unstable` is enabled, a `tokio.runtime.id` label is
    /// automatically added.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use opentelemetry_instrumentation_tokio::Config;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let handle = tokio::runtime::Handle::current();
    /// Config::new().observe_runtime(&handle);
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the global runtime registry is poisoned.
    pub fn observe_runtime(self, handle: &tokio::runtime::Handle) {
        self::runtime::track_runtime(handle, &self.labels);
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Observe metrics for the current Tokio runtime.
///
/// This is a convenience function that uses default configuration.
/// For more control, use [`Config`].
///
/// # Panics
///
/// Panics if called outside of a Tokio runtime context.
///
/// # Examples
///
/// ```no_run
/// use opentelemetry_instrumentation_tokio;
///
/// # #[tokio::main]
/// # async fn main() {
/// opentelemetry_instrumentation_tokio::observe_current_runtime();
/// # }
/// ```
pub fn observe_current_runtime() {
    Config::default().observe_current_runtime();
}

/// Observe metrics for a specific Tokio runtime.
///
/// This is a convenience function that uses default configuration.
/// For more control, use [`Config`].
///
/// # Examples
///
/// ```no_run
/// use opentelemetry_instrumentation_tokio;
///
/// # #[tokio::main]
/// # async fn main() {
/// let handle = tokio::runtime::Handle::current();
/// opentelemetry_instrumentation_tokio::observe_runtime(&handle);
/// # }
/// ```
pub fn observe_runtime(handle: &tokio::runtime::Handle) {
    Config::default().observe_runtime(handle);
}
