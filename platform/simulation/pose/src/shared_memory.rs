use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use memmap2::{Mmap, MmapMut, MmapOptions};

use crate::PoseError;

const MAGIC: &[u8; 8] = b"VVPSHM01";
const HEADER_BYTES: usize = 64;
const GENERATION_OFFSET: usize = 16;
const ACTIVE_SLOT_OFFSET: usize = 24;
const SLOT_ZERO_LENGTH_OFFSET: usize = 32;
const SLOT_ONE_LENGTH_OFFSET: usize = 40;

pub struct SharedPoseWriter {
    map: MmapMut,
    slot_capacity: usize,
}

impl SharedPoseWriter {
    pub fn create(path: &Path, slot_capacity: usize) -> Result<Self, PoseError> {
        if slot_capacity == 0 {
            return Err(PoseError::SharedSlot(
                "slot capacity must be positive".to_owned(),
            ));
        }
        let total_bytes = HEADER_BYTES
            .checked_add(slot_capacity.checked_mul(2).ok_or_else(|| {
                PoseError::SharedSlot("shared pose slot size overflow".to_owned())
            })?)
            .ok_or_else(|| PoseError::SharedSlot("shared pose slot size overflow".to_owned()))?;
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)?;
        file.set_len(total_bytes as u64)?;
        let mut map = map_mut(&file, total_bytes)?;
        map[..HEADER_BYTES].fill(0);
        map[..8].copy_from_slice(MAGIC);
        map[8..10].copy_from_slice(&1_u16.to_ne_bytes());
        map[48..56].copy_from_slice(&(slot_capacity as u64).to_ne_bytes());
        map.flush()?;
        Ok(Self { map, slot_capacity })
    }

    pub fn publish(&mut self, encoded_snapshot: &[u8]) -> Result<u64, PoseError> {
        if encoded_snapshot.len() > self.slot_capacity {
            return Err(PoseError::MessageBytes {
                actual: encoded_snapshot.len(),
                maximum: self.slot_capacity,
            });
        }
        let generation = self
            .atomic(GENERATION_OFFSET)
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        debug_assert_eq!(generation % 2, 1);
        let active = self.atomic(ACTIVE_SLOT_OFFSET).load(Ordering::Acquire) as usize;
        let inactive = 1 - active;
        let start = HEADER_BYTES + inactive * self.slot_capacity;
        self.map[start..start + encoded_snapshot.len()].copy_from_slice(encoded_snapshot);
        let length_offset = if inactive == 0 {
            SLOT_ZERO_LENGTH_OFFSET
        } else {
            SLOT_ONE_LENGTH_OFFSET
        };
        self.atomic(length_offset)
            .store(encoded_snapshot.len() as u64, Ordering::Release);
        self.atomic(ACTIVE_SLOT_OFFSET)
            .store(inactive as u64, Ordering::Release);
        let generation = self
            .atomic(GENERATION_OFFSET)
            .fetch_add(1, Ordering::Release)
            + 1;
        Ok(generation / 2)
    }

    fn atomic(&self, offset: usize) -> &AtomicU64 {
        // The map is page-aligned and every control offset is aligned to eight
        // bytes. The writer owns initialization before another process opens it.
        unsafe { &*(self.map.as_ptr().add(offset).cast::<AtomicU64>()) }
    }
}

pub struct SharedPoseReader {
    map: Mmap,
    slot_capacity: usize,
}

impl SharedPoseReader {
    pub fn open(path: &Path) -> Result<Self, PoseError> {
        let file = File::open(path)?;
        let length = usize::try_from(file.metadata()?.len())
            .map_err(|_| PoseError::SharedSlot("shared pose file is too large".to_owned()))?;
        if length < HEADER_BYTES || (length - HEADER_BYTES) % 2 != 0 {
            return Err(PoseError::SharedSlot(
                "shared pose file has an invalid length".to_owned(),
            ));
        }
        let map = map(&file, length)?;
        if &map[..8] != MAGIC || u16::from_ne_bytes(map[8..10].try_into().expect("length")) != 1 {
            return Err(PoseError::SharedSlot(
                "shared pose file has invalid identity".to_owned(),
            ));
        }
        let slot_capacity = (length - HEADER_BYTES) / 2;
        let declared = u64::from_ne_bytes(map[48..56].try_into().expect("length")) as usize;
        if slot_capacity == 0 || declared != slot_capacity {
            return Err(PoseError::SharedSlot(
                "shared pose slot capacity does not match".to_owned(),
            ));
        }
        Ok(Self { map, slot_capacity })
    }

    pub fn latest(&self) -> Result<Option<(u64, Vec<u8>)>, PoseError> {
        for _ in 0..3 {
            let generation = self.atomic(GENERATION_OFFSET).load(Ordering::Acquire);
            if generation == 0 {
                return Ok(None);
            }
            if generation % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let active = self.atomic(ACTIVE_SLOT_OFFSET).load(Ordering::Acquire) as usize;
            if active > 1 {
                return Err(PoseError::SharedSlot(
                    "shared pose active slot is invalid".to_owned(),
                ));
            }
            let length_offset = if active == 0 {
                SLOT_ZERO_LENGTH_OFFSET
            } else {
                SLOT_ONE_LENGTH_OFFSET
            };
            let length = self.atomic(length_offset).load(Ordering::Acquire) as usize;
            if length == 0 || length > self.slot_capacity {
                return Err(PoseError::SharedSlot(
                    "shared pose payload length is invalid".to_owned(),
                ));
            }
            let start = HEADER_BYTES + active * self.slot_capacity;
            let bytes = self.map[start..start + length].to_vec();
            if generation == self.atomic(GENERATION_OFFSET).load(Ordering::Acquire)
                && active == self.atomic(ACTIVE_SLOT_OFFSET).load(Ordering::Acquire) as usize
            {
                return Ok(Some((generation / 2, bytes)));
            }
        }
        Ok(None)
    }

    fn atomic(&self, offset: usize) -> &AtomicU64 {
        // The writer creates the page-aligned map and aligned control words.
        unsafe { &*(self.map.as_ptr().add(offset).cast::<AtomicU64>()) }
    }
}

fn map_mut(file: &File, length: usize) -> Result<MmapMut, PoseError> {
    // The file has already been sized and is kept alive by the returned map.
    unsafe { MmapOptions::new().len(length).map_mut(file) }.map_err(PoseError::Io)
}

fn map(file: &File, length: usize) -> Result<Mmap, PoseError> {
    // The file length and header were validated before the mapping is exposed.
    unsafe { MmapOptions::new().len(length).map(file) }.map_err(PoseError::Io)
}
