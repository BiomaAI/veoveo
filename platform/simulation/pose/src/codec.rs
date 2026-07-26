use crate::{
    CoordinateConvention, EntityId, EntityPose, EnuPosition, EpochId, FluVelocity, FrameRevision,
    POSE_PROTOCOL_VERSION, PoseError, PoseLimits, PoseSnapshot, QuaternionXyzw, Rgba8,
    SemanticDisplayState, SessionId, Sha256Digest,
};

const MAGIC: &[u8; 8] = b"VVPOSE01";
const HEADER_BYTES: usize = 116;

pub fn encode_snapshot(snapshot: &PoseSnapshot, limits: &PoseLimits) -> Result<Vec<u8>, PoseError> {
    snapshot.validate(limits)?;
    let mut bytes = Vec::with_capacity(HEADER_BYTES + snapshot.entities.len() * 96);
    bytes.extend_from_slice(MAGIC);
    push_u16(&mut bytes, snapshot.protocol_version);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u64(&mut bytes, snapshot.sequence);
    push_i64(&mut bytes, snapshot.simulation_timestamp_ns);
    push_u64(&mut bytes, snapshot.entity_table_revision);
    push_u32(
        &mut bytes,
        u32::try_from(snapshot.entities.len()).map_err(|_| PoseError::MessageBytes {
            actual: usize::MAX,
            maximum: limits.max_message_bytes,
        })?,
    );
    push_string_length(&mut bytes, snapshot.session_id.as_str())?;
    push_string_length(&mut bytes, snapshot.epoch_id.as_str())?;
    push_string_length(&mut bytes, &snapshot.frame_revision.uri)?;
    push_u16(&mut bytes, 1);
    bytes.extend_from_slice(&snapshot.frame_revision.digest.as_bytes());
    bytes.extend_from_slice(&snapshot.entity_table_digest.as_bytes());
    bytes.extend_from_slice(snapshot.session_id.as_str().as_bytes());
    bytes.extend_from_slice(snapshot.epoch_id.as_str().as_bytes());
    bytes.extend_from_slice(snapshot.frame_revision.uri.as_bytes());

    for entity in &snapshot.entities {
        push_string_length(&mut bytes, entity.entity_id.as_str())?;
        let mut flags = 0_u8;
        flags |= u8::from(entity.active);
        flags |= u8::from(entity.visible) << 1;
        flags |= u8::from(entity.velocity.is_some()) << 2;
        flags |= u8::from(entity.display.is_some()) << 3;
        bytes.push(flags);
        bytes.push(0);
        for value in [
            entity.position.east_m,
            entity.position.north_m,
            entity.position.up_m,
            entity.orientation.x,
            entity.orientation.y,
            entity.orientation.z,
            entity.orientation.w,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        if let Some(velocity) = entity.velocity {
            for value in [
                velocity.forward_mps,
                velocity.left_mps,
                velocity.up_mps,
                velocity.roll_rps,
                velocity.pitch_rps,
                velocity.yaw_rps,
            ] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        if let Some(display) = entity.display {
            bytes.extend_from_slice(&[
                display.color.red,
                display.color.green,
                display.color.blue,
                display.color.alpha,
            ]);
            push_u16(&mut bytes, display.status_code);
        }
        bytes.extend_from_slice(entity.entity_id.as_str().as_bytes());
    }
    if bytes.len() > limits.max_message_bytes {
        return Err(PoseError::MessageBytes {
            actual: bytes.len(),
            maximum: limits.max_message_bytes,
        });
    }
    let message_len = u32::try_from(bytes.len()).map_err(|_| PoseError::MessageBytes {
        actual: bytes.len(),
        maximum: limits.max_message_bytes,
    })?;
    bytes[12..16].copy_from_slice(&message_len.to_be_bytes());
    Ok(bytes)
}

pub fn decode_snapshot(bytes: &[u8], limits: &PoseLimits) -> Result<PoseSnapshot, PoseError> {
    if bytes.len() > limits.max_message_bytes {
        return Err(PoseError::MessageBytes {
            actual: bytes.len(),
            maximum: limits.max_message_bytes,
        });
    }
    if bytes.len() < HEADER_BYTES {
        return Err(PoseError::Truncated);
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != MAGIC {
        return Err(PoseError::InvalidMagic);
    }
    let protocol_version = reader.u16()?;
    if protocol_version != POSE_PROTOCOL_VERSION {
        return Err(PoseError::UnsupportedVersion(protocol_version));
    }
    let flags = reader.u16()?;
    if flags != 0 {
        return Err(PoseError::UnsupportedVersion(protocol_version));
    }
    let declared_length = reader.u32()? as usize;
    if declared_length != bytes.len() {
        return Err(PoseError::Truncated);
    }
    let sequence = reader.u64()?;
    let simulation_timestamp_ns = reader.i64()?;
    let entity_table_revision = reader.u64()?;
    let entity_count = reader.u32()? as usize;
    let session_len = reader.u16()? as usize;
    let epoch_len = reader.u16()? as usize;
    let frame_len = reader.u16()? as usize;
    let convention = match reader.u16()? {
        1 => CoordinateConvention::EnuMetersFluXyzw,
        _ => return Err(PoseError::UnsupportedVersion(protocol_version)),
    };
    let frame_digest = Sha256Digest::from_bytes(reader.array_32()?);
    let entity_table_digest = Sha256Digest::from_bytes(reader.array_32()?);
    let session_id = SessionId::new(reader.string(session_len)?)?;
    let epoch_id = EpochId::new(reader.string(epoch_len)?)?;
    let frame_revision = FrameRevision {
        uri: reader.string(frame_len)?,
        digest: frame_digest,
    };
    let mut entities = Vec::with_capacity(entity_count);
    for _ in 0..entity_count {
        let id_len = reader.u16()? as usize;
        let flags = reader.u8()?;
        if flags & !0x0f != 0 || reader.u8()? != 0 {
            return Err(PoseError::UnsupportedVersion(protocol_version));
        }
        let position = EnuPosition {
            east_m: reader.f64()?,
            north_m: reader.f64()?,
            up_m: reader.f64()?,
        };
        let orientation = QuaternionXyzw {
            x: reader.f64()?,
            y: reader.f64()?,
            z: reader.f64()?,
            w: reader.f64()?,
        };
        let velocity = if flags & 0x04 != 0 {
            Some(FluVelocity {
                forward_mps: reader.f32()?,
                left_mps: reader.f32()?,
                up_mps: reader.f32()?,
                roll_rps: reader.f32()?,
                pitch_rps: reader.f32()?,
                yaw_rps: reader.f32()?,
            })
        } else {
            None
        };
        let display = if flags & 0x08 != 0 {
            Some(SemanticDisplayState {
                color: Rgba8 {
                    red: reader.u8()?,
                    green: reader.u8()?,
                    blue: reader.u8()?,
                    alpha: reader.u8()?,
                },
                status_code: reader.u16()?,
            })
        } else {
            None
        };
        entities.push(EntityPose {
            entity_id: EntityId::new(reader.string(id_len)?)?,
            position,
            orientation,
            active: flags & 0x01 != 0,
            visible: flags & 0x02 != 0,
            velocity,
            display,
        });
    }
    if !reader.is_empty() {
        return Err(PoseError::TrailingBytes);
    }
    let snapshot = PoseSnapshot {
        protocol_version,
        session_id,
        epoch_id,
        sequence,
        simulation_timestamp_ns,
        frame_revision,
        coordinate_convention: convention,
        entity_table_revision,
        entity_table_digest,
        entities,
    };
    snapshot.validate(limits)?;
    Ok(snapshot)
}

fn push_string_length(bytes: &mut Vec<u8>, value: &str) -> Result<(), PoseError> {
    let length = u16::try_from(value.len()).map_err(|_| PoseError::MessageBytes {
        actual: value.len(),
        maximum: u16::MAX as usize,
    })?;
    push_u16(bytes, length);
    Ok(())
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], PoseError> {
        if self.remaining.len() < count {
            return Err(PoseError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(count);
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, PoseError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, PoseError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("length"),
        ))
    }

    fn u32(&mut self) -> Result<u32, PoseError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("length"),
        ))
    }

    fn u64(&mut self) -> Result<u64, PoseError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("length"),
        ))
    }

    fn i64(&mut self) -> Result<i64, PoseError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().expect("length"),
        ))
    }

    fn f32(&mut self) -> Result<f32, PoseError> {
        Ok(f32::from_be_bytes(
            self.take(4)?.try_into().expect("length"),
        ))
    }

    fn f64(&mut self) -> Result<f64, PoseError> {
        Ok(f64::from_be_bytes(
            self.take(8)?.try_into().expect("length"),
        ))
    }

    fn array_32(&mut self) -> Result<[u8; 32], PoseError> {
        Ok(self.take(32)?.try_into().expect("length"))
    }

    fn string(&mut self, count: usize) -> Result<String, PoseError> {
        std::str::from_utf8(self.take(count)?)
            .map(str::to_owned)
            .map_err(|_| PoseError::Truncated)
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}
