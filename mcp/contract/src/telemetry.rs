//! Shared telemetry setup for Veoveo MCP servers.

use anyhow::Result;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    logs::{BatchConfigBuilder as LogBatchConfigBuilder, BatchLogProcessor, SdkLoggerProvider},
    trace::{BatchConfigBuilder as TraceBatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider},
};
use std::{env, time::Duration};
use tracing_subscriber::{EnvFilter, prelude::*};

const OTEL_SDK_DISABLED: &str = "OTEL_SDK_DISABLED";
const OTEL_EXPORTER_OTLP_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT";
const OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT";
const EXPORT_SCHEDULE_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.logger_provider {
            let _ = provider.shutdown();
        }
        if let Some(provider) = &self.tracer_provider {
            let _ = provider.shutdown();
        }
    }
}

pub fn init_server_telemetry(
    service_name: &'static str,
    default_filter: &'static str,
) -> Result<TelemetryGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_ansi(false);

    let tracer_provider = if otlp_traces_enabled() {
        Some(build_tracer_provider(service_name)?)
    } else {
        None
    };
    let logger_provider = if otlp_logs_enabled() {
        Some(build_logger_provider(service_name)?)
    } else {
        None
    };

    match (&tracer_provider, &logger_provider) {
        (Some(tracer_provider), Some(logger_provider)) => {
            let tracer = tracer_provider.tracer(service_name);
            let trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            let log_filter = EnvFilter::new("info,opentelemetry=off,hyper=off,h2=off,reqwest=off");
            let log_layer =
                OpenTelemetryTracingBridge::new(logger_provider).with_filter(log_filter);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(trace_layer)
                .with(log_layer)
                .init();
        }
        (Some(tracer_provider), None) => {
            let tracer = tracer_provider.tracer(service_name);
            let trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(trace_layer)
                .init();
        }
        (None, Some(logger_provider)) => {
            let log_filter = EnvFilter::new("info,opentelemetry=off,hyper=off,h2=off,reqwest=off");
            let log_layer =
                OpenTelemetryTracingBridge::new(logger_provider).with_filter(log_filter);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(log_layer)
                .init();
        }
        (None, None) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }

    Ok(TelemetryGuard {
        tracer_provider,
        logger_provider,
    })
}

fn build_tracer_provider(service_name: &'static str) -> Result<SdkTracerProvider> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .build()?;
    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            TraceBatchConfigBuilder::default()
                .with_scheduled_delay(EXPORT_SCHEDULE_DELAY)
                .build(),
        )
        .build();
    let provider = SdkTracerProvider::builder()
        .with_resource(resource(service_name))
        .with_span_processor(processor)
        .build();
    global::set_tracer_provider(provider.clone());
    Ok(provider)
}

fn build_logger_provider(service_name: &'static str) -> Result<SdkLoggerProvider> {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .build()?;
    let processor = BatchLogProcessor::builder(exporter)
        .with_batch_config(
            LogBatchConfigBuilder::default()
                .with_scheduled_delay(EXPORT_SCHEDULE_DELAY)
                .build(),
        )
        .build();
    Ok(SdkLoggerProvider::builder()
        .with_resource(resource(service_name))
        .with_log_processor(processor)
        .build())
}

fn resource(service_name: &'static str) -> Resource {
    Resource::builder().with_service_name(service_name).build()
}

fn otlp_traces_enabled() -> bool {
    !sdk_disabled()
        && (env::var_os(OTEL_EXPORTER_OTLP_ENDPOINT).is_some()
            || env::var_os(OTEL_EXPORTER_OTLP_TRACES_ENDPOINT).is_some())
}

fn otlp_logs_enabled() -> bool {
    !sdk_disabled()
        && (env::var_os(OTEL_EXPORTER_OTLP_ENDPOINT).is_some()
            || env::var_os(OTEL_EXPORTER_OTLP_LOGS_ENDPOINT).is_some())
}

fn sdk_disabled() -> bool {
    env::var(OTEL_SDK_DISABLED)
        .map(|value| sdk_disabled_value(&value))
        .unwrap_or(false)
}

fn sdk_disabled_value(value: &str) -> bool {
    matches!(value, "true" | "TRUE" | "True" | "1")
}

#[cfg(test)]
mod tests {
    use super::sdk_disabled_value;

    #[test]
    fn sdk_disabled_accepts_spec_boolean_and_hard_false() {
        assert!(sdk_disabled_value("true"));
        assert!(sdk_disabled_value("TRUE"));
        assert!(sdk_disabled_value("1"));
        assert!(!sdk_disabled_value("false"));
    }
}
