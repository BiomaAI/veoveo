//! Checks run inside the owned host fixture after its retained-image upgrade.
use crate::fixture::images;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{fs, path::Path};

const ROOT: &str = "/sys/fs/cgroup";
const SOCKET: &str = "unix:///run/veoveo-computers/docker.sock";
const NAMESPACE: &str = "host-qualification";
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Role {
    #[vocabulary(rename = "sandbox")]
    Sandbox,
    #[vocabulary(rename = "supervisor")]
    Supervisor,
}
#[derive(Deserialize)]
struct Labels {
    #[serde(rename = "openshell.ai/sandbox-namespace")]
    namespace: String,
    #[serde(rename = "openshell.ai/sandbox-id")]
    sandbox_id: String,
    #[serde(rename = "openshell.ai/sandbox-name")]
    name: String,
    #[serde(rename = "openshell.ai/isolation-role")]
    role: Role,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inspection {
    id: String,
    image: veoveo_types::Sha256Digest,
    state: State,
    config: Config,
    host_config: HostConfig,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    pid: u32,
    running: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Config {
    labels: Labels,
    user: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HostConfig {
    nano_cpus: i64,
    memory: i64,
    memory_reservation: i64,
    cpu_shares: i64,
    cpuset_cpus: String,
    cpuset_mems: String,
    pids_limit: i64,
    network_mode: String,
    pid_mode: String,
    ipc_mode: String,
    cgroupns_mode: String,
    readonly_rootfs: bool,
    cap_drop: Option<Vec<String>>,
    cap_add: Option<Vec<String>>,
    security_opt: Option<Vec<String>>,
}
fn docker(args: &[&str]) -> Result<String> {
    // The surrounding owned fault also has a 55-second deadline.
    let mut bounded = vec!["10", "docker", "-H", SOCKET];
    bounded.extend_from_slice(args);
    super::command("timeout", &bounded)
}
fn inspect(id: &str) -> Result<Inspection> {
    ensure!(
        id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "exact Docker container ID required"
    );
    let value: Inspection =
        serde_json::from_str(&docker(&["inspect", "--format", "{{json .}}", id])?)?;
    ensure!(
        value.id == id && value.state.running && value.state.pid > 0,
        "owned container identity or process changed"
    );
    Ok(value)
}
fn admit_child(value: &Inspection, role: Role) -> Result<()> {
    ensure!(
        value.config.labels.namespace == NAMESPACE && value.config.labels.role == role,
        "container belongs to another namespace or role"
    );
    let host = &value.host_config;
    ensure!(
        host.nano_cpus == 2_000_000_000 && host.memory == 2_147_483_648 && host.pids_limit == 256,
        "child CPU/memory/PID ceiling differs from the admitted template"
    );
    ensure!(
        host.memory_reservation == 0
            && host.cpu_shares == 0
            && host.cpuset_cpus.is_empty()
            && host.cpuset_mems.is_empty(),
        "child reserves memory or CPU placement"
    );
    ensure!(
        host.pid_mode.is_empty() && host.ipc_mode == "private" && host.cgroupns_mode == "private",
        "child must have private PID/IPC/cgroup namespaces"
    );
    match role {
        Role::Sandbox => ensure!(
            host.network_mode == "none",
            "workload gained a network attachment"
        ),
        Role::Supervisor => {
            ensure!(
                host.network_mode == "host"
                    && host.readonly_rootfs
                    && value.config.user == "65534:65534"
                    && host
                        .cap_drop
                        .as_ref()
                        .is_some_and(|values| values.iter().any(|value| value == "ALL"))
                    && host.cap_add.as_ref().is_none_or(Vec::is_empty)
                    && host.security_opt.as_ref().is_some_and(|values| values
                        .iter()
                        .any(|value| value == "no-new-privileges:true")),
                "supervisor privilege or private-host network profile changed"
            );
        }
    }
    Ok(())
}
fn admit_pair(workload: &Inspection, supervisor: &Inspection) -> Result<()> {
    admit_child(workload, Role::Sandbox)?;
    admit_child(supervisor, Role::Supervisor)?;
    let labels = &workload.config.labels;
    ensure!(
        !labels.sandbox_id.is_empty()
            && !labels.name.is_empty()
            && workload.id != supervisor.id
            && supervisor.config.labels.sandbox_id == labels.sandbox_id
            && supervisor.config.labels.name == labels.name,
        "supervisor belongs to another retained Computer or repeats its workload"
    );
    Ok(())
}
fn admit_companion_image(
    supervisor: &Inspection,
    image: &images::Inspection,
    reference: &str,
    authority: &str,
) -> Result<veoveo_types::Sha256Digest> {
    let id = image.admit(reference, authority)?;
    ensure!(
        supervisor.image == id,
        "supervisor container uses another local image identity"
    );
    Ok(id)
}
fn child_cgroup(value: &Inspection) -> Result<()> {
    let relative = format!("/docker/{}", value.id);
    ensure!(
        fs::read_to_string(format!("/proc/{}/cgroup", value.state.pid))?.trim()
            == format!("0::{relative}"),
        "child escaped the compute host's cgroup root"
    );
    for (name, expected) in [
        ("cpu.max", "200000 100000"),
        ("memory.max", "2147483648"),
        ("pids.max", "256"),
        ("memory.min", "0"),
        ("memory.low", "0"),
    ] {
        ensure!(
            fs::read_to_string(format!("{ROOT}{relative}/{name}"))?.trim() == expected,
            "child cgroup {name} differs from its ceiling or zero reservation"
        );
    }
    for namespace in ["mnt", "pid", "ipc", "cgroup"] {
        ensure!(
            fs::read_link(format!("/proc/{}/ns/{namespace}", value.state.pid))?
                != fs::read_link(format!("/proc/1/ns/{namespace}"))?,
            "child shares the launcher {namespace} namespace"
        );
    }
    Ok(())
}
pub fn check(directory: &Path, after: bool) -> Result<()> {
    // OCI exec may join the original cgroup namespace instead of PID 1's scope.
    nix::sched::setns(
        fs::File::open("/proc/1/ns/cgroup")?,
        nix::sched::CloneFlags::CLONE_NEWCGROUP,
    )?;
    ensure!(fs::read_to_string("/proc/1/cgroup")?.trim() == "0::/init");
    for (name, expected) in [
        ("cpu.max", "100000 100000"),
        ("memory.max", "6442450944"),
        ("pids.max", "1024"),
        ("memory.min", "0"),
        ("memory.low", "0"),
    ] {
        ensure!(
            fs::read_to_string(format!("{ROOT}/{name}"))?.trim() == expected,
            "aggregate host cgroup {name} changed"
        );
    }
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("record.json"))?)?;
    let id = record["writer"]["writer"]["containerId"]
        .as_str()
        .context("retained writer")?;
    let workload = inspect(id)?;
    admit_child(&workload, Role::Sandbox)?;
    let labels = &workload.config.labels;
    let namespace = format!("label=openshell.ai/sandbox-namespace={}", labels.namespace);
    let sandbox_id = format!("label=openshell.ai/sandbox-id={}", labels.sandbox_id);
    let name = format!("label=openshell.ai/sandbox-name={}", labels.name);
    let output = docker(&[
        "ps",
        "--no-trunc",
        "--filter",
        &namespace,
        "--filter",
        &sandbox_id,
        "--filter",
        &name,
        "--filter",
        "label=openshell.ai/isolation-role=supervisor",
        "--format",
        "{{.ID}}",
    ])?;
    let ids: Vec<_> = output.lines().collect();
    ensure!(
        ids.len() == 1 && ids[0] != id,
        "exactly one separate supervisor required for this retained workload"
    );
    let supervisor = inspect(ids[0])?;
    admit_pair(&workload, &supervisor)?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HostImages {
        supervisor_image: String,
        registry: Registry,
    }
    #[derive(Deserialize)]
    struct Registry {
        authority: String,
    }
    let configured: HostImages =
        serde_json::from_slice(&fs::read("/etc/veoveo/computers/host-config/host.json")?)?;
    let image = images::decode(
        docker(&[
            "image",
            "inspect",
            &configured.supervisor_image,
            "--format",
            "{{json .}}",
        ])?
        .as_bytes(),
    )?;
    let supervisor_image_id = admit_companion_image(
        &supervisor,
        &image,
        &configured.supervisor_image,
        &configured.registry.authority,
    )?;
    child_cgroup(&workload)?;
    child_cgroup(&supervisor)?;
    for namespace in ["mnt", "pid", "ipc", "cgroup"] {
        ensure!(
            fs::read_link(format!("/proc/{}/ns/{namespace}", workload.state.pid))?
                != fs::read_link(format!("/proc/{}/ns/{namespace}", supervisor.state.pid))?,
            "workload and supervisor share {namespace} namespace"
        );
    }
    let host_network = fs::read_link("/proc/1/ns/net")?;
    ensure!(
        fs::read_link(format!("/proc/{}/ns/net", supervisor.state.pid))? == host_network,
        "supervisor escaped the private compute-host network"
    );
    ensure!(
        fs::read_link(format!("/proc/{}/ns/net", workload.state.pid))? != host_network,
        "workload joined the supervisor network"
    );
    let stat = fs::read_to_string(format!("{ROOT}/cpu.stat"))?;
    let throttled: u64 = stat
        .lines()
        .find_map(|line| line.strip_prefix("nr_throttled "))
        .context("aggregate throttling counter")?
        .parse()?;
    let baseline = directory.join("limits-before");
    if after {
        let before: u64 = fs::read_to_string(baseline)?.parse()?;
        ensure!(
            throttled > before,
            "guest workload did not hit the stricter host CPU ceiling"
        );
    } else {
        fs::write(baseline, throttled.to_string())?;
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Evidence {
        workload_container_id: String,
        supervisor_container_id: String,
        supervisor_image: String,
        supervisor_image_id: veoveo_types::Sha256Digest,
    }
    let evidence = Evidence {
        workload_container_id: workload.id,
        supervisor_container_id: supervisor.id,
        supervisor_image: configured.supervisor_image,
        supervisor_image_id,
    };
    fs::write(
        directory.join("limits-containers.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn child(role: Role) -> Inspection {
        Inspection {
            id: match role {
                Role::Sandbox => "a",
                Role::Supervisor => "b",
            }
            .repeat(64),
            image: veoveo_types::Sha256Digest::from_bytes([4; 32]),
            state: State {
                pid: 1,
                running: true,
            },
            config: Config {
                labels: Labels {
                    namespace: NAMESPACE.into(),
                    sandbox_id: "owned-instance".into(),
                    name: "owned-computer".into(),
                    role,
                },
                user: "65534:65534".into(),
            },
            host_config: HostConfig {
                nano_cpus: 2_000_000_000,
                memory: 2_147_483_648,
                memory_reservation: 0,
                cpu_shares: 0,
                cpuset_cpus: String::new(),
                cpuset_mems: String::new(),
                pids_limit: 256,
                network_mode: match role {
                    Role::Sandbox => "none",
                    Role::Supervisor => "host",
                }
                .into(),
                pid_mode: String::new(),
                ipc_mode: "private".into(),
                cgroupns_mode: "private".into(),
                readonly_rootfs: true,
                cap_drop: Some(vec!["ALL".into()]),
                cap_add: None,
                security_opt: Some(vec!["no-new-privileges:true".into()]),
            },
        }
    }
    #[test]
    fn companion_uses_local_image_id_not_the_registry_manifest_digest() {
        let (reference, value) = images::tests::source_image();
        let image = images::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        let mut supervisor = child(Role::Supervisor);
        supervisor.image = image.id.clone();
        admit_companion_image(&supervisor, &image, &reference, "registry.internal").unwrap();
        assert_ne!(
            supervisor.image.hex(),
            reference.split_once("@sha256:").unwrap().1
        );
        supervisor.image =
            veoveo_types::Sha256Digest::from_hex(reference.split_once("@sha256:").unwrap().1)
                .unwrap();
        assert!(
            admit_companion_image(&supervisor, &image, &reference, "registry.internal").is_err()
        );
    }
    #[test]
    fn companion_selection_rejects_another_instance_namespace_or_workload_identity() {
        let workload = child(Role::Sandbox);
        admit_pair(&workload, &child(Role::Supervisor)).unwrap();
        let mut supervisor = child(Role::Supervisor);
        supervisor.config.labels.sandbox_id = "another-instance".into();
        assert!(admit_pair(&workload, &supervisor).is_err());
        supervisor = child(Role::Supervisor);
        supervisor.config.labels.name = "another-name".into();
        assert!(admit_pair(&workload, &supervisor).is_err());
        supervisor = child(Role::Supervisor);
        supervisor.config.labels.namespace = "another-fixture".into();
        assert!(admit_pair(&workload, &supervisor).is_err());
        supervisor = child(Role::Supervisor);
        supervisor.id = workload.id.clone();
        assert!(admit_pair(&workload, &supervisor).is_err());
    }
    #[test]
    fn companion_resource_admission_rejects_unbounded_reserved_or_privileged_children() {
        for role in [Role::Sandbox, Role::Supervisor] {
            let mut value = child(role);
            value.host_config.memory = 0;
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.nano_cpus = 0;
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.pids_limit = -1;
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.memory_reservation = 1;
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.cpuset_cpus = "0".into();
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.pid_mode = "host".into();
            assert!(admit_child(&value, role).is_err());
            value = child(role);
            value.host_config.network_mode = "bridge".into();
            assert!(admit_child(&value, role).is_err());
        }
        let mut value = child(Role::Supervisor);
        value.host_config.cap_add = Some(vec!["SYS_ADMIN".into()]);
        assert!(admit_child(&value, Role::Supervisor).is_err());
    }
}
