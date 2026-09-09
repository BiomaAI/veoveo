//! Bounded S3 enumeration closes uncertain create/part acknowledgements.

use super::*;
use object_store::{
    aws::{AmazonS3, AwsAuthorizer},
    client::{HttpClient, HttpConnector, HttpRequestBody, ReqwestConnector},
};
use serde::{Deserialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;

const PAGE_SIZE: usize = 64;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub(super) enum Reconciliation {
    S3(Arc<S3Multipart>),
    #[cfg(test)]
    MemoryFixture,
}

pub(super) struct S3Multipart {
    store: Arc<AmazonS3>,
    client: HttpClient,
    bucket_url: Url,
    region: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UploadList {
    #[serde(default)]
    upload: Vec<Upload>,
    is_truncated: bool,
    next_key_marker: Option<String>,
    next_upload_id_marker: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Upload {
    key: String,
    upload_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PartList {
    #[serde(default)]
    part: Vec<Part>,
    is_truncated: bool,
    next_part_number_marker: Option<u32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Part {
    part_number: u32,
    size: u64,
    #[serde(rename = "ETag")]
    etag: String,
}

fn backend(error: impl std::fmt::Display) -> BlobStoreError {
    BlobStoreError::Backend(error.to_string())
}

impl S3Multipart {
    pub(super) fn new(
        store: Arc<AmazonS3>,
        endpoint: Option<&str>,
        bucket: &str,
        region: &str,
        allow_http: bool,
    ) -> Result<Self, BlobStoreError> {
        let mut bucket_url = Url::parse(
            &endpoint
                .map(str::to_owned)
                .unwrap_or_else(|| format!("https://s3.{region}.amazonaws.com")),
        )
        .map_err(backend)?;
        if !matches!(bucket_url.scheme(), "http" | "https")
            || bucket_url.query().is_some()
            || bucket_url.fragment().is_some()
            || !bucket_url.username().is_empty()
            || bucket_url.password().is_some()
            || bucket.is_empty()
            || bucket.contains('/')
        {
            return Err(backend("invalid S3 multipart endpoint or bucket"));
        }
        bucket_url
            .path_segments_mut()
            .map_err(|_| backend("S3 endpoint cannot hold a bucket path"))?
            .pop_if_empty()
            .push(bucket);
        let client = ReqwestConnector::default()
            .connect(
                &object_store::ClientOptions::new()
                    .with_allow_http(allow_http)
                    .with_timeout(Duration::from_secs(30)),
            )
            .map_err(map_store_error)?;
        Ok(Self {
            store,
            client,
            bucket_url,
            region: region.to_owned(),
        })
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T, BlobStoreError> {
        let mut request = axum::http::Request::builder()
            .method("GET")
            .uri(url.as_str())
            .body(HttpRequestBody::empty())
            .map_err(backend)?;
        let credential = self
            .store
            .credentials()
            .get_credential()
            .await
            .map_err(map_store_error)?;
        AwsAuthorizer::new(&credential, "s3", &self.region)
            .try_authorize(&mut request, None)
            .map_err(map_store_error)?;
        let response = self.client.execute(request).await.map_err(backend)?;
        if !response.status().is_success() {
            // Provider bodies may contain object paths, identifiers, and request
            // credentials. Keep the status only; callers map this to safe errors.
            return Err(backend(format!(
                "S3 multipart enumeration returned {}",
                response.status()
            )));
        }
        let mut stream = response.into_body().bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(backend)?;
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(backend(
                    "S3 multipart enumeration response exceeds its bound",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        quick_xml::de::from_reader(bytes.as_slice()).map_err(backend)
    }

    pub(super) async fn remove_orphans(
        &self,
        object_key: &str,
        retained: Option<&str>,
    ) -> Result<(), BlobStoreError> {
        let mut marker: Option<(String, String)> = None;
        loop {
            let mut url = self.bucket_url.clone();
            url.query_pairs_mut()
                .append_pair("uploads", "")
                .append_pair("prefix", object_key)
                .append_pair("max-uploads", &PAGE_SIZE.to_string());
            if let Some((key, id)) = &marker {
                url.query_pairs_mut()
                    .append_pair("key-marker", key)
                    .append_pair("upload-id-marker", id);
            }
            let page: UploadList = self.get(url).await?;
            if page.upload.len() > PAGE_SIZE {
                return Err(backend("S3 exceeded the requested multipart page size"));
            }
            for upload in page.upload {
                if upload.key != object_key || retained == Some(upload.upload_id.as_str()) {
                    continue;
                }
                if upload.upload_id.is_empty() || upload.upload_id.len() > 4096 {
                    return Err(backend("invalid S3 multipart identifier"));
                }
                use object_store::multipart::MultipartStore;
                match self
                    .store
                    .abort_multipart(&ArtifactObjectStore::path(object_key)?, &upload.upload_id)
                    .await
                {
                    Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
                    Err(error) if error.to_string().contains("NoSuchUpload") => {}
                    Err(error) => return Err(map_store_error(error)),
                }
            }
            if !page.is_truncated {
                return Ok(());
            }
            let next = (
                page.next_key_marker
                    .ok_or_else(|| backend("missing S3 multipart key marker"))?,
                page.next_upload_id_marker
                    .ok_or_else(|| backend("missing S3 multipart upload marker"))?,
            );
            if marker.as_ref() == Some(&next) {
                return Err(backend("S3 multipart enumeration did not advance"));
            }
            marker = Some(next);
        }
    }

    pub(super) async fn content_ids(
        &self,
        object_key: &str,
        multipart_id: &str,
        expected: &[multipart::StoredUploadPart],
    ) -> Result<Vec<String>, BlobStoreError> {
        let mut ids = Vec::with_capacity(expected.len());
        let mut marker = 0_u32;
        loop {
            let mut url = self.bucket_url.clone();
            {
                let mut path = url
                    .path_segments_mut()
                    .map_err(|_| backend("invalid S3 object path"))?;
                for segment in object_key.split('/') {
                    path.push(segment);
                }
            }
            url.query_pairs_mut()
                .append_pair("uploadId", multipart_id)
                .append_pair("max-parts", &PAGE_SIZE.to_string())
                .append_pair("part-number-marker", &marker.to_string());
            let page: PartList = self.get(url).await?;
            if page.part.len() > PAGE_SIZE {
                return Err(backend("S3 exceeded the requested part page size"));
            }
            for part in page.part {
                let expected = expected
                    .get(ids.len())
                    .ok_or_else(|| backend("S3 contains an unaccepted upload part"))?;
                if part.part_number != expected.number.get()
                    || part.size != expected.byte_len
                    || part.etag.is_empty()
                    || part.etag.len() > 4096
                {
                    return Err(backend("S3 part differs from the accepted upload manifest"));
                }
                ids.push(part.etag);
            }
            if !page.is_truncated {
                break;
            }
            let next = page
                .next_part_number_marker
                .ok_or_else(|| backend("missing S3 part marker"))?;
            if next <= marker {
                return Err(backend("S3 part enumeration did not advance"));
            }
            marker = next;
        }
        if ids.len() != expected.len() {
            return Err(backend("S3 is missing an accepted upload part"));
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn s3_xml_decodes_bounded_profiles_and_quoted_etags() {
        let uploads: UploadList = quick_xml::de::from_str("<ListMultipartUploadsResult><IsTruncated>false</IsTruncated><Upload><Key>tenants/one/uploads/two</Key><UploadId>opaque</UploadId></Upload></ListMultipartUploadsResult>").unwrap();
        assert_eq!(uploads.upload.len(), 1);
        let parts: PartList = quick_xml::de::from_str("<ListPartsResult><IsTruncated>false</IsTruncated><Part><PartNumber>1</PartNumber><Size>16777216</Size><ETag>&quot;etag&quot;</ETag></Part></ListPartsResult>").unwrap();
        assert_eq!(parts.part[0].etag, "\"etag\"");
        assert_eq!(parts.part[0].size, 16777216);
    }
}
