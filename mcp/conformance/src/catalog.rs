//! Complete catalog reads with traversal bounds for conformance and CLI consumers.
mod selected;
pub(crate) use selected::SourceCatalogCoverage;

use anyhow::{Context, Result, bail};
use rmcp::{RoleClient, model::*, service::Peer};
use std::{collections::BTreeSet, future::Future, time::Duration};

const MAX_PAGES: usize = 1_024;
const MAX_ITEMS: usize = 16_384;
const DEADLINE: Duration = Duration::from_secs(30);

pub(crate) async fn before_naming_deadline<T>(
    started: std::time::Instant,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    let remaining = crate::naming::NAMING_DEADLINE
        .checked_sub(started.elapsed())
        .ok_or_else(|| anyhow::anyhow!("aggregate naming discovery deadline expired"))?;
    tokio::time::timeout(remaining, operation)
        .await
        .context("aggregate naming discovery deadline expired")?
}

async fn collect<T, F, Fut>(operation: &str, request: F) -> Result<Vec<T>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, Option<String>)>>,
{
    collect_admitted(operation, request, Ok).await
}

async fn collect_admitted<T, P, F, Fut, A>(
    operation: &str,
    mut request: F,
    mut admit: A,
) -> Result<Vec<T>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<P>>,
    A: FnMut(P) -> Result<(Vec<T>, Option<String>)>,
{
    tokio::time::timeout(DEADLINE, async {
        let mut items = Vec::new();
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        for _ in 0..MAX_PAGES {
            let (page, next) = admit(request(cursor).await?)?;
            if items.len() + page.len() > MAX_ITEMS {
                bail!("{operation} exceeded {MAX_ITEMS} catalog items");
            }
            items.extend(page);
            let Some(next) = next else { return Ok(items) };
            if !seen.insert(next.clone()) {
                bail!("{operation} repeated a continuation cursor");
            }
            cursor = Some(next);
        }
        bail!("{operation} exceeded {MAX_PAGES} catalog pages")
    })
    .await
    .with_context(|| format!("{operation} exceeded its 30-second catalog deadline"))?
}

// A degraded page cannot establish complete discovery, even when it carries
// useful items. Admit the shared producer's failure DTO before refusing it.
fn admit_page_metadata(meta: Option<&MetaObject>, operation: &str) -> Result<()> {
    if let Some(value) = meta
        .and_then(|meta| meta.get(veoveo_gateway_contract::GATEWAY_DISCOVERY_DEGRADATION_META_KEY))
    {
        let degradation: veoveo_gateway_contract::GatewayDiscoveryDegradation =
            serde_json::from_value(value.clone())
                .with_context(|| format!("{operation} has malformed gateway discovery metadata"))?;
        if !degradation.is_empty() {
            bail!(
                "{operation} is incomplete: {} admitted gateway discovery failures",
                degradation.failures.len()
            );
        }
    }
    Ok(())
}

pub async fn tools(client: &Peer<RoleClient>) -> Result<Vec<Tool>> {
    collect("tools/list", |cursor| async move {
        let page = client
            .list_tools(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        admit_page_metadata(page.meta.as_ref(), "tools/list")?;
        Ok((page.tools, page.next_cursor))
    })
    .await
}
pub async fn resources(client: &Peer<RoleClient>) -> Result<Vec<Resource>> {
    collect("resources/list", |cursor| async move {
        let page = client
            .list_resources(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        admit_page_metadata(page.meta.as_ref(), "resources/list")?;
        Ok((page.resources, page.next_cursor))
    })
    .await
}
pub async fn templates(client: &Peer<RoleClient>) -> Result<Vec<ResourceTemplate>> {
    collect("resources/templates/list", |cursor| async move {
        let page = client
            .list_resource_templates(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        admit_page_metadata(page.meta.as_ref(), "resource_templates/list")?;
        Ok((page.resource_templates, page.next_cursor))
    })
    .await
}
pub async fn prompts(client: &Peer<RoleClient>) -> Result<Vec<Prompt>> {
    collect("prompts/list", |cursor| async move {
        let page = client
            .list_prompts(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        admit_page_metadata(page.meta.as_ref(), "prompts/list")?;
        Ok((page.prompts, page.next_cursor))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn follows_empty_pages_and_rejects_cycles_and_excessive_catalogs() {
        let values = collect("fixture", |cursor| {
            std::future::ready(Ok(match cursor.as_deref() {
                None => (vec![1], Some("second".into())),
                Some("second") => (vec![], Some("last".into())),
                Some("last") => (vec![2], None),
                _ => unreachable!(),
            }))
        })
        .await
        .unwrap();
        assert_eq!(values, vec![1, 2]);
        let repeated = collect::<(), _, _>("fixture", |_| {
            std::future::ready(Ok((vec![], Some("same".into()))))
        })
        .await
        .unwrap_err();
        assert!(repeated.to_string().contains("repeated"));
        let oversized = collect("fixture", |_| {
            std::future::ready(Ok((vec![(); MAX_ITEMS + 1], None)))
        })
        .await
        .unwrap_err();
        assert!(oversized.to_string().contains("catalog items"));
        let mut sequence = 0;
        let endless = collect::<(), _, _>("fixture", |_| {
            sequence += 1;
            std::future::ready(Ok((vec![], Some(sequence.to_string()))))
        })
        .await
        .unwrap_err();
        assert!(endless.to_string().contains("catalog pages"));
    }
}
