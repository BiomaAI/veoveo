use super::{MediaPredictionIndexUri, MediaPredictionUri, MediaTaskUsageUri, MediaUsageIndexUri};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaSubscriptionResource {
    Prediction(MediaPredictionUri),
    PredictionsIndex(MediaPredictionIndexUri),
    UsageIndex(MediaUsageIndexUri),
    TaskUsage(MediaTaskUsageUri),
}
impl MediaSubscriptionResource {
    pub fn parse(uri: &str) -> Option<Self> {
        MediaPredictionUri::parse(uri)
            .map(Self::Prediction)
            .ok()
            .or_else(|| {
                MediaPredictionIndexUri::parse(uri)
                    .map(Self::PredictionsIndex)
                    .ok()
            })
            .or_else(|| MediaTaskUsageUri::parse(uri).map(Self::TaskUsage).ok())
            .or_else(|| MediaUsageIndexUri::parse(uri).map(Self::UsageIndex).ok())
    }
}
