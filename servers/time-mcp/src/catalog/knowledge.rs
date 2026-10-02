//! Source observations are constructed from the same SQL-admitted row as the body.
use super::*;
use crate::{AuthorityCursor, MissionEpochId, TimeKnowledgeCollection, TimeVersion};
use anyhow::ensure;
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_knowledge_extension::{
    AccessDescriptor, ModifiedBy, Observation, ReadPolicy, content_digest,
};
use veoveo_types::AccessSubject;

pub struct ObservedTime<T> {
    value: T,
    collection: TimeKnowledgeCollection,
    access: AccessDescriptor,
    modified_at: DateTime<Utc>,
}

impl<T: Serialize> ObservedTime<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn collection(&self) -> TimeKnowledgeCollection {
        self.collection
    }
    pub fn document(&self) -> Result<(String, Observation)> {
        let text = serde_json::to_string(&self.value)?;
        ensure!(
            text.len() <= 64 * 1024,
            "Time knowledge member exceeds 64 KiB"
        );
        let revision = content_digest(&serde_json::to_string(&(
            &self.value,
            &self.access,
            self.modified_at,
        ))?);
        let AccessSubject::Principal(owner) = &self.access.owner else {
            anyhow::bail!("Time provenance must identify its creating principal");
        };
        let descriptor = self.collection.descriptor();
        let mut observation = Observation::builder(
            descriptor.collection().clone(),
            revision.to_string().parse()?,
            content_digest(&text),
            Utc::now(),
        )
        .access(self.access.clone())
        .modified_at(self.modified_at);
        // Event transitions can come from the scheduler. The event records its
        // creator, but does not record the actor responsible for its latest state.
        if self.collection != TimeKnowledgeCollection::Events {
            observation = observation.modified_by(ModifiedBy::Principal(owner.clone()));
        }
        Ok((text, observation.build(&descriptor)?))
    }
}

impl TimeCatalog {
    pub async fn observed_calendar(
        &self,
        scope: &TimeAccessContext,
        id: &CalendarId,
        version: TimeVersion,
    ) -> Result<Option<ObservedTime<OperationalCalendar>>> {
        self.persistence
            .time_calendar_version(scope.identity.tenant_id, id, version)
            .await?
            .map(|record| {
                let access = record.provenance.access(
                    &record.tenant,
                    &record.owner,
                    ReadPolicy::Tenant {},
                )?;
                Ok(ObservedTime {
                    collection: TimeKnowledgeCollection::Calendars,
                    access,
                    modified_at: record.updated_at,
                    value: calendar_from_record(record)?,
                })
            })
            .transpose()
    }
    pub async fn observed_epoch(
        &self,
        scope: &TimeAccessContext,
        id: &MissionEpochId,
        version: TimeVersion,
    ) -> Result<Option<ObservedTime<MissionEpoch>>> {
        self.persistence
            .time_mission_epoch(scope.identity.tenant_id, id, version)
            .await?
            .map(|record| {
                let access = record.provenance.access(
                    &record.tenant,
                    &record.owner,
                    ReadPolicy::Tenant {},
                )?;
                Ok(ObservedTime {
                    collection: TimeKnowledgeCollection::Epochs,
                    access,
                    modified_at: record.updated_at,
                    value: epoch_from_record(record)?,
                })
            })
            .transpose()
    }
    pub async fn observed_event(
        &self,
        scope: &TimeAccessContext,
        id: &TemporalEventId,
    ) -> Result<Option<ObservedTime<TemporalEvent>>> {
        self.persistence
            .time_temporal_event(&scope.identity, id)
            .await?
            .map(|record| {
                let access = record.provenance.access(
                    &record.tenant,
                    &record.owner,
                    ReadPolicy::Subjects {},
                )?;
                Ok(ObservedTime {
                    collection: TimeKnowledgeCollection::Events,
                    access,
                    modified_at: record.updated_at,
                    value: event_from_record(record)?,
                })
            })
            .transpose()
    }
    pub async fn observed_release(
        &self,
        scope: &TimeAccessContext,
        id: &crate::AuthorityReleaseId,
    ) -> Result<Option<ObservedTime<TimeAuthorityReference>>> {
        let Some(record) = self
            .persistence
            .time_authority_release(scope.identity.tenant_id, id)
            .await?
        else {
            return Ok(None);
        };
        let access =
            record
                .provenance
                .access(&record.tenant, &record.owner, ReadPolicy::Tenant {})?;
        // The compiler reference is immutable; activation changes administrative
        // state, not its bytes or provenance. Validation produced this revision.
        let modified_at = record.validated_at;
        let release = release_from_record(record)?;
        let value = self.authority_reference(scope, &release).await?;
        Ok(Some(ObservedTime {
            collection: TimeKnowledgeCollection::AuthorityReleases,
            value,
            access,
            modified_at,
        }))
    }
    pub async fn releases_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&AuthorityCursor>,
    ) -> Result<crate::CollectionPage<AuthorityRelease, AuthorityCursor>> {
        let rows = self
            .persistence
            .time_authority_releases_page(scope.identity.tenant_id, after)
            .await?;
        crate::index::page(
            rows,
            |row| {
                Ok(AuthorityCursor::new(
                    &row.release_key.parse().map_err(anyhow::Error::msg)?,
                ))
            },
            release_from_record,
        )
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[test]
    fn knowledge_revision_covers_stored_modification_time() {
        let mut member = ObservedTime {
            value: "unchanged calendar",
            collection: TimeKnowledgeCollection::Calendars,
            access: AccessDescriptor {
                tenant: "tenant".parse().unwrap(),
                work_context: "context".parse().unwrap(),
                read_policy: ReadPolicy::Tenant {},
                owner: AccessSubject::Principal("author".parse().unwrap()),
                grants: vec![],
                data_labels: vec![],
                expires_at: None,
            },
            modified_at: Utc::now(),
        };
        let (text, original) = member.document().unwrap();
        assert_eq!(original.revision(), member.document().unwrap().1.revision());
        member.modified_at += chrono::TimeDelta::seconds(1);
        let (same_text, modified) = member.document().unwrap();
        assert_eq!(text, same_text);
        assert_eq!(original.access(), modified.access());
        assert_ne!(original.revision(), modified.revision());
    }
}
