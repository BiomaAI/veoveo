//! Recording-scoped Rerun MessageProxy service.
//!
//! Rerun WebViewer consumes live SDK messages through its native gRPC-Web
//! `ReadMessages` stream. Authorization and recording selection happen before
//! this service is constructed, so the empty upstream request cannot escape
//! its one governed recording. One receiver follows cataloged segment
//! rollovers without reopening the WebViewer or replaying rows it already saw.

use std::{pin::Pin, time::Duration};

use futures::{Stream, StreamExt as _};
use re_log_encoding::{ToTransport as _, rrd::Compression};
use re_log_types::{LogMsg, StoreId};
use re_protos::log_msg::v1alpha1::LogMsg as TransportLogMsg;
use re_protos::sdk_comms::v1alpha1::{
    ReadMessagesRequest, ReadMessagesResponse, ReadTablesRequest, ReadTablesResponse,
    WriteMessagesRequest, WriteMessagesResponse, WriteTableRequest, WriteTableResponse,
    message_proxy_service_server::{MessageProxyService, MessageProxyServiceServer},
};
use tonic::{Request, Response as TonicResponse, Status};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{PlatformTable, RecordingId, RecordingState, SegmentRecord};

use crate::{
    live_playback::stream_live_messages,
    service::{PlaybackLiveSegmentPlan, RecordingService},
};

pub const READ_MESSAGES_RPC_PATH: &str =
    "/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages";

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct AuthorizedLiveMessageProxy {
    recordings: RecordingService,
    identity: GatewayInternalIdentity,
    recording_id: RecordingId,
    history: Duration,
    playback_store_id: StoreId,
}

impl AuthorizedLiveMessageProxy {
    pub fn new(
        recordings: RecordingService,
        identity: GatewayInternalIdentity,
        recording_id: RecordingId,
        history: Duration,
        playback_store_id: StoreId,
    ) -> Self {
        Self {
            recordings,
            identity,
            recording_id,
            history,
            playback_store_id,
        }
    }

    pub fn service(self) -> MessageProxyServiceServer<Self> {
        MessageProxyServiceServer::new(self)
            .max_decoding_message_size(MAX_MESSAGE_BYTES)
            .max_encoding_message_size(MAX_MESSAGE_BYTES)
    }
}

#[tonic::async_trait]
impl MessageProxyService for AuthorizedLiveMessageProxy {
    async fn write_messages(
        &self,
        _request: Request<tonic::Streaming<WriteMessagesRequest>>,
    ) -> Result<TonicResponse<WriteMessagesResponse>, Status> {
        Err(Status::unimplemented("live playback is read-only"))
    }

    type ReadMessagesStream =
        Pin<Box<dyn Stream<Item = Result<ReadMessagesResponse, Status>> + Send + 'static>>;

    async fn read_messages(
        &self,
        _request: Request<ReadMessagesRequest>,
    ) -> Result<TonicResponse<Self::ReadMessagesStream>, Status> {
        let recordings = self.recordings.clone();
        let identity = self.identity.clone();
        let recording_id = self.recording_id;
        let history = self.history;
        let playback_store_id = self.playback_store_id.clone();
        // Open LIVE before reading the projection. Segment changes racing the
        // initial authorization stay queued as wakeups, while the typed plan
        // remains the source of truth for authorization and paths.
        let mut segment_wake = recordings
            .platform_store()
            .live::<SegmentRecord>(PlatformTable::Segment)
            .await
            .map_err(|error| {
                Status::unavailable(format!("subscribe to recording segments: {error}"))
            })?;
        let stream = async_stream::try_stream! {
            let mut last_ordinal = None;
            let mut store_info_sent = false;
            loop {
                let next = loop {
                    let plan = recordings
                        .playback_plan(&identity, recording_id)
                        .await
                        .map_err(|error| Status::internal(format!("authorize live recording: {error}")))?
                        .ok_or_else(|| Status::not_found("recording is not visible"))?;
                    if let Some(live) = next_live_segment(plan.live, last_ordinal) {
                        break Some(live);
                    }
                    if plan.state != RecordingState::Live {
                        break None;
                    }
                    match segment_wake.next().await {
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            Err(Status::unavailable(format!("recording segment subscription failed: {error}")))?;
                        }
                        None => Err(Status::unavailable("recording segment subscription ended"))?,
                    }
                };
                let Some(live) = next else { break; };
                let ordinal = live.descriptor.ordinal;
                let segment_id = live.descriptor.segment_id.clone();
                let mut receiver = stream_live_messages(
                    live.path,
                    history,
                    playback_store_id.clone(),
                );
                tracing::info!(%recording_id, %segment_id, ordinal, "governed Rerun playback following live segment");
                while let Some(message) = receiver.recv().await {
                    let message = message.map_err(|error| Status::unavailable(error.to_string()))?;
                    if matches!(message, LogMsg::SetStoreInfo(_)) {
                        if store_info_sent {
                            continue;
                        }
                        store_info_sent = true;
                    }
                    yield encode_response(message)?;
                }
                last_ordinal = Some(ordinal);
            }
        };
        Ok(TonicResponse::new(Box::pin(stream)))
    }

    async fn write_table(
        &self,
        _request: Request<WriteTableRequest>,
    ) -> Result<TonicResponse<WriteTableResponse>, Status> {
        Err(Status::unimplemented("live playback is read-only"))
    }

    type ReadTablesStream =
        Pin<Box<dyn Stream<Item = Result<ReadTablesResponse, Status>> + Send + 'static>>;

    async fn read_tables(
        &self,
        _request: Request<ReadTablesRequest>,
    ) -> Result<TonicResponse<Self::ReadTablesStream>, Status> {
        Err(Status::unimplemented(
            "live playback exposes Rerun log messages only",
        ))
    }
}

fn next_live_segment(
    live: Option<PlaybackLiveSegmentPlan>,
    last_ordinal: Option<i64>,
) -> Option<PlaybackLiveSegmentPlan> {
    live.filter(|candidate| last_ordinal.is_none_or(|last| candidate.descriptor.ordinal > last))
}

fn encode_response(message: LogMsg) -> Result<ReadMessagesResponse, Status> {
    let message: TransportLogMsg = message
        .to_transport(Compression::LZ4)
        .map_err(|error| Status::internal(format!("encode Rerun message: {error}")))?
        .into();
    Ok(ReadMessagesResponse {
        log_msg: Some(message),
    })
}

#[cfg(test)]
mod tests {
    use re_build_info::CrateVersion;
    use re_log_encoding::{
        CachingApplicationIdInjector, EncodingOptions, ToApplication as _, rrd::Encoder,
    };
    use re_log_types::LogMsg;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn live_messages_retain_reruns_scoped_store_identity() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("source-recording")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        for message in storage.take() {
            encoder.append(&message).unwrap();
        }
        encoder.finish().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("active.rrd");
        std::fs::write(&path, encoder.into_inner().unwrap()).unwrap();

        let playback_store_id = StoreId::recording("playback-dataset", "source-recording");
        let mut receiver =
            stream_live_messages(path, Duration::from_secs(60), playback_store_id.clone());
        let messages = [
            receiver.recv().await.unwrap(),
            receiver.recv().await.unwrap(),
        ]
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, LogMsg::SetStoreInfo(_)))
        );
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
        );
        assert!(
            messages
                .iter()
                .all(|message| message.store_id() == &playback_store_id)
        );

        let mut injector = CachingApplicationIdInjector::default();
        let decoded = messages
            .into_iter()
            .map(encode_response)
            .map(Result::unwrap)
            .map(|response| {
                response
                    .log_msg
                    .unwrap()
                    .to_application((&mut injector, None))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(
            decoded
                .iter()
                .all(|message| message.store_id() == &playback_store_id)
        );
    }

    #[test]
    fn rollover_advances_only_to_a_strictly_newer_writing_segment() {
        fn segment(ordinal: i64) -> PlaybackLiveSegmentPlan {
            PlaybackLiveSegmentPlan {
                descriptor: crate::contract::PlaybackLiveSegment {
                    segment_id: uuid::Uuid::now_v7().to_string(),
                    ordinal,
                    current_byte_len: 0,
                    history_seconds: 1,
                    video_preroll_seconds: 2,
                    transport: crate::contract::PlaybackLiveTransport::RerunMessageProxyGrpc,
                },
                path: std::path::PathBuf::from(format!("segment-{ordinal}.rrd")),
            }
        }

        assert_eq!(
            next_live_segment(Some(segment(7)), None)
                .unwrap()
                .descriptor
                .ordinal,
            7
        );
        assert!(next_live_segment(Some(segment(7)), Some(7)).is_none());
        assert!(next_live_segment(Some(segment(6)), Some(7)).is_none());
        assert_eq!(
            next_live_segment(Some(segment(8)), Some(7))
                .unwrap()
                .descriptor
                .ordinal,
            8
        );
    }
}
