use std::{
    fs::{File, OpenOptions},
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use memmap2::{Mmap, MmapMut, MmapOptions};

use crate::PoseError;

const MAGIC: &[u8; 8] = b"VVPSHM02";
const VERSION: u16 = 2;
const HEADER_BYTES: usize = 64;
const LATEST_GENERATION_OFFSET: usize = 16;
const SLOT_CAPACITY_OFFSET: usize = 24;
const SLOT_COUNT_OFFSET: usize = 32;
const SLOT_STRIDE_OFFSET: usize = 40;
const SLOT_HEADER_BYTES: usize = 16;
const SLOT_GENERATION_OFFSET: usize = 0;
const SLOT_LENGTH_OFFSET: usize = 8;

pub const MAXIMUM_SHARED_POSE_SLOTS: usize = 4096;

pub struct SharedPoseWriter {
    map: MmapMut,
    slot_capacity: usize,
    slot_count: usize,
    slot_stride: usize,
}

impl SharedPoseWriter {
    pub fn create(path: &Path, slot_capacity: usize, slot_count: usize) -> Result<Self, PoseError> {
        Self::open(path, slot_capacity, slot_count)
    }

    pub fn replace(
        path: &Path,
        slot_capacity: usize,
        slot_count: usize,
    ) -> Result<Self, PoseError> {
        let temporary = path.with_extension("pose.next");
        match std::fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let writer = Self::create(&temporary, slot_capacity, slot_count)?;
        if let Err(error) = std::fs::rename(&temporary, path) {
            drop(writer);
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(writer)
    }

    fn open(path: &Path, slot_capacity: usize, slot_count: usize) -> Result<Self, PoseError> {
        if slot_capacity == 0 || !(2..=MAXIMUM_SHARED_POSE_SLOTS).contains(&slot_count) {
            return Err(PoseError::SharedSlot(
                "shared pose history dimensions are invalid".to_owned(),
            ));
        }
        let unaligned_stride = SLOT_HEADER_BYTES
            .checked_add(slot_capacity)
            .ok_or_else(|| PoseError::SharedSlot("shared pose slot size overflow".to_owned()))?;
        let slot_stride = unaligned_stride
            .checked_add(7)
            .map(|value| value & !7)
            .ok_or_else(|| PoseError::SharedSlot("shared pose slot size overflow".to_owned()))?;
        let total_bytes = HEADER_BYTES
            .checked_add(slot_stride.checked_mul(slot_count).ok_or_else(|| {
                PoseError::SharedSlot("shared pose history size overflow".to_owned())
            })?)
            .ok_or_else(|| PoseError::SharedSlot("shared pose history size overflow".to_owned()))?;
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.set_len(total_bytes as u64)?;
        let mut map = map_mut(&file, total_bytes)?;
        map[..HEADER_BYTES].fill(0);
        map[..8].copy_from_slice(MAGIC);
        map[8..10].copy_from_slice(&VERSION.to_ne_bytes());
        map[SLOT_CAPACITY_OFFSET..SLOT_CAPACITY_OFFSET + 8]
            .copy_from_slice(&(slot_capacity as u64).to_ne_bytes());
        map[SLOT_COUNT_OFFSET..SLOT_COUNT_OFFSET + 8]
            .copy_from_slice(&(slot_count as u64).to_ne_bytes());
        map[SLOT_STRIDE_OFFSET..SLOT_STRIDE_OFFSET + 8]
            .copy_from_slice(&(slot_stride as u64).to_ne_bytes());
        map.flush()?;
        Ok(Self {
            map,
            slot_capacity,
            slot_count,
            slot_stride,
        })
    }

    pub fn publish(&mut self, encoded_snapshot: &[u8]) -> Result<u64, PoseError> {
        if encoded_snapshot.len() > self.slot_capacity {
            return Err(PoseError::MessageBytes {
                actual: encoded_snapshot.len(),
                maximum: self.slot_capacity,
            });
        }
        let latest = self
            .atomic(LATEST_GENERATION_OFFSET)
            .load(Ordering::Acquire);
        let generation = latest
            .checked_add(1)
            .ok_or_else(|| PoseError::SharedSlot("shared pose generation overflow".to_owned()))?;
        let stable_marker = generation
            .checked_mul(2)
            .ok_or_else(|| PoseError::SharedSlot("shared pose generation overflow".to_owned()))?;
        let writing_marker = stable_marker - 1;
        let slot = usize::try_from((generation - 1) % self.slot_count as u64)
            .expect("shared pose slot index fits usize");
        let start = HEADER_BYTES + slot * self.slot_stride;
        self.atomic(start + SLOT_GENERATION_OFFSET)
            .store(writing_marker, Ordering::Release);
        self.map[start + SLOT_HEADER_BYTES..start + SLOT_HEADER_BYTES + encoded_snapshot.len()]
            .copy_from_slice(encoded_snapshot);
        self.atomic(start + SLOT_LENGTH_OFFSET)
            .store(encoded_snapshot.len() as u64, Ordering::Release);
        self.atomic(start + SLOT_GENERATION_OFFSET)
            .store(stable_marker, Ordering::Release);
        self.atomic(LATEST_GENERATION_OFFSET)
            .store(generation, Ordering::Release);
        Ok(generation)
    }

    fn atomic(&self, offset: usize) -> &AtomicU64 {
        // The map and each slot header are aligned to eight bytes. The writer
        // owns initialization before another process opens the file.
        unsafe { &*(self.map.as_ptr().add(offset).cast::<AtomicU64>()) }
    }
}

pub struct SharedPoseReader {
    map: Mmap,
    slot_capacity: usize,
    slot_count: usize,
    slot_stride: usize,
}

impl SharedPoseReader {
    pub fn open(path: &Path) -> Result<Self, PoseError> {
        let file = File::open(path)?;
        let length = usize::try_from(file.metadata()?.len())
            .map_err(|_| PoseError::SharedSlot("shared pose file is too large".to_owned()))?;
        if length < HEADER_BYTES || &map(&file, HEADER_BYTES)?[..8] != MAGIC {
            return Err(PoseError::SharedSlot(
                "shared pose file has invalid identity".to_owned(),
            ));
        }
        let map = map(&file, length)?;
        let version = u16::from_ne_bytes(map[8..10].try_into().expect("length"));
        let slot_capacity = declared_usize(&map, SLOT_CAPACITY_OFFSET)?;
        let slot_count = declared_usize(&map, SLOT_COUNT_OFFSET)?;
        let slot_stride = declared_usize(&map, SLOT_STRIDE_OFFSET)?;
        let expected_stride = SLOT_HEADER_BYTES
            .checked_add(slot_capacity)
            .and_then(|value| value.checked_add(7))
            .map(|value| value & !7)
            .ok_or_else(|| PoseError::SharedSlot("shared pose slot size overflow".to_owned()))?;
        let expected_length = HEADER_BYTES
            .checked_add(slot_stride.checked_mul(slot_count).ok_or_else(|| {
                PoseError::SharedSlot("shared pose history size overflow".to_owned())
            })?)
            .ok_or_else(|| PoseError::SharedSlot("shared pose history size overflow".to_owned()))?;
        if version != VERSION
            || slot_capacity == 0
            || !(2..=MAXIMUM_SHARED_POSE_SLOTS).contains(&slot_count)
            || slot_stride != expected_stride
            || length != expected_length
        {
            return Err(PoseError::SharedSlot(
                "shared pose history declaration is invalid".to_owned(),
            ));
        }
        Ok(Self {
            map,
            slot_capacity,
            slot_count,
            slot_stride,
        })
    }

    pub fn latest(&self) -> Result<Option<(u64, Vec<u8>)>, PoseError> {
        let latest = self
            .atomic(LATEST_GENERATION_OFFSET)
            .load(Ordering::Acquire);
        if latest == 0 {
            return Ok(None);
        }
        Ok(self.snapshots_after(latest - 1)?.pop())
    }

    pub fn snapshots_after(&self, generation: u64) -> Result<Vec<(u64, Vec<u8>)>, PoseError> {
        for _ in 0..3 {
            let latest = self
                .atomic(LATEST_GENERATION_OFFSET)
                .load(Ordering::Acquire);
            if latest <= generation {
                return Ok(Vec::new());
            }
            let oldest = latest.saturating_sub(self.slot_count as u64 - 1).max(1);
            let first = generation.saturating_add(1).max(oldest);
            let mut snapshots = Vec::with_capacity(
                usize::try_from(latest - first + 1).expect("bounded by slot count"),
            );
            let mut stable = true;
            for expected in first..=latest {
                let slot = usize::try_from((expected - 1) % self.slot_count as u64)
                    .expect("shared pose slot index fits usize");
                let start = HEADER_BYTES + slot * self.slot_stride;
                let expected_marker = expected.checked_mul(2).ok_or_else(|| {
                    PoseError::SharedSlot("shared pose generation overflow".to_owned())
                })?;
                if self
                    .atomic(start + SLOT_GENERATION_OFFSET)
                    .load(Ordering::Acquire)
                    != expected_marker
                {
                    stable = false;
                    break;
                }
                let length = self
                    .atomic(start + SLOT_LENGTH_OFFSET)
                    .load(Ordering::Acquire) as usize;
                if length == 0 || length > self.slot_capacity {
                    return Err(PoseError::SharedSlot(
                        "shared pose payload length is invalid".to_owned(),
                    ));
                }
                let payload_start = start + SLOT_HEADER_BYTES;
                let bytes = self.map[payload_start..payload_start + length].to_vec();
                if self
                    .atomic(start + SLOT_GENERATION_OFFSET)
                    .load(Ordering::Acquire)
                    != expected_marker
                {
                    stable = false;
                    break;
                }
                snapshots.push((expected, bytes));
            }
            if stable {
                return Ok(snapshots);
            }
            std::hint::spin_loop();
        }
        Err(PoseError::SharedSlot(
            "shared pose history changed during read".to_owned(),
        ))
    }

    fn atomic(&self, offset: usize) -> &AtomicU64 {
        // The writer validated and aligned every atomic control word before
        // publishing the file.
        unsafe { &*(self.map.as_ptr().add(offset).cast::<AtomicU64>()) }
    }
}

fn declared_usize(map: &[u8], offset: usize) -> Result<usize, PoseError> {
    usize::try_from(u64::from_ne_bytes(
        map[offset..offset + 8].try_into().expect("length"),
    ))
    .map_err(|_| PoseError::SharedSlot("shared pose declaration is too large".to_owned()))
}

fn map_mut(file: &File, length: usize) -> Result<MmapMut, PoseError> {
    // The file has already been sized and is kept alive by the returned map.
    unsafe { MmapOptions::new().len(length).map_mut(file) }.map_err(PoseError::Io)
}

fn map(file: &File, length: usize) -> Result<Mmap, PoseError> {
    // The file length and header are validated before the mapping is exposed.
    unsafe { MmapOptions::new().len(length).map(file) }.map_err(PoseError::Io)
}
