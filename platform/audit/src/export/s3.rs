//! Immutable two-object delivery through signed S3 requests. A lost PUT response
//! reconciles the same key by GET and hash before another conditional PUT.
use super::{
    Bundle, ExportError,
    config::{ObjectLock, S3Config, destination_id, endpoint},
    digest, response_bytes,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use object_store::{
    aws::{AmazonS3, AmazonS3Builder, AwsAuthorizer},
    client::HttpRequestBody,
};
use sha2::{Digest, Sha256};
use url::Url;
use veoveo_audit_contract::{AuditBlock, AuditDestinationId};

pub(super) struct S3Destination {
    pub id: AuditDestinationId,
    config: S3Config,
    signer: AmazonS3,
    http: reqwest::Client,
}
impl S3Destination {
    pub fn new(config: S3Config, http: reqwest::Client) -> Result<Self, ExportError> {
        endpoint(&config.endpoint, config.allow_http)?;
        if config.region.is_empty()
            || config.region.len() > 64
            || !config
                .region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ExportError::Configuration("invalid audit S3 region"));
        }
        if let ObjectLock::Compliance { days } = &config.object_lock {
            Utc::now()
                .checked_add_signed(chrono::TimeDelta::days(i64::from(days.get())))
                .ok_or(ExportError::Configuration(
                    "audit Object Lock retention exceeds UTC range",
                ))?;
        }
        let signer = AmazonS3Builder::from_env()
            .with_endpoint(config.endpoint.as_str())
            .with_region(&config.region)
            .with_bucket_name(config.bucket.as_str())
            .with_allow_http(config.allow_http)
            .with_virtual_hosted_style_request(false)
            .build()
            .map_err(|_| {
                ExportError::Configuration("cannot initialize audit S3 credentials or endpoint")
            })?;
        Ok(Self {
            id: destination_id("s3", &config)?,
            config,
            signer,
            http,
        })
    }
    fn object_url(&self, block: &AuditBlock, suffix: &str) -> Result<Url, ExportError> {
        let mut url = self.config.endpoint.clone();
        let mut segments = url.path_segments_mut().map_err(|_| {
            ExportError::Configuration("audit S3 endpoint cannot hold object paths")
        })?;
        segments.pop_if_empty().push(self.config.bucket.as_str());
        for part in self.config.prefix.as_str().split('/') {
            segments.push(part);
        }
        segments
            .push(self.id.as_str())
            .push(digest(block.head.partition.storage_key().as_bytes()).as_str())
            .push(&format!(
                "{}-{}.{}",
                block.head.sequence, block.head_hash, suffix
            ));
        drop(segments);
        Ok(url)
    }
    pub async fn deliver(&self, block: &AuditBlock, bundle: &Bundle) -> Result<(), ExportError> {
        let until = match self.config.object_lock {
            ObjectLock::Disabled => None,
            ObjectLock::Compliance { days } => {
                let until = block
                    .head
                    .sealed_at
                    .checked_add_signed(chrono::TimeDelta::days(i64::from(days.get())))
                    .ok_or(ExportError::Configuration(
                        "audit Object Lock retention exceeds UTC range",
                    ))?;
                if until <= Utc::now() {
                    return Err(ExportError::Configuration(
                        "audit block predates the configured Object Lock window; increase export retention",
                    ));
                }
                Some(until)
            }
        };
        self.put_checked(
            self.object_url(block, "ocsf.jsonl")?,
            &bundle.content,
            "application/x-ndjson",
            until,
        )
        .await?;
        self.put_checked(
            self.object_url(block, "seal.json")?,
            &bundle.seal,
            "application/json",
            until,
        )
        .await
    }
    async fn request(
        &self,
        method: http::Method,
        url: &Url,
        body: &[u8],
        content_type: Option<&str>,
        until: Option<DateTime<Utc>>,
    ) -> Result<reqwest::Response, ExportError> {
        let mut builder = http::Request::builder()
            .method(method.clone())
            .uri(url.as_str());
        if let Some(content_type) = content_type {
            builder = builder
                .header("content-type", content_type)
                .header("if-none-match", "*")
                .header(
                    "x-amz-checksum-sha256",
                    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body)),
                );
        }
        if let Some(until) = until {
            builder = builder
                .header("x-amz-object-lock-mode", "COMPLIANCE")
                .header(
                    "x-amz-object-lock-retain-until-date",
                    until.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                );
        }
        let mut request = builder
            .body(HttpRequestBody::from(body.to_vec()))
            .map_err(|_| ExportError::Protocol)?;
        let credential = self
            .signer
            .credentials()
            .get_credential()
            .await
            .map_err(|_| ExportError::Unavailable)?;
        AwsAuthorizer::new(&credential, "s3", &self.config.region)
            .try_authorize(&mut request, None)
            .map_err(|_| ExportError::Unavailable)?;
        self.http
            .request(method, url.clone())
            .headers(request.headers().clone())
            .body(body.to_vec())
            .send()
            .await
            .map_err(|_| ExportError::Unavailable)
    }
    async fn verify(
        &self,
        url: &Url,
        bytes: &[u8],
        until: Option<DateTime<Utc>>,
    ) -> Result<bool, ExportError> {
        let response = self
            .request(http::Method::GET, url, &[], None, None)
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        super::successful(response.status())?;
        if let Some(until) = until {
            let headers = response.headers();
            let mode = headers
                .get("x-amz-object-lock-mode")
                .and_then(|h| h.to_str().ok());
            let retention = headers
                .get("x-amz-object-lock-retain-until-date")
                .and_then(|h| h.to_str().ok())
                .and_then(|h| DateTime::parse_from_rfc3339(h).ok());
            let version = headers
                .get("x-amz-version-id")
                .and_then(|h| h.to_str().ok());
            if mode != Some("COMPLIANCE")
                || retention.is_none_or(|date| date < until)
                || version.is_none_or(|v| v.is_empty() || v == "null")
            {
                return Err(ExportError::ObjectLock);
            }
        }
        let actual = response_bytes(response, bytes.len()).await?;
        if actual.len() != bytes.len() || digest(&actual) != digest(bytes) {
            return Err(ExportError::Conflict);
        }
        Ok(true)
    }
    async fn put_checked(
        &self,
        url: Url,
        bytes: &[u8],
        content_type: &str,
        until: Option<DateTime<Utc>>,
    ) -> Result<(), ExportError> {
        if self.verify(&url, bytes, until).await? {
            return Ok(());
        }
        let response = self
            .request(http::Method::PUT, &url, bytes, Some(content_type), until)
            .await?;
        // Bucket creation may still be converging during installation bootstrap.
        // The same conditional object identity is safe to reconcile again.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ExportError::Unavailable);
        }
        if response.status() != reqwest::StatusCode::PRECONDITION_FAILED {
            super::successful(response.status())?;
        }
        if !self.verify(&url, bytes, until).await? {
            return Err(ExportError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode},
        routing::get,
    };
    use std::{
        collections::BTreeMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::Duration,
    };
    #[derive(Default)]
    struct Fixture {
        objects: Mutex<BTreeMap<String, Vec<u8>>>,
        lose_ack: AtomicBool,
        puts: AtomicUsize,
    }
    async fn read(
        State(state): State<Arc<Fixture>>,
        Path(key): Path<String>,
    ) -> (StatusCode, Vec<u8>) {
        match state.objects.lock().unwrap().get(&key) {
            Some(bytes) => (StatusCode::OK, bytes.clone()),
            None => (StatusCode::NOT_FOUND, Vec::new()),
        }
    }
    async fn write(
        State(state): State<Arc<Fixture>>,
        Path(key): Path<String>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        assert_eq!(headers.get("if-none-match").unwrap(), "*");
        assert!(headers.contains_key("authorization"));
        assert!(headers.contains_key("x-amz-checksum-sha256"));
        let mut objects = state.objects.lock().unwrap();
        if objects.contains_key(&key) {
            return StatusCode::PRECONDITION_FAILED;
        }
        objects.insert(key, body.to_vec());
        state.puts.fetch_add(1, Ordering::SeqCst);
        if state.lose_ack.swap(false, Ordering::SeqCst) {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::OK
        }
    }
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    #[tokio::test]
    async fn uncertain_put_reconciles_same_bytes_and_compliance_requires_provider_proof() {
        tokio::time::timeout(Duration::from_secs(30), async {
            let state = Arc::new(Fixture::default());
            state.lose_ack.store(true, Ordering::SeqCst);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let app = Router::new()
                .route("/{*object}", get(read).put(write))
                .with_state(state.clone());
            let mut server = Server(tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            }));
            let config = S3Config {
                endpoint: format!("http://{address}").parse().unwrap(),
                region: "us-east-1".into(),
                bucket: "audit-test".to_owned().try_into().unwrap(),
                prefix: "audit".to_owned().try_into().unwrap(),
                object_lock: ObjectLock::Disabled,
                allow_http: true,
            };
            let signer = AmazonS3Builder::new()
                .with_endpoint(config.endpoint.as_str())
                .with_bucket_name(config.bucket.as_str())
                .with_region(&config.region)
                .with_allow_http(true)
                .with_access_key_id("fixture")
                .with_secret_access_key("fixture")
                .build()
                .unwrap();
            let mut destination = S3Destination {
                id: destination_id("s3", &config).unwrap(),
                config,
                signer,
                http: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .unwrap(),
            };
            let records = vec![super::super::tests::record(
                veoveo_audit_contract::AuditDetail::Read {
                    method: veoveo_audit_contract::AuditReadMethod::AuditView,
                },
                veoveo_audit_contract::AuditTarget::Installation,
                true,
            )];
            let block = crate::integrity::AuditSigningKey::from_seed(&[7; 32])
                .seal(
                    veoveo_audit_contract::AuditPartition::Installation,
                    None,
                    vec![veoveo_audit_contract::AuditBlockMember {
                        id: records[0].draft.id(),
                        versionstamp: veoveo_audit_contract::AuditVersionstamp::new(1).unwrap(),
                    }],
                    &records,
                    Utc::now(),
                )
                .unwrap();
            let bundle = Bundle::ocsf(&block, &records).unwrap();
            assert!(matches!(
                destination.deliver(&block, &bundle).await,
                Err(ExportError::Unavailable)
            ));
            destination.deliver(&block, &bundle).await.unwrap();
            destination.deliver(&block, &bundle).await.unwrap();
            assert_eq!(
                state.puts.load(Ordering::SeqCst),
                2,
                "one JSONL object and one seal; retry reads the previous PUT"
            );
            destination.config.object_lock = ObjectLock::Compliance {
                days: std::num::NonZeroU32::new(365).unwrap(),
            };
            assert!(matches!(
                destination.deliver(&block, &bundle).await,
                Err(ExportError::ObjectLock)
            ));
            destination.config.object_lock = ObjectLock::Disabled;
            for bytes in state.objects.lock().unwrap().values_mut() {
                bytes[0] ^= 1;
            }
            assert!(matches!(
                destination.deliver(&block, &bundle).await,
                Err(ExportError::Conflict)
            ));
            server.0.abort();
            let _ = (&mut server.0).await;
        })
        .await
        .expect("S3 export fixture exceeded 30 seconds");
    }
}
