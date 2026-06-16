//! Runtime configuration for FerroCache.
//!
//! Configuration is layered with the following precedence (lowest to highest):
//! 1. Built-in defaults (see the `Default` impls).
//! 2. A TOML config file, if present.
//! 3. Environment variables (overrides), for container/k8s deployments.
//!
//! ## Config file location
//! The file path is taken from `FERROCACHE_CONFIG` if set, otherwise
//! `./ferrocache.toml`. A missing file is NOT an error — defaults apply.
//!
//! ## Why layered
//! The file is the primary, human-edited source of truth (so changing the OTLP
//! endpoint never requires a recompile). Env vars sit on top so the same image
//! can be retargeted per-environment without editing the baked-in file, and so
//! the standard `OTEL_EXPORTER_OTLP_ENDPOINT` variable is honored.

use serde::Deserialize;
use std::path::Path;

/// Top-level configuration, mirroring the sections of the TOML file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerSettings,
    pub observability: ObservabilitySettings,
}

/// `[server]` section — network and cache-engine settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSettings {
    /// Address the RESP server binds to.
    pub bind_addr: String,
    /// Cache memory budget in megabytes.
    pub memory_limit_mb: usize,
    /// How often the background expiration reaper scans, in seconds.
    pub reaper_interval_secs: u64,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:6379".to_string(),
            memory_limit_mb: 100,
            reaper_interval_secs: 60,
        }
    }
}

impl ServerSettings {
    /// Memory limit expressed in bytes (what `CacheStorage` expects).
    pub fn memory_limit_bytes(&self) -> usize {
        self.memory_limit_mb * 1024 * 1024
    }
}

/// `[observability]` section — logging and tracing export settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObservabilitySettings {
    /// Log/span filter directives (same syntax as `RUST_LOG`), e.g. "info" or
    /// "ferrocache=debug,tower=warn".
    pub log_filter: String,
    /// Output format for stdout logs: `"pretty"` (human) or `"json"` (shipping).
    pub log_format: LogFormat,
    /// OTLP/gRPC collector endpoint for trace export, e.g.
    /// "http://localhost:4317". `None`/empty disables OTLP export entirely.
    pub otlp_endpoint: Option<String>,
    /// Fraction of traces to sample, 0.0–1.0 (applied via a parent-based
    /// `TraceIdRatioBased` sampler). 1.0 = sample everything (default, fine for
    /// dev/low volume); lower it under high throughput so the span queue isn't
    /// overwhelmed. This is the in-app *head* sampling safety valve; for
    /// keep-errors/keep-slow policies, do *tail* sampling in the collector.
    pub trace_sample_ratio: f64,
    /// How often (seconds) metrics are exported to the collector.
    pub metric_export_interval_secs: u64,
}

impl Default for ObservabilitySettings {
    fn default() -> Self {
        Self {
            log_filter: "info".to_string(),
            log_format: LogFormat::Pretty,
            otlp_endpoint: None,
            trace_sample_ratio: 1.0,
            metric_export_interval_secs: 60,
        }
    }
}

impl ObservabilitySettings {
    /// Clamp the configured sample ratio into the valid 0.0–1.0 range.
    /// Out-of-range values in the config are coerced rather than rejected so a
    /// typo can't take the service down.
    pub fn sample_ratio_clamped(&self) -> f64 {
        self.trace_sample_ratio.clamp(0.0, 1.0)
    }
}

/// Stdout log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

impl Config {
    /// Load configuration: defaults, overlaid by a TOML file (if found),
    /// overlaid by environment variables.
    ///
    /// Returns an error only when a config file exists but cannot be read or
    /// parsed — a *missing* file is fine and yields defaults (+ env overrides).
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("FERROCACHE_CONFIG")
            .unwrap_or_else(|_| "ferrocache.toml".to_string());

        let mut config = Self::from_file(&path)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Read and parse the TOML file at `path`. A missing file yields defaults.
    fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let config: Config = toml::from_str(&contents).map_err(|e| {
                    anyhow::anyhow!("failed to parse config file {}: {e}", path.display())
                })?;
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(anyhow::anyhow!(
                "failed to read config file {}: {e}",
                path.display()
            )),
        }
    }

    /// Apply environment-variable overrides on top of file/default values.
    ///
    /// Recognized variables:
    /// - `RUST_LOG` → `observability.log_filter`
    /// - `LOG_FORMAT` (`json`/`pretty`) → `observability.log_format`
    /// - `OTEL_EXPORTER_OTLP_ENDPOINT` → `observability.otlp_endpoint`
    /// - `FERROCACHE_BIND_ADDR` → `server.bind_addr`
    fn apply_env_overrides(&mut self) {
        if let Ok(filter) = std::env::var("RUST_LOG") {
            self.observability.log_filter = filter;
        }
        if let Ok(format) = std::env::var("LOG_FORMAT") {
            match format.to_lowercase().as_str() {
                "json" => self.observability.log_format = LogFormat::Json,
                "pretty" => self.observability.log_format = LogFormat::Pretty,
                _ => {} // ignore unrecognized values, keep file/default
            }
        }
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            if !endpoint.is_empty() {
                self.observability.otlp_endpoint = Some(endpoint);
            }
        }
        if let Ok(addr) = std::env::var("FERROCACHE_BIND_ADDR") {
            self.server.bind_addr = addr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.server.bind_addr, "127.0.0.1:6379");
        assert_eq!(c.server.memory_limit_mb, 100);
        assert_eq!(c.server.memory_limit_bytes(), 100 * 1024 * 1024);
        assert_eq!(c.server.reaper_interval_secs, 60);
        assert_eq!(c.observability.log_filter, "info");
        assert_eq!(c.observability.log_format, LogFormat::Pretty);
        assert_eq!(c.observability.otlp_endpoint, None);
        assert_eq!(c.observability.trace_sample_ratio, 1.0);
        assert_eq!(c.observability.metric_export_interval_secs, 60);
    }

    #[test]
    fn sample_ratio_is_clamped() {
        let mut c = Config::default();
        c.observability.trace_sample_ratio = 1.5;
        assert_eq!(c.observability.sample_ratio_clamped(), 1.0);
        c.observability.trace_sample_ratio = -0.2;
        assert_eq!(c.observability.sample_ratio_clamped(), 0.0);
        c.observability.trace_sample_ratio = 0.1;
        assert_eq!(c.observability.sample_ratio_clamped(), 0.1);
    }

    #[test]
    fn parses_partial_toml_with_defaults_for_rest() {
        // Only specify some fields; the rest must fall back to defaults.
        let toml = r#"
            [observability]
            otlp_endpoint = "http://collector:4317"
            log_format = "json"
        "#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            c.observability.otlp_endpoint.as_deref(),
            Some("http://collector:4317")
        );
        assert_eq!(c.observability.log_format, LogFormat::Json);
        // Untouched fields keep defaults.
        assert_eq!(c.observability.log_filter, "info");
        assert_eq!(c.server.bind_addr, "127.0.0.1:6379");
    }

    #[test]
    fn parses_full_toml() {
        let toml = r#"
            [server]
            bind_addr = "0.0.0.0:6380"
            memory_limit_mb = 512
            reaper_interval_secs = 30

            [observability]
            log_filter = "ferrocache=debug"
            log_format = "json"
            otlp_endpoint = "http://localhost:4317"
        "#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.server.bind_addr, "0.0.0.0:6380");
        assert_eq!(c.server.memory_limit_mb, 512);
        assert_eq!(c.server.reaper_interval_secs, 30);
        assert_eq!(c.observability.log_filter, "ferrocache=debug");
        assert_eq!(c.observability.log_format, LogFormat::Json);
    }

    #[test]
    fn unknown_field_is_rejected() {
        // deny_unknown_fields catches typos in config keys early.
        let toml = r#"
            [server]
            bind_address = "0.0.0.0:6380"
        "#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }
}
