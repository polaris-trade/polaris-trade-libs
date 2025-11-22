use http::Extensions;
use opentelemetry::trace::Status;
use reqwest::{Request, Response};
use reqwest_middleware::Result;
use reqwest_tracing::{
    ReqwestOtelSpanBackend, TracingMiddleware, default_on_request_end, reqwest_otel_span,
};
use std::time::Instant;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub struct TimeTrace;

impl ReqwestOtelSpanBackend for TimeTrace {
    fn on_request_start(req: &Request, extension: &mut Extensions) -> Span {
        // record start time
        extension.insert(Instant::now());

        let url = req.url();
        let host = url.host_str().unwrap_or_default();
        let full_url = url.as_str();
        let target = url.path().to_string();

        reqwest_otel_span!(
            name = "http_client_request",
            req,
            net.peer.name = %host,
            http.url = %full_url,
            http.target = %target,
            time_elapsed = tracing::field::Empty,
            request_id = tracing::field::Empty,
            retry_count = tracing::field::Empty,
            http.status_code.string = tracing::field::Empty
        )
    }

    fn on_request_end(span: &Span, outcome: &Result<Response>, extension: &mut Extensions) {
        let elapsed = extension
            .get::<Instant>()
            .map(|s| s.elapsed().as_millis() as i64)
            .unwrap_or_default();

        default_on_request_end(span, outcome);

        if let Ok(res) = outcome {
            let status = res.status();
            tracing::info!("Request ended with status: {}", status.as_u16());
            span.record("http.status_code", status.as_u16());

            // Set the OpenTelemetry span status based on HTTP status
            let otel_status = if status.is_success() {
                Status::Ok
            } else if status.is_server_error() {
                Status::error("Server error")
            } else if status.is_client_error() {
                Status::error("Client error")
            } else {
                Status::Ok
            };

            span.set_status(otel_status);
        } else {
            span.record("http.status_code", 0);
            span.set_status(Status::error("Request failed"));
        }

        span.record("time_elapsed", elapsed);
    }
}

/// Construct the middleware to be used in HttpClientBuilder
pub fn tracing_middleware() -> TracingMiddleware<TimeTrace> {
    TracingMiddleware::<TimeTrace>::new()
}
