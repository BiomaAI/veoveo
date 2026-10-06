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
    /// The subscribable form of a Media address. Catalogs, models, documents,
    /// generation results and artifacts do not change and return `None`.
    pub fn from_resource(resource: MediaResource) -> Option<Self> {
        match resource {
            MediaResource::Prediction(uri) => Some(Self::Prediction(uri)),
            MediaResource::Predictions(uri) => Some(Self::PredictionsIndex(uri)),
            MediaResource::TaskUsage(uri) => Some(Self::TaskUsage(uri)),
            MediaResource::Usage(uri) => Some(Self::UsageIndex(uri)),
            MediaResource::Docs
            | MediaResource::Document(_)
            | MediaResource::Contract
            | MediaResource::StudioApp
            | MediaResource::Models(_)
            | MediaResource::Model(_)
            | MediaResource::Generation(_)
            | MediaResource::Artifact(_) => None,
        }
    }
}
