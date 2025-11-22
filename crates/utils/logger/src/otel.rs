use crate::{LoggingError, OpenTelemetryLayer};
use config_loader::{app_config::BaseAppConfig, logging::OtelConfig};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    trace::{RandomIdGenerator, Sampler},
};
pub use time::UtcOffset;
use tracing_subscriber::Registry;

pub fn setup_otel(
    app_config: BaseAppConfig,
    otel_config: OtelConfig,
) -> Result<
    (
        OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>,
        opentelemetry_sdk::trace::SdkTracerProvider,
        opentelemetry_sdk::logs::SdkLoggerProvider,
        opentelemetry_sdk::metrics::SdkMeterProvider,
    ),
    LoggingError,
> {
    use std::time::Duration;

    let app_name = app_config.name.clone();
    let otel_endpoint = otel_config.endpoint.clone();
    const MAX_QUEUE_SIZE: usize = 65536; // Max queue size for batching
    const EXPORT_DELAY: Duration = Duration::from_millis(200); // Delay between export attempts

    // Setup trace exporter for spans with timeout
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&otel_endpoint)
        .with_protocol(Protocol::Grpc)
        .with_timeout(Duration::from_secs(3)) // 3 second timeout for export
        .build()
        .map_err(|e| LoggingError::OtelExporterBuilderError(e.to_string()))?;

    // Create resource with service name
    let resource = Resource::builder()
        .with_service_name(app_name.clone())
        .build();

    // Configure batch span processor with error-resilient settings
    let batch_config = opentelemetry_sdk::trace::BatchConfigBuilder::default()
        .with_max_queue_size(MAX_QUEUE_SIZE)
        .with_scheduled_delay(EXPORT_DELAY)
        .with_max_export_batch_size(512) // Batch size
        .build();

    let batch_processor =
        opentelemetry_sdk::trace::BatchSpanProcessor::new(trace_exporter, batch_config);

    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_max_events_per_span(64)
        .with_max_attributes_per_span(16)
        .with_resource(resource.clone())
        .build();

    let tracer: opentelemetry_sdk::trace::Tracer = tracer_provider.tracer(app_name.clone());
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    // Setup log exporter with timeout
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(&otel_endpoint)
        .with_protocol(Protocol::Grpc)
        .with_timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| LoggingError::OtelExporterBuilderError(e.to_string()))?;

    // Configure batch log processor
    let log_batch_config = opentelemetry_sdk::logs::BatchConfigBuilder::default()
        .with_max_queue_size(MAX_QUEUE_SIZE)
        .with_scheduled_delay(EXPORT_DELAY)
        .with_max_export_batch_size(512)
        .build();

    let log_batch_processor = opentelemetry_sdk::logs::BatchLogProcessor::builder(log_exporter)
        .with_batch_config(log_batch_config)
        .build();

    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_log_processor(log_batch_processor)
        .with_resource(resource.clone())
        .build();

    // Setup metrics exporter with timeout
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&otel_endpoint)
        .with_protocol(Protocol::Grpc)
        .with_timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| LoggingError::OtelExporterBuilderError(e.to_string()))?;

    // Configure periodic streams for metrics
    let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_reader(
            opentelemetry_sdk::metrics::PeriodicReader::builder(metric_exporter)
                .with_interval(Duration::from_secs(60)) // Export every 60 seconds
                .build(),
        )
        .build();

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // Create the OpenTelemetry layer for automatic span capturing
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((
        telemetry_layer,
        tracer_provider,
        logger_provider,
        meter_provider,
    ))
}
