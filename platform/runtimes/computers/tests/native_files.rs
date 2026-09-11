//! File bytes traverse the real provider under the guest's native confinement.
#![allow(dead_code)] // Shared fixtures expose other scenarios' operations.
#[path = "native_support/block_home.rs"]
mod block_home;
mod native_support;
#[path = "native_support/template.rs"]
mod template;

use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computer_execution::{FileFailure, FileReceipt, FileRequest};
use veoveo_computers_runtime::*;

async fn import(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    ready: &Observation,
    request: &FileRequest,
    bytes: &[u8],
) -> Result<FileTransferResult> {
    runtime
        .transfer_file(
            binding,
            ready,
            request,
            Some(&mut Cursor::new(bytes)),
            15,
            |_| async { panic!("an import must not produce file bytes") },
        )
        .await
}

async fn export(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    ready: &Observation,
    path: &str,
    maximum: u64,
) -> (Result<FileTransferResult>, Vec<u8>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let sink = output.clone();
    let request = FileRequest::export(path.into(), maximum).unwrap();
    let result = runtime
        .transfer_file(binding, ready, &request, None, 15, move |bytes| {
            sink.lock().unwrap().extend(bytes);
            async { Ok(()) }
        })
        .await;
    (
        result,
        Arc::try_unwrap(output).unwrap().into_inner().unwrap(),
    )
}

#[tokio::test]
#[ignore = "requires exact native provider binaries and files/v1 template; owns isolated retained storage"]
async fn regular_file_transfer_verifies_bytes_retention_and_uncertain_input() {
    let mut provider = native_support::Provider::start_with_execution_logging().await;
    let runtime = &provider.runtime;
    let selected = template::retained_template(provider.image.clone());
    let computer = Uuid::now_v7();
    let binding = Binding::new(computer, selected.fingerprint()).unwrap();
    let home =
        block_home::BlockHome::create(provider.dir.clone(), provider.image.clone(), computer);
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding.clone()).unwrap();
    let created = runtime.create(&binding, &selected).await.unwrap();
    let ready = runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(30))
        .await
        .unwrap();
    home.assert_registered_no_copy();

    // An archive is an opaque regular file; the helper never extracts its contents.
    let path = "private-transfer-雪.tar";
    let bytes: Vec<_> = (0..1_000_003).map(|i| (i % 256) as u8).collect();
    let receipt = FileReceipt {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(&bytes).into(),
    };
    let request = FileRequest::import(path.into(), receipt.bytes, receipt.sha256).unwrap();
    assert_eq!(
        import(runtime, &binding, &ready, &request, &bytes)
            .await
            .unwrap(),
        FileTransferResult::Completed(receipt.clone())
    );
    let (result, actual) = export(runtime, &binding, &ready, path, receipt.bytes).await;
    assert_eq!(
        result.unwrap(),
        FileTransferResult::Completed(receipt.clone())
    );
    assert_eq!(actual, bytes);
    assert_eq!(
        import(runtime, &binding, &ready, &request, &bytes)
            .await
            .unwrap(),
        FileTransferResult::Rejected(FileFailure::DestinationExists)
    );
    let (result, actual) = export(runtime, &binding, &ready, path, receipt.bytes - 1).await;
    assert_eq!(
        result.unwrap(),
        FileTransferResult::Rejected(FileFailure::TooLarge)
    );
    assert!(actual.is_empty());

    let bad_hash =
        FileRequest::import("private-checksum-rejection".into(), receipt.bytes, [0; 32]).unwrap();
    assert_eq!(
        import(runtime, &binding, &ready, &bad_hash, &bytes)
            .await
            .unwrap(),
        FileTransferResult::Rejected(FileFailure::Integrity)
    );
    let (result, actual) = export(
        runtime,
        &binding,
        &ready,
        "private-checksum-rejection",
        receipt.bytes,
    )
    .await;
    assert_eq!(
        result.unwrap(),
        FileTransferResult::Rejected(FileFailure::PathUnavailable)
    );
    assert!(actual.is_empty());

    // Transport/input failure cannot prove that a mutating operation had no effect.
    // Containment uses the same explicit Stop boundary as command execution.
    let interrupted = FileRequest::import(
        "private-interrupted-transfer".into(),
        receipt.bytes,
        receipt.sha256,
    )
    .unwrap();
    assert_eq!(
        import(runtime, &binding, &ready, &interrupted, &bytes[..1000]).await,
        Err(RuntimeFailure::ExecutionUnknown)
    );
    let stop = LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        Uuid::now_v7(),
        binding.clone(),
        &ready,
    )
    .unwrap();
    let stopping = runtime.stop(&binding, &ready).await.unwrap();
    let stopped = runtime
        .wait_for_lifecycle(&stop, &stopping, Duration::from_secs(30))
        .await
        .unwrap();
    let start = LifecycleCheckpoint::start(
        Uuid::from_u128(100),
        Uuid::now_v7(),
        binding.clone(),
        &stopped,
    )
    .unwrap();
    let starting = runtime.start(&binding, &stopped).await.unwrap();
    let restarted = runtime
        .wait_for_lifecycle(&start, &starting, Duration::from_secs(30))
        .await
        .unwrap();
    assert_ne!(
        ready.main_process_instance_id,
        restarted.main_process_instance_id
    );
    let stale = FileRequest::import(
        "private-stale-run-transfer".into(),
        receipt.bytes,
        receipt.sha256,
    )
    .unwrap();
    assert_eq!(
        import(runtime, &binding, &ready, &stale, &bytes).await,
        Err(RuntimeFailure::BindingMismatch)
    );
    for absent in ["private-interrupted-transfer", "private-stale-run-transfer"] {
        let (result, actual) = export(runtime, &binding, &restarted, absent, receipt.bytes).await;
        assert_eq!(
            result.unwrap(),
            FileTransferResult::Rejected(FileFailure::PathUnavailable)
        );
        assert!(actual.is_empty());
    }
    let (result, actual) = export(runtime, &binding, &restarted, path, receipt.bytes).await;
    assert_eq!(result.unwrap(), FileTransferResult::Completed(receipt));
    assert_eq!(actual, bytes);
    let log = std::fs::read_to_string(provider.dir.join("gateway.log")).unwrap();
    assert!(log.contains("command_preview") && log.contains("--files"));
    for value in [
        path,
        "private-checksum-rejection",
        "private-interrupted-transfer",
        "private-stale-run-transfer",
    ] {
        assert!(!log.contains(value), "file path leaked into provider log");
    }
    std::fs::write(provider.dir.join("files-result.txt"), "files/v1 native confinement; 1000003 binary bytes and SHA256; opaque archive; no overwrite; bounded export; checksum refusal; interrupted input remains uncertain; Stop/Start retains committed bytes and removes incomplete input; stale run refused; provider command log excludes paths\n").unwrap();
    home.finish();
    provider.assert_running();
}
