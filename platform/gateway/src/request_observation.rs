//! Request identity and timings are shared by HTTP, MCP and internal assertions.
use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use opentelemetry::{
    propagation::{Extractor, TextMapPropagator},
    trace::TraceContextExt,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use veoveo_audit_contract::{AuditRequest, AuditRequestId, AuditSpanId, AuditTraceId};

#[derive(Debug, Clone)]
pub struct RequestObservation {
    pub audit: AuditRequest,
    timings: Arc<Timings>,
}
#[derive(Debug, Default)]
struct Timings {
    policy: AtomicU64,
    audit: AtomicU64,
    upstream: AtomicU64,
}
#[derive(Clone, Copy)]
pub enum RequestStage {
    Policy,
    Audit,
    Upstream,
}
tokio::task_local! {static CURRENT:RequestObservation;}
impl RequestObservation {
    pub fn current() -> Option<Self> {
        CURRENT.try_with(Clone::clone).ok()
    }
    pub async fn scope<F: std::future::Future>(self, future: F) -> F::Output {
        CURRENT.scope(self, future).await
    }
    pub fn record(&self, stage: RequestStage, elapsed: std::time::Duration) {
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        let counter = match stage {
            RequestStage::Policy => &self.timings.policy,
            RequestStage::Audit => &self.timings.audit,
            RequestStage::Upstream => &self.timings.upstream,
        };
        counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(nanos))
            })
            .ok();
        histogram(stage).record(elapsed.as_secs_f64(), &[]);
    }
}
fn histogram(stage: RequestStage) -> &'static opentelemetry::metrics::Histogram<f64> {
    static POLICY: OnceLock<opentelemetry::metrics::Histogram<f64>> = OnceLock::new();
    static AUDIT: OnceLock<opentelemetry::metrics::Histogram<f64>> = OnceLock::new();
    static UPSTREAM: OnceLock<opentelemetry::metrics::Histogram<f64>> = OnceLock::new();
    let (slot, name) = match stage {
        RequestStage::Policy => (&POLICY, "veoveo.gateway.policy.duration"),
        RequestStage::Audit => (&AUDIT, "veoveo.gateway.audit.duration"),
        RequestStage::Upstream => (&UPSTREAM, "veoveo.gateway.upstream.duration"),
    };
    slot.get_or_init(|| {
        opentelemetry::global::meter("veoveo.gateway")
            .f64_histogram(name)
            .with_unit("s")
            .build()
    })
}
pub fn record_stage(stage: RequestStage, elapsed: std::time::Duration) {
    if let Some(request) = RequestObservation::current() {
        request.record(stage, elapsed)
    } else {
        histogram(stage).record(elapsed.as_secs_f64(), &[])
    }
}
struct Headers<'a>(&'a HeaderMap);
impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.to_str().ok()
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|name| name.as_str()).collect()
    }
}
fn request_identity(headers: &HeaderMap, source_ip: Option<IpAddr>) -> (AuditRequest, u8) {
    let parent = TraceContextPropagator::new().extract(&Headers(headers));
    let span = parent.span();
    let context = span.span_context();
    let (trace, flags) = if context.is_valid() {
        (
            context.trace_id().to_string(),
            context.trace_flags().to_u8(),
        )
    } else {
        (uuid::Uuid::new_v4().simple().to_string(), 0)
    };
    let span = uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();
    (
        AuditRequest {
            id: AuditRequestId::new(),
            trace_id: AuditTraceId::parse(trace).expect("SDK generates valid trace identity"),
            span_id: AuditSpanId::parse(span).expect("UUID version bits make this span nonzero"),
            source_ip,
        },
        flags,
    )
}
pub async fn observe_request(mut request: Request, next: Next) -> Response {
    // Forwarded headers are not trusted merely because they are present. The socket
    // peer is always attributable; installation proxy trust requires explicit admission.
    let source = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| peer.0.ip());
    let (audit, flags) = request_identity(request.headers(), source);
    let observation = RequestObservation {
        audit,
        timings: Arc::default(),
    };
    request.extensions_mut().insert(observation.clone());
    let traceparent = format!(
        "00-{}-{}-{flags:02x}",
        observation.audit.trace_id, observation.audit.span_id
    );
    request.headers_mut().insert(
        "traceparent",
        HeaderValue::from_str(&traceparent).expect("typed trace header"),
    );
    let started = Instant::now();
    let mut response = observation.clone().scope(next.run(request)).await;
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&observation.audit.id.to_string()).expect("UUID header"),
    );
    response.headers_mut().insert(
        "traceparent",
        HeaderValue::from_str(&traceparent).expect("typed trace header"),
    );
    tracing::info!(request_id=%observation.audit.id,trace_id=%observation.audit.trace_id,span_id=%observation.audit.span_id,
  policy_ms=observation.timings.policy.load(Ordering::Relaxed) as f64/1_000_000.,
  audit_commit_ms=observation.timings.audit.load(Ordering::Relaxed) as f64/1_000_000.,
  upstream_ms=observation.timings.upstream.load(Ordering::Relaxed) as f64/1_000_000.,
  request_ms=started.elapsed().as_secs_f64()*1000.,status=response.status().as_u16(),"gateway request completed");
    response
}

/// RAII covers successful, denied, failed and cancelled operations uniformly.
pub struct StageTimer {
    stage: RequestStage,
    started: Instant,
    request: Option<RequestObservation>,
}
impl StageTimer {
    pub fn start(stage: RequestStage) -> Self {
        Self {
            stage,
            started: Instant::now(),
            request: RequestObservation::current(),
        }
    }
}
impl Drop for StageTimer {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        if let Some(request) = &self.request {
            request.record(self.stage, elapsed)
        } else {
            histogram(self.stage).record(elapsed.as_secs_f64(), &[])
        }
    }
}
