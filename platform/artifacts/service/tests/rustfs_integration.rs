use std::{env, time::Duration};

use axum::body::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::{ObjectStoreExt, WriteMultipart};
use sha2::{Digest, Sha256};

const OBJECT_BYTES: usize = 18 * 1024 * 1024 + 137;
const PART_BYTES: usize = 6 * 1024 * 1024;
const RANGE_START: usize = 7 * 1024 * 1024 + 19;
const RANGE_BYTES: usize = 4096;
const OBJECT_KEY: &str = "veoveo-rustfs-upgrade/multipart.bin";

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required for the RustFS integration"))
}

fn payload() -> Vec<u8> {
    (0..OBJECT_BYTES)
        .map(|index| ((index * 31 + 17) % 251) as u8)
        .collect()
}

#[tokio::test]
#[ignore = "requires an explicitly managed RustFS container and persistent volume"]
async fn multipart_range_and_restart_persistence() {
    let endpoint = required("VEOVEO_RUSTFS_TEST_ENDPOINT");
    let bucket = required("VEOVEO_RUSTFS_TEST_BUCKET");
    let access_key = required("VEOVEO_RUSTFS_TEST_ACCESS_KEY");
    let secret_key = required("VEOVEO_RUSTFS_TEST_SECRET_KEY");
    let phase = required("VEOVEO_RUSTFS_TEST_PHASE");
    // External fixture ownership supplies synthetic pairs and performs the server
    // replacement over its retained volume between write and credential-verify.
    // This phase does not establish IAM/STS retirement or IAM-state preservation.
    let old_store = if phase == "credential-verify" {
        Some(
            AmazonS3Builder::new()
                .with_bucket_name(bucket.clone())
                .with_region("us-east-1")
                .with_access_key_id(required("VEOVEO_RUSTFS_TEST_OLD_ACCESS_KEY"))
                .with_secret_access_key(required("VEOVEO_RUSTFS_TEST_OLD_SECRET_KEY"))
                .with_endpoint(endpoint.clone())
                .with_allow_http(true)
                .build()
                .unwrap_or_else(|_| panic!("build retired-root fixture S3 client")),
        )
    } else {
        None
    };
    let store = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region("us-east-1")
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key)
        .with_endpoint(endpoint)
        .with_allow_http(true)
        .build()
        .unwrap_or_else(|_| panic!("build RustFS S3 client"));
    let path = Path::from(OBJECT_KEY);
    let expected = payload();

    if phase == "write" {
        let upload = store
            .put_multipart(&path)
            .await
            .unwrap_or_else(|_| panic!("start RustFS multipart upload"));
        let mut writer = WriteMultipart::new_with_chunk_size(upload, PART_BYTES);
        for chunk in expected.chunks(1024 * 1024) {
            writer.put(Bytes::copy_from_slice(chunk));
            writer
                .wait_for_capacity(4)
                .await
                .unwrap_or_else(|_| panic!("upload RustFS multipart part"));
        }
        writer
            .finish()
            .await
            .unwrap_or_else(|_| panic!("complete RustFS multipart upload"));
    } else if phase != "verify" && phase != "credential-verify" {
        panic!("VEOVEO_RUSTFS_TEST_PHASE must be `write`, `verify` or `credential-verify`");
    }

    let metadata = store
        .head(&path)
        .await
        .unwrap_or_else(|_| panic!("head persisted RustFS object"));
    assert_eq!(metadata.size, OBJECT_BYTES as u64);
    let range = store
        .get_range(
            &path,
            RANGE_START as u64..(RANGE_START + RANGE_BYTES) as u64,
        )
        .await
        .unwrap_or_else(|_| panic!("read persisted RustFS object range"));
    assert_eq!(
        range.as_ref(),
        &expected[RANGE_START..RANGE_START + RANGE_BYTES]
    );
    let actual = store
        .get(&path)
        .await
        .unwrap_or_else(|_| panic!("get persisted RustFS object"))
        .bytes()
        .await
        .unwrap_or_else(|_| panic!("read persisted RustFS object body"));
    assert_eq!(actual.as_ref(), expected.as_slice());

    if let Some(old_store) = old_store {
        assert_eq!(Sha256::digest(&actual), Sha256::digest(&expected));
        // object_store 0.14.1 maps only service HTTP 401/403 to these variants;
        // transport failures, timeouts and 404 must not prove retirement.
        match tokio::time::timeout(Duration::from_secs(15), old_store.head(&path)).await {
            Ok(Err(object_store::Error::Unauthenticated { .. }))
            | Ok(Err(object_store::Error::PermissionDenied { .. })) => {}
            Ok(Ok(_)) => panic!("retired root credentials still read the retained object"),
            Ok(Err(_)) => panic!("retired-root HEAD did not return service HTTP 401/403"),
            Err(_) => panic!("retired-root HEAD exceeded its 15-second deadline"),
        }
    }

    if phase == "verify" || phase == "credential-verify" {
        store
            .delete(&path)
            .await
            .unwrap_or_else(|_| panic!("delete RustFS acceptance object"));
    }
}
