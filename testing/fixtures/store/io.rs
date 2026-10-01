//! Linux cgroup-v2 write accounting for an owned local Docker fixture.
#![allow(dead_code, reason = "Most Store fixtures do not measure block I/O")]

use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct BlockDevice {
    pub major: u32,
    pub minor: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DeviceWrites {
    pub device: BlockDevice,
    pub bytes: u64,
    pub operations: u64,
}

pub struct IoProbe {
    path: PathBuf,
}

impl IoProbe {
    pub(super) fn for_process(pid: u32) -> Self {
        let membership = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
            .expect("measurement requires readable local cgroup-v2 membership");
        let group = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::/"))
            .expect("measurement requires a unified cgroup-v2 hierarchy");
        assert!(
            !group.is_empty()
                && Path::new(group)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "invalid cgroup path"
        );
        let path = Path::new("/sys/fs/cgroup").join(group).join("io.stat");
        assert!(
            path.is_file(),
            "measurement requires cgroup-v2 I/O accounting"
        );
        Self { path }
    }

    fn read(&self) -> BTreeMap<BlockDevice, DeviceWrites> {
        let text =
            std::fs::read_to_string(&self.path).expect("owned cgroup I/O counters disappeared");
        text.lines()
            .map(|line| {
                let mut fields = line.split_whitespace();
                let (major, minor) = fields.next().unwrap().split_once(':').unwrap();
                let device = BlockDevice {
                    major: major.parse().unwrap(),
                    minor: minor.parse().unwrap(),
                };
                let counters = fields
                    .filter_map(|field| field.split_once('='))
                    .collect::<BTreeMap<_, _>>();
                let writes = DeviceWrites {
                    device,
                    bytes: counters["wbytes"].parse().unwrap(),
                    operations: counters["wios"].parse().unwrap(),
                };
                (device, writes)
            })
            .collect()
    }

    /// Flush kernel writeback outside the timed query window, then require two
    /// seconds of stable counters. This does not flush RocksDB's memtables or
    /// account for compaction after this observation window.
    pub async fn settled(&self) -> BTreeMap<BlockDevice, DeviceWrites> {
        tokio::time::timeout(Duration::from_secs(30), async {
            let status = Command::new("sync")
                .kill_on_drop(true)
                .status()
                .await
                .expect("measurement requires the host sync utility");
            assert!(status.success(), "measurement writeback flush failed");
            let mut previous = self.read();
            let mut stable = tokio::time::Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let current = self.read();
                if current == previous {
                    if stable.elapsed() >= Duration::from_secs(2) {
                        return current;
                    }
                } else {
                    stable = tokio::time::Instant::now();
                    previous = current;
                }
            }
        })
        .await
        .expect("measurement I/O did not settle within 30 seconds")
    }
}

pub fn delta(
    before: &BTreeMap<BlockDevice, DeviceWrites>,
    after: &BTreeMap<BlockDevice, DeviceWrites>,
) -> Vec<DeviceWrites> {
    assert!(
        before.keys().all(|device| after.contains_key(device)),
        "measurement device disappeared"
    );
    after
        .iter()
        .map(|(device, writes)| {
            let previous = before.get(device).copied().unwrap_or(DeviceWrites {
                device: *device,
                bytes: 0,
                operations: 0,
            });
            DeviceWrites {
                device: *device,
                bytes: writes
                    .bytes
                    .checked_sub(previous.bytes)
                    .expect("write counter went backwards"),
                operations: writes
                    .operations
                    .checked_sub(previous.operations)
                    .expect("I/O counter went backwards"),
            }
        })
        .collect()
}
