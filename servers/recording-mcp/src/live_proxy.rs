//! Recording-scoped Rerun MessageProxy service.
//!
//! Rerun WebViewer consumes live SDK messages through its native gRPC-Web
//! `ReadMessages` stream. Authorization and recording selection happen before
//! this service is constructed, so the empty upstream request cannot escape
//! its one governed recording segment.

use std::{path::PathBuf, pin::Pin, time::Duration};

use futures::{Stream, StreamExt as _};
use re_log_encoding::{ToTransport as _, rrd::Compression};
use re_log_types::StoreId;
use re_protos::sdk_comms::v1alpha1::{
    ReadMessagesRequest, ReadMessagesResponse, ReadTablesRequest, ReadTablesResponse,
    WriteMessagesRequest, WriteMessagesResponse, WriteTableRequest, WriteTableResponse,
    message_proxy_service_server::{MessageProxyService, MessageProxyServiceServer},
};
use tonic::{Request, Response as TonicResponse, Status};

use crate::live_playback::stream_live_messages;

pub const READ_MESSAGES_RPC_PATH: &str =
    "/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages";

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct AuthorizedLiveMessageProxy {
    segment_path: PathBuf,
    history: Duration,
    playback_store_id: StoreId,
}

impl AuthorizedLiveMessageProxy {
    pub fn new(segment_path: PathBuf, history: Duration, playback_store_id: StoreId) -> Self {
        Self {
            segment_path,
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
        let receiver = stream_live_messages(
            self.segment_path.clone(),
            self.history,
            self.playback_store_id.clone(),
        );
        let stream = futures::stream::unfold(receiver, |mut receiver| async move {
            let message = receiver.recv().await?;
            Some((message, receiver))
        })
        .map(|message| {
            let message = message.map_err(|error| Status::unavailable(error.to_string()))?;
            let message = message
                .to_transport(Compression::LZ4)
                .map_err(|error| Status::internal(format!("encode Rerun message: {error}")))?;
            Ok(ReadMessagesResponse {
                log_msg: Some(message.into()),
            })
        });
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

#[cfg(test)]
mod tests {
    use futures::StreamExt as _;
    use re_build_info::CrateVersion;
    use re_log_encoding::{
        CachingApplicationIdInjector, EncodingOptions, ToApplication as _, rrd::Encoder,
    };
    use re_log_types::LogMsg;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn read_messages_uses_reruns_native_transport_and_scoped_store_identity() {
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
        let proxy = AuthorizedLiveMessageProxy::new(
            path,
            Duration::from_secs(60),
            playback_store_id.clone(),
        );
        let response = proxy
            .read_messages(Request::new(ReadMessagesRequest {}))
            .await
            .unwrap();
        let responses = response
            .into_inner()
            .take(2)
            .collect::<Vec<Result<ReadMessagesResponse, Status>>>()
            .await;
        assert_eq!(responses.len(), 2);

        let mut injector = CachingApplicationIdInjector::default();
        let messages = responses
            .into_iter()
            .map(|response| {
                response
                    .unwrap()
                    .log_msg
                    .unwrap()
                    .to_application((&mut injector, None))
                    .unwrap()
            })
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
    }
}
