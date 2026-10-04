//! Typed Recording ingest addresses used for immutable policy audit attribution.
use super::{RecordingIngestStreamId, RecordingProducerId, RecordingTarget};
use veoveo_gateway_contract::TargetAuditResource;
use veoveo_types::{ExtensionError, IdentifierError, ResourceAddress};

#[derive(Clone, Debug, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(error=IdentifierError,route_error=|_| IdentifierError::new("recording-ingest URI", "invalid Recording ingest address"))]
pub enum RecordingIngestUri {
    #[resource(template = "recording-ingest://producers/{producer}")]
    Producer {
        #[resource(variable="producer", error=|error| error)]
        producer: RecordingProducerId,
    },
    #[resource(template = "recording-ingest://producers/{producer}/streams/{stream_id}")]
    Stream {
        #[resource(variable="producer", error=|error| error)]
        producer: RecordingProducerId,
        #[resource(variable="stream_id", error=|error| error)]
        stream_id: RecordingIngestStreamId,
    },
}
pub fn target_audit_resource(
    target: &RecordingTarget,
) -> Result<TargetAuditResource, ExtensionError> {
    let address = match target {
        RecordingTarget::RecordingProducer { producer } => RecordingIngestUri::Producer {
            producer: producer.clone(),
        },
        RecordingTarget::RecordingStream {
            producer,
            stream_id,
        } => RecordingIngestUri::Stream {
            producer: producer.clone(),
            stream_id: stream_id.clone(),
        },
    };
    Ok(TargetAuditResource {
        server: "recording-hub"
            .parse()
            .map_err(|_| ExtensionError::new("invalid fixed audit server"))?,
        uri: address
            .to_uri()
            .map_err(|error| ExtensionError::new(error.to_string()))?,
    })
}
