//! Typed request receipts around the shared installed transport helpers.
use anyhow::Result;
use rmcp::{Peer, RoleClient, ServiceError};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use veoveo_testing_support::installed::{knowledge as installed, tools};
use veoveo_time_mcp::{
    AssessClockRequest, ConvertTimeRequest, EvaluateWindowsRequest, ResolveTimeRequest,
    TimeResource, TimeSourceId,
};
use veoveo_types::{ResourceAddress, Sha256Digest};

#[derive(Serialize)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
pub enum Request {
    Read(TimeResource),
    Resolve(ResolveTimeRequest),
    Convert(ConvertTimeRequest),
    AssessClock(AssessClockRequest),
    EvaluateWindows(EvaluateWindowsRequest),
    SourceRead(TimeSourceId),
}
impl From<ResolveTimeRequest> for Request {
    fn from(value: ResolveTimeRequest) -> Self {
        Self::Resolve(value)
    }
}
impl From<ConvertTimeRequest> for Request {
    fn from(value: ConvertTimeRequest) -> Self {
        Self::Convert(value)
    }
}
impl From<AssessClockRequest> for Request {
    fn from(value: AssessClockRequest) -> Self {
        Self::AssessClock(value)
    }
}
impl From<EvaluateWindowsRequest> for Request {
    fn from(value: EvaluateWindowsRequest) -> Self {
        Self::EvaluateWindows(value)
    }
}
impl Request {
    fn tool(&self) -> &'static str {
        match self {
            Self::Resolve(_) => "time__resolve_time",
            Self::Convert(_) => "time__convert_time",
            Self::AssessClock(_) => "time__assess_clock",
            Self::EvaluateWindows(_) => "time__evaluate_windows",
            _ => unreachable!("resource operations use the read helper"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    request: Request,
    vector_index: Option<usize>,
    completed: bool,
    http_status: Option<u16>,
    mcp_code: Option<i32>,
    response_sha256: Option<Sha256Digest>,
    response_complete: bool,
    failure_sha256: Option<Sha256Digest>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    observations: Vec<Observation>,
    #[serde(skip)]
    pub vector_index: Option<usize>,
}
impl Trace {
    pub fn begin(&mut self, request: Request) {
        self.observations.push(Observation {
            request,
            vector_index: self.vector_index,
            completed: false,
            http_status: None,
            mcp_code: None,
            response_sha256: None,
            response_complete: false,
            failure_sha256: None,
        });
    }
    pub fn http_status(&mut self, status: reqwest::StatusCode) {
        if let Some(last) = self.observations.last_mut() {
            last.http_status = Some(status.as_u16());
        }
    }
    pub fn http(&mut self, status: reqwest::StatusCode, body: &[u8]) {
        if let Some(last) = self.observations.last_mut() {
            last.http_status = Some(status.as_u16());
            last.response_sha256 = Some(hash(body));
            last.response_complete = true;
        }
    }
    pub fn finish<T: Serialize>(&mut self, result: &Result<T>) {
        if let Some(last) = self.observations.last_mut() {
            last.completed = true;
            match result {
                Ok(value) => {
                    if let Ok(bytes) = serde_json::to_vec(value) {
                        last.response_sha256.get_or_insert_with(|| hash(&bytes));
                        last.response_complete = true;
                    }
                }
                Err(error) => {
                    last.failure_sha256 = Some(hash(format!("{error:#}").as_bytes()));
                    for cause in error.chain() {
                        if let Some(ServiceError::McpError(data)) =
                            cause.downcast_ref::<ServiceError>()
                        {
                            last.mcp_code = Some(data.code.0);
                        }
                        if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
                            last.http_status = error
                                .status()
                                .map(|status| status.as_u16())
                                .or(last.http_status);
                        }
                    }
                }
            }
        }
    }
    pub async fn read<T: Serialize + DeserializeOwned>(
        &mut self,
        peer: &Peer<RoleClient>,
        resource: TimeResource,
    ) -> Result<T> {
        let uri = resource.to_uri()?;
        self.begin(Request::Read(resource));
        let result = installed::read(peer, &uri).await;
        self.finish(&result);
        result
    }
    pub async fn call<I: Clone + Serialize + Into<Request>, O: Serialize + DeserializeOwned>(
        &mut self,
        peer: &Peer<RoleClient>,
        input: &I,
    ) -> Result<O> {
        let request: Request = input.clone().into();
        let name = request.tool().parse()?;
        self.begin(request);
        let result = tools::call(peer, name, input).await;
        self.finish(&result);
        result
    }
}
fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_hex(hex::encode(Sha256::digest(bytes))).expect("SHA-256 emits 32 bytes")
}

#[test]
fn observations_preserve_request_index_and_codes_without_raw_errors() -> Result<()> {
    let mut trace = Trace {
        vector_index: Some(3),
        ..Trace::default()
    };
    trace.begin(Request::Resolve(ResolveTimeRequest {
        expression: veoveo_time_mcp::TimeExpressionValue::Tai {
            seconds_since_1970: 0,
            nanosecond: veoveo_time_mcp::SubsecondNanoseconds::MAX,
        }
        .build()?,
        additional_uncertainty_nanoseconds: 7,
    }));
    let error = ServiceError::McpError(rmcp::ErrorData::invalid_params(
        "private-provider-error",
        None,
    ));
    trace.finish(&Err::<(), _>(anyhow::Error::new(error)));
    let value = serde_json::to_value(&trace)?;
    anyhow::ensure!(value["observations"][0]["vectorIndex"] == 3);
    anyhow::ensure!(value["observations"][0]["mcpCode"] == -32602);
    anyhow::ensure!(
        value["observations"][0]["failureSha256"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    anyhow::ensure!(!serde_json::to_string(&trace)?.contains("private-provider-error"));
    trace.begin(Request::SourceRead(
        TimeSourceId::parse("time-source-fixture").map_err(anyhow::Error::msg)?,
    ));
    trace.http(reqwest::StatusCode::FORBIDDEN, b"private-http-body");
    trace.finish(&Err::<(), _>(anyhow::anyhow!(
        "source metadata read failed"
    )));
    let value = serde_json::to_value(&trace)?;
    anyhow::ensure!(value["observations"][1]["httpStatus"] == 403);
    anyhow::ensure!(
        value["observations"][1]["responseSha256"]
            == serde_json::to_value(hash(b"private-http-body"))?
    );
    anyhow::ensure!(!serde_json::to_string(&trace)?.contains("private-http-body"));
    Ok(())
}

#[test]
fn interrupted_http_body_has_status_without_a_complete_digest() -> Result<()> {
    let mut trace = Trace::default();
    trace.begin(Request::SourceRead(
        TimeSourceId::parse("time-source-fixture").map_err(anyhow::Error::msg)?,
    ));
    trace.http_status(reqwest::StatusCode::OK);
    let pending = serde_json::to_value(&trace)?;
    anyhow::ensure!(pending["observations"][0]["completed"] == false);
    anyhow::ensure!(pending["observations"][0]["responseComplete"] == false);
    anyhow::ensure!(pending["observations"][0]["responseSha256"].is_null());
    trace.finish(&Err::<(), _>(anyhow::anyhow!(
        "body exceeded fixture limit"
    )));
    let failed = serde_json::to_value(&trace)?;
    anyhow::ensure!(failed["observations"][0]["httpStatus"] == 200);
    anyhow::ensure!(failed["observations"][0]["completed"] == true);
    anyhow::ensure!(failed["observations"][0]["responseComplete"] == false);
    anyhow::ensure!(failed["observations"][0]["responseSha256"].is_null());
    Ok(())
}
