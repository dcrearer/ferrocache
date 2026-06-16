//! Telemetry initialization for FerroCache — all three observability pillars.
//!
//! When an OTLP endpoint is configured, [`init`] wires up:
//! 1. **Logs** — a `tracing` fmt layer to stdout (pretty or JSON), filtered by
//!    the configured directives.
//! 2. **Traces** — an OpenTelemetry span pipeline exported over OTLP/gRPC, with
//!    head sampling controlled by `trace_sample_ratio`.
//! 3. **Metrics** — an OpenTelemetry meter with a periodic OTLP exporter (see
//!    [`register_cache_metrics`] for the instruments).
//!
//! When no endpoint is configured, only the stdout log layer is installed and
//! there is zero traces/metrics overhead.
//!
//! All settings come from [`ObservabilitySettings`] (config file + env
//! overrides) — nothing here reads the environment directly.
//!
//! ## Why this shape
//! Logs and traces share the `tracing` pipeline; metrics use the OTel metrics
//! SDK. All export over OTLP to a single Collector, which routes to the backend
//! (Elasticsearch via APM Server) — keeping the app decoupled from any vendor.
//!
//! ## Collector availability is non-fatal (best-effort export)
//! Building the OTLP exporters does NOT open a connection, so a collector that
//! is down at startup does not fail `init` or block the server. Export happens
//! in the background batch/periodic processors; if the collector is
//! unreachable, telemetry is retried and ultimately dropped while the cache
//! keeps serving. This is intentional: **observability must never take down the
//! data plane.**
//!
//! ## Lifecycle
//! [`init`] returns an [`OtelGuard`] that MUST be kept alive for the program's
//! lifetime. Dropping it flushes and shuts down both pipelines so buffered
//! spans/metrics are not lost on exit.

use crate::cache::storage::CacheStorage;
use crate::config::{LogFormat, ObservabilitySettings};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Held by `main` for the process lifetime. On drop, flushes and shuts down the
/// OTLP exporters so no spans/metrics are lost. Providers are `None` when OTLP
/// export is disabled (no endpoint configured).
pub struct OtelGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        // Flush both pipelines before exit. Errors are best-effort — we're
        // shutting down anyway, so log and move on.
        //
        // We use `eprintln!` rather than `tracing` deliberately: this runs at
        // process teardown, where the tracing subscriber / OTel layer may
        // already be shutting down, so a `tracing` call here could be dropped.
        // stderr is always available.
        if let Some(provider) = self.tracer_provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("error shutting down tracer provider: {e:?}");
            }
        }
        if let Some(provider) = self.meter_provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("error shutting down meter provider: {e:?}");
            }
        }
    }
}

/// Initialize logging and (optionally) OTLP trace + metric export from config.
///
/// - `settings.log_filter` sets the log/span filter (same syntax as `RUST_LOG`).
/// - `settings.log_format` chooses pretty vs JSON stdout output.
/// - `settings.otlp_endpoint`, when present, enables OTLP/gRPC export of both
///   traces and metrics to that collector. When `None`/empty, only stdout
///   logging is active (no traces/metrics overhead).
/// - `settings.trace_sample_ratio` sets head sampling (0.0–1.0).
/// - `settings.metric_export_interval_secs` sets the metric push interval.
///
/// A collector that is unreachable at startup is non-fatal — see the module
/// docs. Returns `Err` only if an exporter cannot be *constructed* (e.g. an
/// invalid endpoint string), not if the collector is merely down.
pub fn init(settings: &ObservabilitySettings) -> anyhow::Result<OtelGuard> {
    // Build the filter from the configured directives. Fall back to "info" if
    // the directive string is somehow invalid rather than failing startup.
    let env_filter = EnvFilter::try_new(&settings.log_filter)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // The fmt layer: JSON for production/shipping, pretty for local dev.
    let fmt_layer = match settings.log_format {
        LogFormat::Json => tracing_subscriber::fmt::layer().json().boxed(),
        LogFormat::Pretty => tracing_subscriber::fmt::layer().boxed(),
    };

    if let Some(endpoint) = settings.otlp_endpoint.as_deref().filter(|s| !s.is_empty()) {
        // Shared resource (service.name/version) for both traces and metrics.
        let resource = Resource::builder()
            .with_service_name("ferrocache")
            .with_attributes([KeyValue::new("service.version", env!("CARGO_PKG_VERSION"))])
            .build();

        // --- Traces: OTLP/gRPC span exporter, batched ---
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        // Head sampling: keep `ratio` of traces. ParentBased honors an upstream
        // sampling decision when present (so a sampled trace stays whole across
        // services) and applies the ratio only at the root. This is the in-app
        // safety valve against span-queue overflow under load; keep-errors /
        // keep-slow policies belong in the collector as tail sampling.
        let ratio = settings.sample_ratio_clamped();
        let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)));

        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .with_sampler(sampler)
            .with_resource(resource.clone())
            .build();

        let tracer = tracer_provider.tracer("ferrocache");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        // --- Metrics: OTLP/gRPC metric exporter, periodic push ---
        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let reader = PeriodicReader::builder(metric_exporter)
            .with_interval(Duration::from_secs(settings.metric_export_interval_secs))
            .build();

        let meter_provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource)
            .build();

        // Make this the global meter provider so `opentelemetry::global::meter`
        // anywhere in the app records into this pipeline.
        opentelemetry::global::set_meter_provider(meter_provider.clone());

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();

        Ok(OtelGuard {
            tracer_provider: Some(tracer_provider),
            meter_provider: Some(meter_provider),
        })
    } else {
        // No collector configured: stdout logging only.
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(OtelGuard {
            tracer_provider: None,
            meter_provider: None,
        })
    }
}

/// Register observable (async) instruments that report cache metrics.
///
/// ## Why observable instruments
/// Cache `get`/`set` run on the hot path millions of times/sec. Calling the
/// metrics SDK there would add overhead and contention. Instead the hot path
/// only bumps lock-free atomics in [`CacheMetrics`]; these *observable*
/// instruments read those totals on the SDK's collection interval (off the hot
/// path), so metric export costs nothing per operation.
///
/// Call once, after [`init`], passing the live cache. The registered callbacks
/// hold an `Arc` to the cache for the program's lifetime.
pub fn register_cache_metrics(cache: &Arc<CacheStorage>) {
    let meter = opentelemetry::global::meter("ferrocache");

    // --- Monotonic counters (observable) ---
    let metrics = cache.metrics();
    meter
        .u64_observable_counter("ferrocache.cache.hits")
        .with_description("Total GET operations that found a live entry")
        .with_callback({
            let m = metrics.clone();
            move |o| o.observe(m.hit_count(), &[])
        })
        .build();

    meter
        .u64_observable_counter("ferrocache.cache.misses")
        .with_description("Total GET operations that found nothing or an expired entry")
        .with_callback({
            let m = metrics.clone();
            move |o| o.observe(m.miss_count(), &[])
        })
        .build();

    meter
        .u64_observable_counter("ferrocache.cache.evictions")
        .with_description("Total entries removed by LRU eviction under memory pressure")
        .with_callback({
            let m = metrics.clone();
            move |o| o.observe(m.eviction_count(), &[])
        })
        .build();

    // --- Gauges (observable) ---
    meter
        .u64_observable_gauge("ferrocache.cache.memory.used_bytes")
        .with_description("Current cache memory usage in bytes")
        .with_callback({
            let c = cache.clone();
            move |o| o.observe(c.memory_used() as u64, &[])
        })
        .build();

    meter
        .u64_observable_gauge("ferrocache.cache.keys")
        .with_description("Current number of keys in the cache")
        .with_callback({
            let c = cache.clone();
            move |o| o.observe(c.len() as u64, &[])
        })
        .build();
}
