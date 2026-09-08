use std::{collections::BTreeMap, path::Path};

use super::{atomic_unit_content_digest, atomic_unit_digest, types::*};
use crate::validate_name;
use anyhow::{Context, Result, ensure};

/// Seals complete renderings into a component lock. Cross-owner checks follow when
/// the complete installation catalog is passed to [`validate_component_catalog`].
pub fn lock_component(
    declaration: DeploymentComponent,
    prepared: Vec<PreparedAtomicUnit>,
) -> Result<LockedComponent> {
    validate_declaration(&declaration)?;
    let units = prepared
        .into_iter()
        .map(|unit| {
            Ok(LockedAtomicUnit {
                digest: atomic_unit_digest(&declaration, &unit)?,
                content_digest: atomic_unit_content_digest(&declaration, &unit)?,
                target: unit.target,
                inputs: unit.inputs,
                objects: unit.objects,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let component = LockedComponent { declaration, units };
    validate_locked_component(&component)?;
    Ok(component)
}

/// Validates every owner's declaration and locked inventory without rendering an
/// unselected component or contacting a deployment tool.
pub fn validate_component_catalog(catalog: &[LockedComponent]) -> Result<()> {
    ensure!(!catalog.is_empty(), "component catalog cannot be empty");
    let mut ids = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut targets = BTreeMap::new();
    let mut objects = BTreeMap::new();
    let mut input_contents = BTreeMap::new();
    let mut image_owners = BTreeMap::new();
    for component in catalog {
        let declaration = &component.declaration;
        validate_locked_component(component)
            .with_context(|| format!("validating component {}", declaration.id))?;
        ensure!(
            ids.insert(&declaration.id, component).is_none(),
            "duplicate component {}",
            declaration.id
        );
        for source in
            std::iter::once(&declaration.source).chain(declaration.inputs.iter().map(input_source))
        {
            if let Some(previous) = sources.insert(&source.name, &source.repository) {
                ensure!(
                    previous == &source.repository,
                    "source {} has conflicting identities",
                    source.name
                );
            }
        }
        for target in &declaration.targets {
            if let Some(owner) = targets.insert(target, &declaration.id) {
                anyhow::bail!(
                    "atomic target {target:?} is owned by both {owner} and {}",
                    declaration.id
                );
            }
        }
        for input in &declaration.inputs {
            if let Some(previous) = input_contents.insert(input_identity(input), input) {
                ensure!(
                    previous == input,
                    "immutable input has conflicting locked contents"
                );
            }
            if let ComponentInput::Image {
                source,
                target,
                repository,
                ..
            } = input
            {
                let owner = (&source.name, target);
                if let Some(previous) = image_owners.insert(repository, owner) {
                    ensure!(
                        previous == owner,
                        "image repository has multiple source owners"
                    );
                }
            }
        }
        // Reserved but currently absent identities also belong to exactly one owner.
        for object in &declaration.permitted_objects {
            if let Some(owner) = objects.insert(object, &declaration.id) {
                anyhow::bail!(
                    "object {object:?} is owned by both {owner} and {}",
                    declaration.id
                );
            }
        }
    }
    for component in catalog {
        for dependency in &component.declaration.dependencies {
            ensure!(
                ids.contains_key(dependency),
                "component {} requires missing dependency {dependency}",
                component.declaration.id
            );
        }
    }
    dependency_order(catalog)?;
    Ok(())
}

pub(super) fn dependency_order(catalog: &[LockedComponent]) -> Result<Vec<ComponentId>> {
    let mut remaining = catalog
        .iter()
        .map(|item| {
            (
                item.declaration.id.clone(),
                item.declaration.dependencies.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut order = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .find(|(_, dependencies)| dependencies.is_empty())
            .map(|(id, _)| id.clone())
            .context("component dependency graph contains a cycle")?;
        remaining.remove(&next);
        for dependencies in remaining.values_mut() {
            dependencies.remove(&next);
        }
        order.push(next);
    }
    Ok(order)
}

fn validate_locked_component(component: &LockedComponent) -> Result<()> {
    let declaration = &component.declaration;
    validate_declaration(declaration)?;
    let mut targets = std::collections::BTreeSet::new();
    let mut objects = std::collections::BTreeSet::new();
    let mut inputs = std::collections::BTreeSet::new();
    for unit in &component.units {
        ensure!(
            targets.insert(unit.target.clone()),
            "duplicate locked atomic target"
        );
        let prepared = PreparedAtomicUnit {
            component: declaration.id.clone(),
            source: declaration.source.clone(),
            target: unit.target.clone(),
            inputs: unit.inputs.clone(),
            objects: unit.objects.clone(),
            tool_scope: AtomicToolScope::Exact,
        };
        ensure!(
            atomic_unit_digest(declaration, &prepared)? == unit.digest,
            "locked atomic unit digest differs from its complete contents"
        );
        ensure!(
            atomic_unit_content_digest(declaration, &prepared)? == unit.content_digest,
            "locked atomic unit content digest differs from its complete contents"
        );
        for object in &unit.objects {
            ensure!(
                objects.insert(&object.identity),
                "object occurs in multiple atomic targets"
            );
        }
        inputs.extend(unit.inputs.iter().cloned());
    }
    ensure!(
        targets == declaration.targets,
        "locked atomic targets do not match declaration"
    );
    ensure!(
        inputs == declaration.inputs,
        "locked units do not consume the declared input closure"
    );
    Ok(())
}

pub(super) fn validate_prepared(
    component: &DeploymentComponent,
    unit: &PreparedAtomicUnit,
) -> Result<()> {
    ensure!(
        unit.component == component.id,
        "prepared component differs from locked owner"
    );
    ensure!(
        unit.source == component.source,
        "prepared source differs from locked owner"
    );
    ensure!(
        unit.tool_scope == AtomicToolScope::Exact,
        "deployment tool cannot target the atomic unit exactly"
    );
    ensure!(
        component.targets.contains(&unit.target),
        "atomic target is outside component declaration"
    );
    ensure!(
        unit.inputs.is_subset(&component.inputs),
        "input is outside component closure"
    );
    ensure!(
        !unit.objects.is_empty(),
        "atomic target has no explicit objects"
    );
    let mut identities = std::collections::BTreeSet::new();
    for object in &unit.objects {
        validate_owned_object(component, &object.identity)?;
        ensure!(
            identities.insert(&object.identity),
            "duplicate rendered object identity"
        );
    }
    Ok(())
}

pub(super) fn validate_owned_object(
    component: &DeploymentComponent,
    object: &ObjectIdentity,
) -> Result<()> {
    validate_object(object)?;
    ensure!(
        component.permitted_objects.contains(object),
        "object {object:?} is outside component {} ownership",
        component.id
    );
    if let Some(namespace) = &object.namespace {
        ensure!(
            component.namespaces.contains(namespace),
            "object uses undeclared namespace"
        );
    }
    Ok(())
}

pub(super) fn validate_declaration(component: &DeploymentComponent) -> Result<()> {
    validate_source(&component.source)?;
    ensure!(
        !component.targets.is_empty(),
        "component must own an atomic target"
    );
    ensure!(
        !component.permitted_objects.is_empty(),
        "component must declare object ownership"
    );
    ensure!(
        (component.role == ComponentRole::Extension) == component.extension_release.is_some(),
        "exact extension release identity is required only for extension components"
    );
    for namespace in &component.namespaces {
        validate_name("namespace", namespace)?;
    }
    for target in &component.targets {
        match target {
            AtomicTarget::HelmRelease { namespace, name } => {
                validate_name("Helm release", name)?;
                ensure!(name.len() <= 53, "Helm release name exceeds 53 characters");
                ensure!(
                    component.namespaces.contains(namespace),
                    "Helm release uses undeclared namespace"
                );
            }
            AtomicTarget::ManifestSet { name } => validate_name("manifest set", name)?,
        }
    }
    for object in &component.permitted_objects {
        validate_owned_object(component, object)?;
    }
    let mut inputs = std::collections::BTreeSet::new();
    for input in &component.inputs {
        validate_source(input_source(input))?;
        match input {
            ComponentInput::File { path, .. } => {
                ensure!(
                    !path.is_empty()
                        && !path.contains('\\')
                        && Path::new(path)
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_)))
                        && path
                            .split('/')
                            .all(|part| !part.is_empty() && part != "." && part != ".."),
                    "component file input must be a canonical source-relative path"
                );
            }
            ComponentInput::Image {
                target, repository, ..
            } => {
                validate_name("image target", target)?;
                ensure!(
                    !repository.is_empty()
                        && !repository.contains('@')
                        && !repository.chars().any(char::is_whitespace)
                        && !repository
                            .rsplit('/')
                            .next()
                            .unwrap_or_default()
                            .contains(':'),
                    "image input repository must not contain a tag or digest"
                );
            }
            ComponentInput::Chart { coordinate, .. } => {
                ensure!(
                    (coordinate.starts_with("source://") || coordinate.starts_with("oci://"))
                        && !coordinate.ends_with(":latest")
                        && !coordinate.chars().any(char::is_whitespace),
                    "component chart must use a locked source or OCI coordinate"
                );
                let url = url::Url::parse(coordinate).context("invalid chart coordinate")?;
                ensure!(
                    url.host_str().is_some()
                        && url.username().is_empty()
                        && url.password().is_none()
                        && url.query().is_none()
                        && url.fragment().is_none(),
                    "chart coordinate must not contain credentials, query or fragment"
                );
            }
        }
        ensure!(
            inputs.insert(input_identity(input)),
            "input identity has multiple locked contents"
        );
    }
    Ok(())
}

fn validate_source(source: &ComponentSource) -> Result<()> {
    validate_name("source", &source.name)?;
    ensure!(
        !source.repository.is_empty() && !source.repository.chars().any(char::is_whitespace),
        "source repository cannot be empty or contain whitespace"
    );
    // Origin normalization belongs to the source resolver. Never accept credentialed
    // URLs as receipt identities, and do not print the rejected value in diagnostics.
    if let Ok(url) = url::Url::parse(&source.repository) {
        ensure!(
            url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && (url.username().is_empty() || url.scheme() == "ssh"),
            "source repository identity must not contain credentials, query or fragment"
        );
    }
    Ok(())
}

fn input_source(input: &ComponentInput) -> &ComponentSource {
    match input {
        ComponentInput::File { source, .. }
        | ComponentInput::Image { source, .. }
        | ComponentInput::Chart { source, .. } => source,
    }
}

fn input_identity(input: &ComponentInput) -> (&'static str, &ComponentSource, &str) {
    match input {
        ComponentInput::File { source, path, .. } => ("file", source, path),
        ComponentInput::Image { source, target, .. } => ("image", source, target),
        ComponentInput::Chart {
            source, coordinate, ..
        } => ("chart", source, coordinate),
    }
}

pub(super) fn validate_object(object: &ObjectIdentity) -> Result<()> {
    // These are already discovery-resolved identities. API versions are intentionally
    // absent; the same object cannot gain a second owner through another served version.
    ensure!(
        !object.kind.is_empty() && object.kind.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid Kubernetes kind"
    );
    ensure!(
        object
            .group
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-')),
        "invalid Kubernetes API group"
    );
    ensure!(
        !object.name.is_empty()
            && !matches!(object.name.as_str(), "." | "..")
            && !object
                .name
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '%')),
        "object identity must contain an explicit Kubernetes object name"
    );
    ensure!(
        object.kind != "List",
        "Kubernetes lists must be expanded before ownership validation"
    );
    ensure!(
        !object.group.is_empty() || object.kind != "Secret",
        "deployment components cannot own Secrets"
    );
    if let Some(namespace) = &object.namespace {
        validate_name("namespace", namespace)?;
    }
    Ok(())
}
