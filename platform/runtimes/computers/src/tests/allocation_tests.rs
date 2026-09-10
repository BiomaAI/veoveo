use super::*;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn allocator_fixture(mode: u8) -> (HomeAllocator, tokio::task::JoinHandle<()>, TlsFiles) {
    let tls = TlsFiles::new();
    let mut roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(tls.ca.as_bytes()) {
        roots.add(cert.unwrap()).unwrap();
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()
    .unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_client_cert_verifier(verifier)
    .with_single_cert(
        CertificateDer::pem_slice_iter(tls.server_cert.as_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap(),
        PrivateKeyDer::from_pem_slice(tls.server_key.as_bytes()).unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let initial = binding();
        let replacement = Binding::replacement(
            initial.computer_id(),
            Uuid::from_u128(999),
            initial.template_fingerprint().into(),
        )
        .unwrap();
        for (expected, binding) in [
            ("ready", &initial),
            ("prepare", &initial),
            ("restore", &initial),
            ("prepare", &replacement),
            ("restore", &replacement),
        ] {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = acceptor.accept(tcp).await.unwrap();
            let size = socket.read_u32().await.unwrap();
            assert!(size <= 1024);
            let mut bytes = vec![0; size as usize];
            socket.read_exact(&mut bytes).await.unwrap();
            let mut request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request["operation"], expected);
            assert_eq!(
                request["templateFingerprint"],
                binding.template_fingerprint()
            );
            assert_eq!(request["providerId"], Uuid::from_u128(100).to_string());
            if expected != "ready" {
                assert_eq!(request["computerId"], binding.computer_id().to_string());
                assert_eq!(
                    request["instanceId"],
                    binding
                        .replacement_instance_id()
                        .unwrap_or(binding.computer_id())
                        .to_string()
                );
            } else {
                assert!(request.get("computerId").is_none());
                assert!(request.get("instanceId").is_none());
            }
            request["status"] = "ready".into();
            request["capacityBytes"] = (1024u64 * 1024 * 1024).into();
            match mode {
                1 => request["capacityBytes"] = 1.into(),
                2 => request["extra"] = "untrusted".into(),
                3 => request["capacityBytes"] = true.into(),
                4 => request["computerId"] = serde_json::Value::Null,
                5 => {
                    socket.write_u32(1025).await.unwrap();
                    return;
                }
                6 => return,
                8 => request["providerId"] = Uuid::from_u128(101).to_string().into(),
                9 if expected != "ready" => {
                    request["instanceId"] = Uuid::from_u128(999).to_string().into()
                }
                _ => {}
            }
            let mut raw = serde_json::to_vec(&request).unwrap();
            if mode == 7 {
                raw.pop();
                raw.extend(b",\"status\":\"ready\"}");
            }
            socket.write_u32(raw.len() as u32).await.unwrap();
            socket.write_all(&raw).await.unwrap();
            socket.flush().await.unwrap();
            if mode != 0 && !(mode == 9 && expected == "ready") {
                return;
            }
        }
    });
    let config = AllocationConfig::new(
        endpoint,
        tls.dir.join("ca.pem"),
        tls.dir.join("cert.pem"),
        tls.dir.join("key.pem"),
    )
    .unwrap();
    (
        HomeAllocator::new(
            config,
            Uuid::from_u128(100),
            binding().template_fingerprint().into(),
            1024 * 1024 * 1024,
        )
        .await
        .unwrap(),
        task,
        tls,
    )
}
#[tokio::test]
async fn exact_tls13_allocation_ready_prepare_restore() {
    let (allocator, task, _tls) = allocator_fixture(0).await;
    allocator.ready().await.unwrap();
    allocator.prepare(&binding()).await.unwrap();
    allocator.restore(&binding()).await.unwrap();
    let replacement = Binding::replacement(
        binding().computer_id(),
        Uuid::from_u128(999),
        binding().template_fingerprint().into(),
    )
    .unwrap();
    allocator.prepare(&replacement).await.unwrap();
    allocator.restore(&replacement).await.unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn allocation_invalid_unknown_and_duplicate_results_fail_closed() {
    for mode in 1..=8 {
        let (allocator, task, _tls) = allocator_fixture(mode).await;
        assert!(matches!(
            allocator.ready().await,
            Err(RuntimeFailure::AllocationFailed)
        ));
        task.await.unwrap();
    }
    let (allocator, task, _tls) = allocator_fixture(9).await;
    allocator.ready().await.unwrap();
    assert!(matches!(
        allocator.prepare(&binding()).await,
        Err(RuntimeFailure::AllocationFailed)
    ));
    task.await.unwrap();
}

mod generated_contract {
    use crate::allocation::{validate_reply, wire};
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::{Value, json};

    const CAPACITY: u64 = 1024 * 1024 * 1024;

    fn request(operation: &str) -> Value {
        let mut value = json!({
            "schema": "veoveo.io/computer-storage/v1",
            "operation": operation,
            "templateFingerprint": "a".repeat(64),
            "providerId": "00000000-0000-0000-0000-000000000100",
        });
        if operation != "ready" {
            value["computerId"] = "3c4c49ee-4f42-442d-a761-71a15466b992".into();
            value["instanceId"] = "3c4c49ee-4f42-442d-a761-71a15466b992".into();
        }
        value
    }

    fn reply(operation: &str) -> Value {
        let mut value = request(operation);
        value["status"] = "ready".into();
        value["capacityBytes"] = CAPACITY.into();
        value
    }

    fn failure() -> Value {
        json!({"schema": "veoveo.io/computer-storage/v1", "status": "error"})
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn round_trip<T: DeserializeOwned + Serialize>(value: &Value) {
        let typed: T = serde_json::from_slice(&bytes(value)).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), *value);
    }

    fn duplicate_fields<T: DeserializeOwned>(value: &Value) {
        let raw = serde_json::to_string(value).unwrap();
        for (key, item) in value.as_object().unwrap() {
            let escaped = format!("\"\\u{:04x}{}\"", key.as_bytes()[0], &key[1..]);
            for name in [serde_json::to_string(key).unwrap(), escaped] {
                let duplicate = format!("{},{}:{}}}", &raw[..raw.len() - 1], name, item);
                assert!(
                    serde_json::from_slice::<T>(duplicate.as_bytes()).is_err(),
                    "duplicate {key} must fail before any JSON Value normalization"
                );
            }
        }
    }

    fn closed_fields<T: DeserializeOwned>(value: &Value) {
        let mut extra = value.clone();
        extra["extra"] = "untrusted".into();
        assert!(serde_json::from_slice::<T>(&bytes(&extra)).is_err());
        for key in value.as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_slice::<T>(&bytes(&missing)).is_err(),
                "missing {key}"
            );
            let mut null = value.clone();
            null[key] = Value::Null;
            assert!(
                serde_json::from_slice::<T>(&bytes(&null)).is_err(),
                "null {key}"
            );
        }
    }

    #[test]
    fn generated_storage_shapes_round_trip_all_operations() {
        round_trip::<wire::ReadyRequest>(&request("ready"));
        round_trip::<wire::ReadyReply>(&reply("ready"));
        round_trip::<wire::FailureReply>(&failure());
        for operation in ["prepare", "restore"] {
            round_trip::<wire::BoundRequest>(&request(operation));
            round_trip::<wire::BoundReply>(&reply(operation));
        }
        for value in [
            request("ready"),
            request("prepare"),
            request("restore"),
            reply("ready"),
            reply("prepare"),
            reply("restore"),
            failure(),
        ] {
            round_trip::<wire::ComputerStorageProtocol>(&value);
        }
    }

    #[test]
    fn generated_storage_deserializers_reject_duplicate_and_escaped_keys() {
        duplicate_fields::<wire::ReadyRequest>(&request("ready"));
        duplicate_fields::<wire::BoundRequest>(&request("prepare"));
        duplicate_fields::<wire::ReadyReply>(&reply("ready"));
        duplicate_fields::<wire::BoundReply>(&reply("restore"));
        duplicate_fields::<wire::FailureReply>(&failure());
        for value in [
            request("ready"),
            request("prepare"),
            reply("ready"),
            reply("restore"),
            failure(),
        ] {
            duplicate_fields::<wire::ComputerStorageProtocol>(&value);
        }
    }

    #[test]
    fn generated_storage_shapes_deny_unknown_missing_and_null_fields() {
        closed_fields::<wire::ReadyRequest>(&request("ready"));
        closed_fields::<wire::BoundRequest>(&request("prepare"));
        closed_fields::<wire::ReadyReply>(&reply("ready"));
        closed_fields::<wire::BoundReply>(&reply("restore"));
        closed_fields::<wire::FailureReply>(&failure());
        for id in [Value::Null, json!("3c4c49ee-4f42-442d-a761-71a15466b992")] {
            let mut ready = request("ready");
            ready["computerId"] = id.clone();
            assert!(serde_json::from_slice::<wire::ReadyRequest>(&bytes(&ready)).is_err());
            let mut ready = reply("ready");
            ready["computerId"] = id;
            assert!(serde_json::from_slice::<wire::ReadyReply>(&bytes(&ready)).is_err());
        }
    }

    #[test]
    fn generated_storage_identity_and_operation_constraints_are_closed() {
        for identity in [
            "00000000-0000-0000-0000-000000000000",
            "3c4c49ee4f42442da76171a15466b992",
            "3C4C49EE-4F42-442D-A761-71A15466B992",
            "3c4c49ee-4f42-442d-a761-71a15466b992\n",
            "../other",
        ] {
            for field in ["computerId", "providerId", "instanceId"] {
                let mut value = request("prepare");
                value[field] = identity.into();
                assert!(serde_json::from_slice::<wire::BoundRequest>(&bytes(&value)).is_err());
            }
        }
        for fingerprint in [
            "A".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
            format!("{}\n", "a".repeat(64)),
        ] {
            let mut value = request("prepare");
            value["templateFingerprint"] = fingerprint.into();
            assert!(serde_json::from_slice::<wire::BoundRequest>(&bytes(&value)).is_err());
        }
        for operation in ["ready", "delete", "stop", "PREPARE", ""] {
            let mut value = request("prepare");
            value["operation"] = operation.into();
            assert!(serde_json::from_slice::<wire::BoundRequest>(&bytes(&value)).is_err());
        }
    }

    #[test]
    fn generated_reply_decoder_requires_integer_capacity_and_exact_echo() {
        let value = reply("restore");
        let expected: wire::BoundReply = serde_json::from_slice(&bytes(&value)).unwrap();
        validate_reply(&bytes(&value), &expected).unwrap();
        let raw = serde_json::to_string(&value).unwrap();
        for capacity in [
            "true",
            "1073741824.0",
            "1.073741824e9",
            "-1",
            "18446744073709551616",
            "1073741825",
            "536870912",
            "0",
        ] {
            let modified = raw.replace(
                &format!("\"capacityBytes\":{CAPACITY}"),
                &format!("\"capacityBytes\":{capacity}"),
            );
            assert!(validate_reply(modified.as_bytes(), &expected).is_err());
        }
        for (field, replacement) in [
            ("operation", "prepare"),
            ("computerId", "00000000-0000-0000-0000-000000000001"),
            ("providerId", "00000000-0000-0000-0000-000000000001"),
            ("instanceId", "00000000-0000-0000-0000-000000000001"),
            (
                "templateFingerprint",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            ("schema", "other"),
            ("status", "error"),
        ] {
            let mut modified = value.clone();
            modified[field] = replacement.into();
            assert!(validate_reply(&bytes(&modified), &expected).is_err());
        }
    }

    #[test]
    fn generated_reply_decoder_rejects_nonobjects_trailing_bytes_and_failures() {
        let value = reply("ready");
        let expected: wire::ReadyReply = serde_json::from_slice(&bytes(&value)).unwrap();
        validate_reply(&bytes(&value), &expected).unwrap();
        // Serde's struct-sequence representation is not part of this JSON protocol.
        let array: Vec<Value> = value.as_object().unwrap().values().cloned().collect();
        let mut trailing = bytes(&value);
        trailing.extend_from_slice(b"{}");
        let mut oversized = bytes(&value);
        oversized.resize(1025, b' ');
        for raw in [
            Vec::new(),
            b"null".to_vec(),
            b"[]".to_vec(),
            serde_json::to_vec(&array).unwrap(),
            trailing,
            oversized,
            bytes(&failure()),
        ] {
            assert!(validate_reply(&raw, &expected).is_err());
        }
    }
}
