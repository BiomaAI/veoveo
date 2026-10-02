//! Protocol observations for members whose identity is assigned on creation.
use super::{
    CollectionDescriptor, Context, Observation, ResourceUri, Result, ensure, knowledge, notified,
    observe, read, require_same,
};
use crate::{knowledge_probes::KnowledgeCreateDriver, runner::Client};

pub(super) async fn check(
    client: &Client,
    descriptor: &CollectionDescriptor,
    enumeration: &ResourceUri,
    baseline: Vec<ResourceUri>,
    driver: &dyn KnowledgeCreateDriver,
) -> Result<()> {
    let first = create(client, enumeration, driver).await?;
    ensure!(
        !baseline.contains(&first),
        "creation returned an existing member"
    );
    let before = admitted(client, descriptor, &first).await?;
    driver.restart().await.context("owner restart failed")?;
    require_same(&before, &admitted(client, descriptor, &first).await?)?;

    let visible = super::super::enumerate(client, descriptor).await?;
    let second = create(client, enumeration, driver).await?;
    ensure!(
        first != second && !baseline.contains(&second) && !visible.contains(&second),
        "creation after restart returned an existing member"
    );
    admitted(client, descriptor, &second).await?;
    require_same(&before, &admitted(client, descriptor, &first).await?)?;
    Ok(())
}

async fn create(
    client: &Client,
    enumeration: &ResourceUri,
    driver: &dyn KnowledgeCreateDriver,
) -> Result<ResourceUri> {
    notified(client, &[enumeration], async {
        driver.create().await.context("owner creation failed")
    })
    .await
}

async fn admitted(
    client: &Client,
    descriptor: &CollectionDescriptor,
    member: &ResourceUri,
) -> Result<Observation> {
    ensure!(
        super::super::enumerate(client, descriptor)
            .await?
            .contains(member),
        "created member is not enumerated"
    );
    let observation = observe(client, descriptor, member).await?;
    let result = read(client, member, Some(observation.revision())).await?;
    let unchanged =
        knowledge::client::validate_read(&result, member, Some(observation.revision()))?
            .context("created member conditional read omitted observation")?;
    unchanged.validate_collection(descriptor)?;
    ensure!(
        unchanged.not_modified(),
        "matching revision returned content"
    );
    require_same(&observation, &unchanged)?;
    Ok(observation)
}
