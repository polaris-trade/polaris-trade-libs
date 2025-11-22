#[cfg(feature = "file")]
pub mod file;
#[cfg(feature = "otel")]
pub mod otel;
pub mod tracing_unwrap;
pub mod util;
#[cfg(feature = "file")]
use crate::file::setup_file_appender;
#[cfg(feature = "otel")]
use crate::otel::setup_otel;
pub use crate::util::{utc_offset_hms, utc_offset_hours};
use config_loader::{app_config::BaseAppConfig, logging::LoggerConfig};
pub use time::UtcOffset;
use time::{format_description::BorrowedFormatItem, macros::format_description};
pub use tracing::{
    Level, debug, debug_span, error, error_span, info, info_span, instrument, span, trace,
    trace_span, warn, warn_span,
};

#[cfg(feature = "otel")]
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, Registry, fmt::time::OffsetTime, layer::SubscriberExt};

#[cfg(feature = "sysinfo")]
pub mod sysinfo;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoggingError {
    #[error("Failed to build layer: {message}, context: {context}")]
    BuildLayerError {
        message: String,
        context: &'static str,
    },
    #[error("Failed to build otel exporter: {0}")]
    OtelExporterBuilderError(String),
    #[error("Missing configuration: {0}")]
    MissingConfigurationError(String),
    #[error("Failed to get pid: {0}")]
    MissingPid(String),
}

pub struct LoggingGuard {
    #[cfg(feature = "file")]
    /// Need to keep the guard alive to keep the file appender open
    pub file_guard: tracing_appender::non_blocking::WorkerGuard,
    #[cfg(feature = "otel")]
    /// Keep tracer provider alive for proper shutdown
    pub tracer_provider: opentelemetry_sdk::trace::SdkTracerProvider,
    #[cfg(feature = "otel")]
    /// Keep logger provider alive for proper shutdown
    pub logger_provider: opentelemetry_sdk::logs::SdkLoggerProvider,
    #[cfg(feature = "otel")]
    /// Keep meter provider alive for proper shutdown
    pub meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    #[cfg(feature = "stdout")]
    /// Keep stdout guard alive to ensure all logs are flushed
    pub stdout_guard: tracing_appender::non_blocking::WorkerGuard,
}

impl Drop for LoggingGuard {
    /// Shutdown all logging providers gracefully
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Err(e) = self.tracer_provider.force_flush() {
            eprintln!("Failed to force flush tracer provider: {}", e);
        }

        #[cfg(feature = "otel")]
        if let Err(e) = self.logger_provider.force_flush() {
            eprintln!("Failed to force flush logger provider: {}", e);
        }

        #[cfg(feature = "otel")]
        if let Err(e) = self.meter_provider.force_flush() {
            eprintln!("Failed to force flush meter provider: {}", e);
        }
        #[cfg(feature = "otel")]
        if let Err(e) = self.tracer_provider.shutdown() {
            eprintln!("Failed to shutdown tracer provider: {}", e);
        }

        #[cfg(feature = "otel")]
        if let Err(e) = self.logger_provider.shutdown() {
            eprintln!("Failed to shutdown logger provider: {}", e);
        }

        #[cfg(feature = "otel")]
        if let Err(e) = self.meter_provider.shutdown() {
            eprintln!("Failed to shutdown meter provider: {}", e);
        }
    }
}

pub fn setup_logging(
    app_config: BaseAppConfig,
    logger_config: LoggerConfig,
    env_filter_override: Option<Vec<&str>>,
) -> Result<LoggingGuard, LoggingError> {
    let fmt: &[BorrowedFormatItem<'_>] = if cfg!(debug_assertions) {
        format_description!("[hour]:[minute]:[second].[subsecond digits:3]")
    } else {
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]")
    };

    let timezone = match app_config.timezone {
        Some(offset) => utc_offset_hours(offset),
        None => UtcOffset::UTC,
    };
    let timer = OffsetTime::new(timezone, fmt);

    let max_level = logger_config
        .max_level
        .parse::<Level>()
        .unwrap_or(Level::INFO);

    let mut env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if let Some(directives) = env_filter_override {
        for dir in directives {
            env_filter =
                env_filter.add_directive(dir.parse().expect("Invalid env filter directive"));
        }
    }

    let level_filter = tracing_subscriber::filter::LevelFilter::from_level(max_level);

    #[cfg(feature = "otel")]
    let (registry, tracer_provider, logger_provider, meter_provider) = {
        let otel_config = logger_config.otel.as_ref().ok_or_else(|| {
            LoggingError::MissingConfigurationError(
                "otel logger configuration is missing".to_string(),
            )
        })?;

        let base = Registry::default();
        let (otel_layer, tracer_provider, logger_provider, meter_provider) =
            setup_otel(app_config.clone(), otel_config.clone())?;
        let bridge = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
            &logger_provider,
        );
        (
            base.with(otel_layer).with(bridge),
            tracer_provider,
            logger_provider,
            meter_provider,
        )
    };

    #[cfg(not(feature = "otel"))]
    let registry = Registry::default();

    let registry = registry.with(env_filter).with(level_filter);

    #[cfg(feature = "file")]
    let (registry, file_guard) = {
        let file_config = logger_config.file.as_ref().ok_or_else(|| {
            LoggingError::MissingConfigurationError(
                "file logger configuration is missing".to_string(),
            )
        })?;

        let (non_blocking, guard) = setup_file_appender(app_config.clone(), file_config.clone())?;
        let file_layer = tracing_subscriber::fmt::Layer::default()
            .with_writer(non_blocking)
            .with_timer(timer.clone())
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true);
        (registry.with(file_layer), guard)
    };

    #[cfg(not(feature = "file"))]
    let registry = registry;

    #[cfg(feature = "stdout")]
    let (registry, stdout_guard) = {
        let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());

        let console_layer = tracing_subscriber::fmt::Layer::default()
            .with_writer(non_blocking)
            .with_timer(timer)
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true);
        (registry.with(console_layer), guard)
    };

    #[cfg(not(feature = "stdout"))]
    let registry = registry;

    if tracing::dispatcher::has_been_set() {
        warn!("Global trace dispatcher already set, skipping re-init");
    } else {
        tracing::subscriber::set_global_default(registry).map_err(|e| {
            LoggingError::BuildLayerError {
                message: e.to_string(),
                context: "init",
            }
        })?;
    }

    Ok(LoggingGuard {
        #[cfg(feature = "file")]
        file_guard,
        #[cfg(feature = "otel")]
        tracer_provider,
        #[cfg(feature = "otel")]
        logger_provider,
        #[cfg(feature = "otel")]
        meter_provider,
        #[cfg(feature = "stdout")]
        stdout_guard,
    })
}
