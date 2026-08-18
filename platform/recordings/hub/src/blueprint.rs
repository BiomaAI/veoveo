//! Validation and immutable storage helpers for producer-authored Rerun Blueprints.

use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use re_chunk_store::ChunkStoreConfig;
use re_entity_db::EntityDb;
use re_log_encoding::Decoder;
use re_log_types::{LogMsg, StoreId, StoreKind};
use re_sdk_types::blueprint::components::MapProvider;
use re_types_core::{Component as _, Loggable as _};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlueprintMapProviderSelection {
    None,
    OpenStreetMap,
    Mapbox,
    Mixed,
}

#[derive(Clone, Debug)]
pub struct ValidatedBlueprint {
    pub store_id: StoreId,
    pub message_count: u64,
    pub map_provider: BlueprintMapProviderSelection,
}

pub fn validate_blueprint_rrd(
    encoded_rrd: &[u8],
    declared_message_count: u64,
    expected_application_id: &str,
) -> Result<ValidatedBlueprint> {
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(encoded_rrd)))
        .context("decoding recording Blueprint RRD")?;
    let mut store_id: Option<StoreId> = None;
    let mut database: Option<EntityDb> = None;
    let mut count = 0_u64;
    let mut store_info_count = 0_u64;
    let mut activation_count = 0_u64;
    let mut activated = false;
    let mut open_street_map = false;
    let mut mapbox = false;
    for message in decoder {
        let message = message.context("decoding recording Blueprint message")?;
        ensure!(
            !activated,
            "recording Blueprint contains data after activation"
        );
        let message_store_id = message.store_id();
        ensure!(
            message_store_id.kind() == StoreKind::Blueprint,
            "recording Blueprint payload contains a non-Blueprint store"
        );
        ensure!(
            message_store_id.application_id().as_str() == expected_application_id,
            "recording Blueprint application does not match its recording"
        );
        if let Some(expected) = &store_id {
            ensure!(
                message_store_id == expected,
                "recording Blueprint payload contains multiple store identities"
            );
        } else {
            store_id = Some(message_store_id.clone());
            database = Some(EntityDb::with_store_config(
                message_store_id.clone(),
                false,
                ChunkStoreConfig::ALL_DISABLED,
            ));
        }
        match &message {
            LogMsg::SetStoreInfo(_) => {
                store_info_count += 1;
            }
            LogMsg::ArrowMsg(_, arrow) => {
                let chunk = re_chunk::Chunk::from_arrow_msg(arrow)
                    .context("decoding recording Blueprint chunk")?;
                for column in chunk
                    .components()
                    .values()
                    .filter(|column| column.descriptor.component_type == Some(MapProvider::name()))
                {
                    for provider in MapProvider::from_arrow_opt(column.list_array.values().as_ref())
                        .context("decoding recording Blueprint map provider")?
                        .into_iter()
                        .flatten()
                    {
                        match provider {
                            MapProvider::OpenStreetMap => open_street_map = true,
                            MapProvider::MapboxStreets
                            | MapProvider::MapboxDark
                            | MapProvider::MapboxSatellite
                            | MapProvider::MapboxLight => mapbox = true,
                        }
                    }
                }
            }
            LogMsg::BlueprintActivationCommand(command) => {
                activation_count += 1;
                ensure!(
                    command.make_active && command.make_default,
                    "governed producer Blueprint must be active and default"
                );
                activated = true;
            }
        }
        database
            .as_mut()
            .expect("database follows store identity")
            .add_log_msg(&message)
            .context("indexing recording Blueprint")?;
        count += 1;
    }
    ensure!(
        count == declared_message_count,
        "recording Blueprint message count mismatch"
    );
    ensure!(
        store_info_count > 0,
        "recording Blueprint requires SetStoreInfo"
    );
    ensure!(
        activation_count == 1,
        "recording Blueprint requires one terminal activation"
    );
    Ok(ValidatedBlueprint {
        store_id: store_id.context("recording Blueprint contains no messages")?,
        message_count: count,
        map_provider: match (open_street_map, mapbox) {
            (false, false) => BlueprintMapProviderSelection::None,
            (true, false) => BlueprintMapProviderSelection::OpenStreetMap,
            (false, true) => BlueprintMapProviderSelection::Mapbox,
            (true, true) => BlueprintMapProviderSelection::Mixed,
        },
    })
}

pub fn blueprint_relative_path(tenant_id: &str, recording_id: &str, revision: u64) -> PathBuf {
    PathBuf::from("blueprints")
        .join(tenant_id)
        .join(recording_id)
        .join(format!("{revision:020}.rbl"))
}

pub fn ensure_blueprint_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        relative.is_relative(),
        "recording Blueprint path must be relative"
    );
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "recording Blueprint path contains an unsafe component"
    );
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use re_log_encoding::{Encoder, EncodingOptions};
    use re_log_types::ApplicationId;
    use re_sdk::{
        RecordingStreamBuilder,
        blueprint::{Blueprint, MapView},
    };

    use super::*;

    fn blueprint_rrd(application_id: &str) -> Vec<u8> {
        blueprint_value_rrd(application_id, Blueprint::auto())
    }

    fn blueprint_value_rrd(application_id: &str, blueprint: Blueprint) -> Vec<u8> {
        let (recording, storage) =
            RecordingStreamBuilder::new(ApplicationId::try_new(application_id).unwrap())
                .recording_id("recording-a")
                .memory()
                .unwrap();
        blueprint.send(&recording, Default::default()).unwrap();
        let mut encoder = Encoder::new_eager(
            re_build_info::CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        let messages = storage
            .take()
            .into_iter()
            .filter(|message| message.store_id().kind() == StoreKind::Blueprint)
            .collect::<Vec<_>>();
        for message in &messages {
            encoder.append(message).unwrap();
        }
        encoder.finish().unwrap();
        encoder.into_inner().unwrap()
    }

    #[test]
    fn validates_one_complete_active_blueprint() {
        let bytes = blueprint_rrd("anonymous-app");
        let count = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(&bytes)))
            .unwrap()
            .count() as u64;
        let validated = validate_blueprint_rrd(&bytes, count, "anonymous-app").unwrap();
        assert_eq!(validated.store_id.kind(), StoreKind::Blueprint);
    }

    #[test]
    fn rejects_cross_application_blueprints() {
        let bytes = blueprint_rrd("other-app");
        let count = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(&bytes)))
            .unwrap()
            .count() as u64;
        assert!(validate_blueprint_rrd(&bytes, count, "anonymous-app").is_err());
    }

    #[test]
    fn reports_the_producer_map_provider_family() {
        let blueprint =
            Blueprint::new(MapView::new("Map").with_map_provider(
                re_sdk_types::blueprint::components::MapProvider::MapboxSatellite,
            ));
        let bytes = blueprint_value_rrd("anonymous-app", blueprint);
        let count = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(&bytes)))
            .unwrap()
            .count() as u64;
        let validated = validate_blueprint_rrd(&bytes, count, "anonymous-app").unwrap();
        assert_eq!(
            validated.map_provider,
            BlueprintMapProviderSelection::Mapbox
        );
    }

    #[test]
    fn blueprint_paths_cannot_enter_the_recording_segment_scan() {
        let relative = blueprint_relative_path("tenant", "recording", 7);
        assert_eq!(
            relative.extension().and_then(|value| value.to_str()),
            Some("rbl")
        );
        assert!(ensure_blueprint_path(Path::new("/spool"), &relative).is_ok());
    }
}
