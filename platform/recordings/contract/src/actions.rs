//! Owner-declared actions admitted by the gateway policy registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum RecordingAction {
    #[vocabulary(rename = "recording_stream_open")]
    StreamOpen,
    #[vocabulary(rename = "recording_stream_status")]
    StreamStatus,
    #[vocabulary(rename = "recording_batch_append")]
    BatchAppend,
    #[vocabulary(rename = "recording_blueprint_publish")]
    BlueprintPublish,
    #[vocabulary(rename = "recording_stream_finish")]
    StreamFinish,
    #[vocabulary(rename = "recording_layer_publish")]
    LayerPublish,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_wire_spellings_remain_closed() {
        let expected = [
            "recording_stream_open",
            "recording_stream_status",
            "recording_batch_append",
            "recording_blueprint_publish",
            "recording_stream_finish",
            "recording_layer_publish",
        ];
        assert_eq!(
            RecordingAction::ALL
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for wire in expected {
            let action = wire.parse::<RecordingAction>().unwrap();
            assert_eq!(serde_json::to_value(action).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<RecordingAction>(serde_json::json!(wire)).unwrap(),
                action
            );
            assert!(format!("{wire}_alias").parse::<RecordingAction>().is_err());
        }
    }
}
