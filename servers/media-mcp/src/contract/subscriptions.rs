use super::{
    MediaPredictionIndexUri, MediaPredictionUri, MediaResource, MediaTaskUsageUri,
    MediaUsageIndexUri,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaSubscriptionResource {
    Prediction(MediaPredictionUri),
    PredictionsIndex(MediaPredictionIndexUri),
    UsageIndex(MediaUsageIndexUri),
    TaskUsage(MediaTaskUsageUri),
}
impl MediaSubscriptionResource {
    pub fn parse(uri: &str) -> Option<Self> {
        match MediaResource::parse(uri).ok()? {
            MediaResource::Prediction(uri) => Some(Self::Prediction(uri)),
            MediaResource::Predictions(uri) => Some(Self::PredictionsIndex(uri)),
            MediaResource::TaskUsage(uri) => Some(Self::TaskUsage(uri)),
            MediaResource::Usage(uri) => Some(Self::UsageIndex(uri)),
            _ => None,
        }
    }
}
