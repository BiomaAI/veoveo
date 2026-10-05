use super::{MediaReads, bind_owner};
use crate::{
    contract::{
        GenerationPredictionSummary, MEDIA_PREDICTION_PAGE_SIZE, MediaPredictionCursor,
        MediaPredictionId, MediaPredictionPage, MediaPredictionUri,
    },
    storage::PredictionRecord,
};
use surrealdb::types::SurrealValue;

#[derive(SurrealValue)]
struct PredictionRow {
    prediction: String,
    payload: PredictionRecord,
}
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
        let rows: Vec<PredictionRow> = response.take(0)?;
        rows.into_iter()
            .next()
            .map(|row| {
                let prediction = row.payload.0;
                let selected = MediaPredictionId::new(row.prediction)?;
                anyhow::ensure!(
                    prediction.id == selected && &prediction.id == uri.id(),
                    "stored prediction disagrees with its selected identity"
                );
                Ok(prediction.summary())
            })
            .transpose()
    }
}
