//! Time owns its collection vocabulary; shared plumbing owns observation mechanics.
use super::{CollectionPage, TimeResource};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_knowledge_extension::{
    AccessModel, ChangeSignal, CollectionDescriptor, Freshness, IndexingMode,
};
use veoveo_types::{ResourceAddress, ResourceUri};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeKnowledgeCollection {
    Calendars,
    Epochs,
    Events,
    AuthorityReleases,
    BootstrapAuthorities,
}

impl TimeKnowledgeCollection {
    pub fn for_template(template: &str) -> Option<Self> {
        match template {
            uris::CALENDAR_TEMPLATE => Some(Self::Calendars),
            uris::EPOCH_VERSION_TEMPLATE => Some(Self::Epochs),
            uris::EVENT_TEMPLATE => Some(Self::Events),
            uris::AUTHORITY_RELEASE_TEMPLATE => Some(Self::AuthorityReleases),
            uris::BOOTSTRAP_AUTHORITY_TEMPLATE => Some(Self::BootstrapAuthorities),
            _ => None,
        }
    }
    pub fn descriptor(self) -> CollectionDescriptor {
        let (id, kind, enumerate) = match self {
            Self::Calendars => (
                "time.calendars",
                "calendar-version",
                uris::CALENDARS_TEMPLATE,
            ),
            Self::Epochs => ("time.epochs", "epoch-version", uris::EPOCHS_TEMPLATE),
            Self::Events => ("time.events", "temporal-event", uris::EVENTS_TEMPLATE),
            Self::AuthorityReleases => (
                "time.authority-releases",
                "authority-release",
                uris::AUTHORITY_RELEASES_TEMPLATE,
            ),
            Self::BootstrapAuthorities => (
                "time.bootstrap-authorities",
                "authority-release",
                uris::BOOTSTRAP_AUTHORITIES_TEMPLATE,
            ),
        };
        let (freshness, signal) = if self == Self::Events {
            (Freshness::max_age(300), ChangeSignal::Listen)
        } else {
            (Freshness::immutable(), ChangeSignal::Immutable)
        };
        CollectionDescriptor::new(
            id.parse().expect("Time collection"),
            kind.parse().expect("Time entity"),
            veoveo_types::ResourceTemplateUri::new(enumerate).expect("Time enumeration"),
            freshness,
            signal,
            if self == Self::BootstrapAuthorities {
                AccessModel::Profile
            } else {
                AccessModel::WorkContext
            },
            IndexingMode::Content,
        )
        .expect("Time collection contract")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeResourceEntry {
    pub uri: ResourceUri,
    pub title: String,
}
impl TimeResourceEntry {
    pub fn new(address: TimeResource, title: String) -> Self {
        Self {
            uri: address.to_uri().expect("typed Time address"),
            title,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeResourcePage<C> {
    pub items: Vec<TimeResourceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<C>,
}
impl<C> TimeResourcePage<C> {
    pub fn from_page<T>(
        page: CollectionPage<T, C>,
        entry: impl Fn(T) -> TimeResourceEntry,
    ) -> Self {
        Self {
            items: page.items.into_iter().map(entry).collect(),
            next_cursor: page.next_cursor,
        }
    }
}
