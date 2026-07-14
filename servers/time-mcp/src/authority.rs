use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use jiff::tz::TimeZoneDatabase;

use crate::contract::{AuthorityBinding, AuthorityReleaseId};

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
                effective_unix_seconds: ntp_seconds - NTP_UNIX_EPOCH_DELTA_SECONDS,
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

    pub fn utc_from_tai(&self, tai_seconds_since_1970: i64) -> Result<i64> {
        let mut utc = tai_seconds_since_1970
            - self
                .entries
                .last()
                .context("leap-second authority is empty")?
                .tai_minus_utc_seconds;
        for _ in 0..4 {
            let candidate = tai_seconds_since_1970 - self.offset_for_utc(utc)?;
            if candidate == utc {
                return Ok(utc);
            }
            utc = candidate;
        }
        Ok(utc)
    }

    pub fn entries(&self) -> &[LeapSecond] {
        &self.entries
    }
}

#[derive(Clone)]
pub struct AuthorityContext {
    pub binding: AuthorityBinding,
    pub tzdb: TimeZoneDatabase,
    pub leap_seconds: Arc<LeapSecondTable>,
}

impl AuthorityContext {
    pub fn from_paths(
        tzdb_release_id: AuthorityReleaseId,
        leap_seconds_release_id: AuthorityReleaseId,
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
            binding: AuthorityBinding {
                tzdb_release_id,
                leap_seconds_release_id,
            },
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
        assert_eq!(table.utc_from_tai(tai).unwrap(), 1_483_228_800);
    }

    #[test]
    fn rejects_non_monotonic_authority() {
        assert!(LeapSecondTable::from_iana_content("2272060800 11\n2287785600 10\n").is_err());
    }
}
