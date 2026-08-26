use std::env;

use axum::body::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::{ObjectStoreExt, WriteMultipart};

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
    let store = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region("us-east-1")
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key)
        .with_endpoint(endpoint)
        .with_allow_http(true)
        .build()
        .expect("build RustFS S3 client");
    let path = Path::from(OBJECT_KEY);
    let expected = payload();

    if phase == "write" {
        let upload = store
            .put_multipart(&path)
            .await
            .expect("start RustFS multipart upload");
        let mut writer = WriteMultipart::new_with_chunk_size(upload, PART_BYTES);
        for chunk in expected.chunks(1024 * 1024) {
            writer.put(Bytes::copy_from_slice(chunk));
            writer
                .wait_for_capacity(4)
                .await
                .expect("upload RustFS multipart part");
        }
        writer
            .finish()
            .await
            .expect("complete RustFS multipart upload");
    } else if phase != "verify" {
        panic!("VEOVEO_RUSTFS_TEST_PHASE must be `write` or `verify`, got `{phase}`");
    }

    let metadata = store
        .head(&path)
        .await
        .expect("head persisted RustFS object");
    assert_eq!(metadata.size, OBJECT_BYTES as u64);
    let range = store
        .get_range(
            &path,
            RANGE_START as u64..(RANGE_START + RANGE_BYTES) as u64,
        )
        .await
        .expect("read persisted RustFS object range");
    assert_eq!(
        range.as_ref(),
        &expected[RANGE_START..RANGE_START + RANGE_BYTES]
    );
    let actual = store
        .get(&path)
        .await
        .expect("get persisted RustFS object")
        .bytes()
        .await
        .expect("read persisted RustFS object body");
    assert_eq!(actual.as_ref(), expected.as_slice());

    if phase == "verify" {
        store
            .delete(&path)
            .await
            .expect("delete RustFS acceptance object");
    }
}
