//! Installation-only native fixture grant. No public policy-administration API.
use std::time::Duration;
use tonic::{
    Request,
    transport::{Certificate, ClientTlsConfig, Endpoint, Identity},
};
use veoveo_computers_runtime::{
    Binding, Phase,
    protocol::{sandbox::v1 as policy, v1 as api},
};

pub async fn add_grant(provider: &crate::provider::Provider, binding: &Binding) {
    tokio::time::timeout(Duration::from_secs(30), async {
        let before = provider.runtime.get(binding).await.unwrap().unwrap();
        assert!(before.phase == Phase::Ready);
        let dir = &provider.dir;
        let tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(Certificate::from_pem(
                std::fs::read(dir.join("ca.pem")).unwrap(),
            ))
            .identity(Identity::from_pem(
                std::fs::read(dir.join("client.pem")).unwrap(),
                std::fs::read(dir.join("client-key.pem")).unwrap(),
            ));
        let channel = Endpoint::from_shared(format!("https://{}", provider.endpoint))
            .unwrap()
            .tls_config(tls)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = api::open_shell_client::OpenShellClient::new(channel)
            .max_decoding_message_size(1024 * 1024)
            .max_encoding_message_size(1024 * 1024);
        let source = client
            .get_sandbox(api::GetSandboxRequest {
                name: binding.name(),
                workspace: "default".into(),
            })
            .await
            .unwrap()
            .into_inner()
            .sandbox
            .unwrap();
        let metadata = source.metadata.unwrap();
        assert_eq!(metadata.id, before.sandbox_id);
        let mut watch = client
            .watch_sandbox(api::WatchSandboxRequest {
                id: before.sandbox_id.clone(),
                follow_status: true,
                stop_on_terminal: false,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(watch.message().await.unwrap().is_some());
        let name = "retained-policy-fixture";
        let mut request = Request::new(api::UpdateConfigRequest {
            name: binding.name(),
            workspace: "default".into(),
            expected_resource_version: metadata.resource_version,
            merge_operations: vec![api::PolicyMergeOperation {
                operation: Some(api::policy_merge_operation::Operation::AddRule(
                    api::AddNetworkRule {
                        rule_name: name.into(),
                        rule: Some(policy::NetworkPolicyRule {
                            name: name.into(),
                            endpoints: vec![policy::NetworkEndpoint {
                                host: "retained-policy.example.com".into(),
                                port: 443,
                                ports: vec![443],
                                protocol: "rest".into(),
                                tls: "terminate".into(),
                                enforcement: "enforce".into(),
                                access: "read-only".into(),
                                ..Default::default()
                            }],
                            binaries: vec![policy::NetworkBinary {
                                path: "/usr/bin/curl".into(),
                                ..Default::default()
                            }],
                        }),
                    },
                )),
            }],
            ..Default::default()
        });
        request.set_timeout(Duration::from_secs(15));
        let updated = client.update_config(request).await.unwrap().into_inner();
        println!(
            "Native maintenance: policy update accepted at version {}",
            updated.version
        );
        for _ in 0..128 {
            let event = watch.message().await.unwrap().expect("policy watch ended");
            if let Some(api::sandbox_stream_event::Payload::Sandbox(sandbox)) = event.payload {
                assert_eq!(sandbox.metadata.as_ref().unwrap().id, before.sandbox_id);
                let status = sandbox.status.unwrap();
                assert_eq!(
                    status.main_process_instance_id,
                    before.main_process_instance_id
                );
                if status.current_policy_version == updated.version {
                    println!("Native maintenance: policy version loaded");
                    return;
                }
            }
        }
        panic!("policy watch exhausted its finite event budget");
    })
    .await
    .expect("native policy grant exceeded its deadline");
}
