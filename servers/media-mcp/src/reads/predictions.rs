use super::{MediaReads, bind_owner};
use crate::{
    contract::{
        GenerationPredictionSummary, MEDIA_PREDICTION_PAGE_SIZE, MediaPredictionCursor,
        MediaPredictionId, MediaPredictionPage, MediaPredictionUri,
    },
    provider::Prediction,
};
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::TaskOwner;

impl MediaReads<'_> {
    pub async fn predictions(
        &self,
        owner: &TaskOwner,
        cursor: Option<&MediaPredictionCursor>,
    ) -> anyhow::Result<MediaPredictionPage> {
        let mut response = bind_owner(
            self.tasks
                .platform_store()
                .client()
                .query(if cursor.is_some() {
                    include_str!("queries/prediction_page_after.surql")
                } else {
                    include_str!("queries/prediction_page.surql")
                }),
            owner,
        )?
        .bind(("after", cursor.map(|cursor| cursor.after().to_string())))
        .bind(("limit", MEDIA_PREDICTION_PAGE_SIZE + 1))
        .await?
        .check()?;
        let mut ids: Vec<String> = response.take(0)?;
        let has_more = ids.len() > MEDIA_PREDICTION_PAGE_SIZE;
        ids.truncate(MEDIA_PREDICTION_PAGE_SIZE);
        let ids = ids
            .into_iter()
            .map(MediaPredictionId::new)
            .collect::<Result<Vec<_>, _>>()?;
        let next = has_more.then(|| ids.last().expect("overfull page is nonempty").clone());
        Ok(MediaPredictionPage::from_ids(ids, next)?)
    }

    /// Exact resource reads and subscription admission use the same scoped selection.
    pub async fn prediction(
        &self,
        owner: &TaskOwner,
        uri: &MediaPredictionUri,
    ) -> anyhow::Result<Option<GenerationPredictionSummary>> {
        let mut response = bind_owner(
            self.tasks
                .platform_store()
                .client()
                .query(include_str!("queries/read_prediction.surql")),
            owner,
        )?
        .bind(("prediction", uri.id().to_string()))
        .await?
        .check()?;
        let rows: Vec<OpenObject> = response.take(0)?;
        rows.into_iter()
            .next()
            .map(|payload| {
                let prediction: Prediction = serde_json::from_value(serde_json::Value::Object(
                    payload.into_map().into_iter().collect(),
                ))?;
                Ok(prediction.summary())
            })
            .transpose()
    }
}
