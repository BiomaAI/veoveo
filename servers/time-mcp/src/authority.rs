use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use jiff::tz::TimeZoneDatabase;

use crate::contract::AuthorityBinding;

const NTP_UNIX_EPOCH_DELTA_SECONDS: i64 = 2_208_988_800;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeapSecond {
    pub effective_unix_seconds: i64,
    pub tai_minus_utc_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeapSecondTable {
    entries: Vec<LeapSecond>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcCoordinate {
    pub unix_seconds: i64,
    pub is_leap_second: bool,
}

impl LeapSecondTable {
    pub fn from_iana_content(content: &str) -> Result<Self> {
        let mut entries = Vec::new();
        for (line_number, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut columns = line.split_whitespace();
            let ntp_seconds: i64 = columns
                .next()
                .context("leap-second row has no effective instant")?
                .parse()
                .with_context(|| {
                    format!(
                        "invalid NTP instant on leap-second line {}",
                        line_number + 1
                    )
                })?;
            let offset: i64 = columns
                .next()
                .context("leap-second row has no TAI-UTC offset")?
                .parse()
                .with_context(|| {
                    format!(
                        "invalid TAI-UTC offset on leap-second line {}",
                        line_number + 1
                    )
                })?;
            if !(10..=255).contains(&offset) {
                bail!("TAI-UTC offset is outside the supported range");
            }
            entries.push(LeapSecond {
                effective_unix_seconds: ntp_seconds
                    .checked_sub(NTP_UNIX_EPOCH_DELTA_SECONDS)
                    .context("leap-second NTP instant exceeds the supported range")?,
                tai_minus_utc_seconds: offset,
            });
        }
        if entries.is_empty() {
            bail!("leap-second authority contains no entries");
        }
        entries.sort_by_key(|entry| entry.effective_unix_seconds);
        if entries.windows(2).any(|pair| {
            pair[0].effective_unix_seconds >= pair[1].effective_unix_seconds
                || pair[0].tai_minus_utc_seconds >= pair[1].tai_minus_utc_seconds
        }) {
            bail!("leap-second authority is not strictly monotonic");
        }
        Ok(Self { entries })
    }

    pub async fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let content = tokio::fs::read_to_string(path.as_ref())
            .await
            .with_context(|| format!("reading {}", path.as_ref().display()))?;
        Self::from_iana_content(&content)
    }

    pub fn offset_for_utc(&self, unix_seconds: i64) -> Result<i64> {
        self.entries
            .iter()
            .rev()
            .find(|entry| unix_seconds >= entry.effective_unix_seconds)
            .or_else(|| self.entries.first())
            .map(|entry| entry.tai_minus_utc_seconds)
            .context("leap-second authority has no applicable entry")
    }

    pub fn utc_from_tai(&self, tai_seconds_since_1970: i64) -> Result<UtcCoordinate> {
        let mut offset = self
            .entries
            .first()
            .context("leap-second authority is empty")?
            .tai_minus_utc_seconds;
        for pair in self.entries.windows(2) {
            let previous = &pair[0];
            let next = &pair[1];
            let leap_start = next
                .effective_unix_seconds
                .checked_add(previous.tai_minus_utc_seconds)
                .context("leap-second authority exceeds the supported range")?;
            let next_ordinary = next
                .effective_unix_seconds
                .checked_add(next.tai_minus_utc_seconds)
                .context("leap-second authority exceeds the supported range")?;
            if tai_seconds_since_1970 < leap_start {
                break;
            }
            if tai_seconds_since_1970 < next_ordinary {
                return Ok(UtcCoordinate {
                    unix_seconds: next
                        .effective_unix_seconds
                        .checked_sub(1)
                        .context("leap-second UTC instant exceeds the supported range")?,
                    is_leap_second: true,
                });
            }
            offset = next.tai_minus_utc_seconds;
        }
        let utc = tai_seconds_since_1970
            .checked_sub(offset)
            .context("TAI instant exceeds the UTC seconds range")?;
        Ok(UtcCoordinate {
            unix_seconds: utc,
            is_leap_second: false,
        })
    }

    pub fn entries(&self) -> &[LeapSecond] {
        &self.entries
    }
}

#[derive(Clone)]
pub struct AuthorityContext {
    binding: AuthorityBinding,
    effective: crate::contract::EffectiveTimeAuthority,
    tzdb: TimeZoneDatabase,
    leap_seconds: Arc<LeapSecondTable>,
}

impl AuthorityContext {
    pub fn binding(&self) -> &AuthorityBinding {
        &self.binding
    }
    pub fn effective(&self) -> &crate::contract::EffectiveTimeAuthority {
        &self.effective
    }
    pub fn tzdb(&self) -> &TimeZoneDatabase {
        &self.tzdb
    }
    pub fn leap_seconds(&self) -> &Arc<LeapSecondTable> {
        &self.leap_seconds
    }

    pub fn from_paths(
        effective: crate::contract::EffectiveTimeAuthority,
        tzdb_directory: impl AsRef<Path>,
        leap_seconds: LeapSecondTable,
    ) -> Result<Self> {
        let tzdb = TimeZoneDatabase::from_dir(tzdb_directory.as_ref()).with_context(|| {
            format!(
                "loading TZif authority from {}",
                tzdb_directory.as_ref().display()
            )
        })?;
        tzdb.get("UTC")
            .context("TZDB authority does not contain UTC")?;
        Ok(Self {
            binding: effective.binding(),
            effective,
            tzdb,
            leap_seconds: Arc::new(leap_seconds),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAPS: &str = "# fixture\n2272060800 10\n2287785600 11\n3692217600 37\n";

    #[test]
    fn validates_and_applies_versioned_leap_seconds() {
        let table = LeapSecondTable::from_iana_content(LEAPS).unwrap();
        assert_eq!(table.offset_for_utc(0).unwrap(), 10);
        assert_eq!(table.offset_for_utc(1_483_228_800).unwrap(), 37);
        let tai = 1_483_228_800 + 37;
        assert_eq!(
            table.utc_from_tai(tai).unwrap(),
            UtcCoordinate {
                unix_seconds: 1_483_228_800,
                is_leap_second: false,
            }
        );
        assert_eq!(
            table.utc_from_tai(tai - 1).unwrap(),
            UtcCoordinate {
                unix_seconds: 1_483_228_799,
                is_leap_second: true,
            }
        );
    }

    #[test]
    fn rejects_non_monotonic_authority() {
        assert!(LeapSecondTable::from_iana_content("2272060800 11\n2287785600 10\n").is_err());
    }

    #[test]
    fn epoch_offsets_reject_overflow_and_preserve_representable_endpoints() {
        let too_early = format!("{} 10\n", i64::MIN);
        assert!(
            LeapSecondTable::from_iana_content(&too_early)
                .unwrap_err()
                .to_string()
                .contains("NTP instant exceeds")
        );
        let first_ntp = i64::MIN + NTP_UNIX_EPOCH_DELTA_SECONDS;
        let table = LeapSecondTable::from_iana_content(&format!(
            "{first_ntp} 10\n{} 11\n",
            first_ntp + 1000
        ))
        .unwrap();
        assert_eq!(table.entries()[0].effective_unix_seconds, i64::MIN);
        assert!(
            table
                .utc_from_tai(i64::MIN)
                .unwrap_err()
                .to_string()
                .contains("UTC seconds range")
        );
        assert_eq!(
            table.utc_from_tai(i64::MIN + 10).unwrap().unix_seconds,
            i64::MIN
        );
        assert_eq!(
            table.utc_from_tai(i64::MAX).unwrap().unix_seconds,
            i64::MAX - 11
        );
    }

    #[test]
    fn inverse_conversion_resolves_each_transition_and_ordinary_neighbor() {
        let table = LeapSecondTable::from_iana_content(LEAPS).unwrap();
        for entry in table.entries() {
            for delta in -3..=3 {
                let utc = entry.effective_unix_seconds + delta;
                let tai = utc + table.offset_for_utc(utc).unwrap();
                assert_eq!(
                    table.utc_from_tai(tai).unwrap(),
                    UtcCoordinate {
                        unix_seconds: utc,
                        is_leap_second: false,
                    }
                );
            }
        }
        for pair in table.entries().windows(2) {
            let next = pair[1].effective_unix_seconds;
            for offset in pair[0].tai_minus_utc_seconds..pair[1].tai_minus_utc_seconds {
                assert_eq!(
                    table.utc_from_tai(next + offset).unwrap(),
                    UtcCoordinate {
                        unix_seconds: next - 1,
                        is_leap_second: true,
                    }
                );
            }
        }
    }
}
