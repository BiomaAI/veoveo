//! Native Git and Helm checks for publication without rebuilding images.
use super::*;
use veoveo_deploy_contract::{DeploymentLock, components::*};
use veoveo_extension_contract::SourceRevision;

#[test]
fn chart_and_configuration_publication_preserves_unrequested_owners_and_images() {
    independent_sources(Some(verify_publication));
}

fn verify_publication(
    original: &LoadedProfile,
    base: &DeploymentLock,
    roots: &BTreeMap<String, PathBuf>,
) {
    let mut lock = base.clone();
    let mut definition = original.definition.clone();
    for (step, (chart, configuration)) in [(true, false), (false, true), (true, true)]
        .into_iter()
        .enumerate()
    {
        for (owner, other) in [("platform", "extension"), ("extension", "platform")] {
            let setting = format!("{step}-{owner}");
            if configuration {
                for source in &mut definition.sources {
                    for release in &mut source.releases {
                        for path in &release.installation_values {
                            fs::remove_file(original.repository.join(path)).unwrap();
                        }
                        let path = format!("{}-{setting}.yaml", source.name);
                        fs::write(
                            original.repository.join(&path),
                            format!("fixtureSetting: {setting}\n"),
                        )
                        .unwrap();
                        release.installation_values = vec![path.into()];
                    }
                }
                fs::write(
                    &original.path,
                    serde_json::to_vec_pretty(&definition).unwrap(),
                )
                .unwrap();
                commit(&original.repository, "new per-component values paths");
            }
            let profile = LoadedProfile::load(&original.path, &original.repository).unwrap();
            let requested = BTreeSet::from([ComponentId::try_from(owner.to_owned()).unwrap()]);
            let mut updates = crate::ComponentUpdates {
                refresh_configuration: configuration,
                ..Default::default()
            };
            if chart {
                let path = roots[owner].join("chart/templates/workload.yaml");
                let mut template = fs::read_to_string(&path).unwrap();
                template = template.replacen(
                    "  annotations:\n",
                    &format!("  annotations:\n    test.example/chart-{step}: {owner}\n"),
                    1,
                );
                fs::write(&path, template).unwrap();
                let revision = commit(&roots[owner], "new source chart");
                updates
                    .source_revisions
                    .insert(owner.into(), SourceRevision::new(revision).unwrap());
            }
            let hidden = roots[other].with_extension("unavailable");
            fs::rename(&roots[other], &hidden).unwrap();
            let updated = crate::update_components(&profile, &lock, &requested, &updates).unwrap();
            fs::rename(&hidden, &roots[other]).unwrap();
            for previous in &lock.components {
                let current = updated
                    .components
                    .iter()
                    .find(|component| component.declaration.id == previous.declaration.id)
                    .unwrap();
                if previous.declaration.id.as_str() != owner {
                    assert_eq!(
                        current, previous,
                        "unrequested owners and dependencies stay byte-for-byte equivalent"
                    );
                    continue;
                }
                assert_ne!(
                    current.units[0].content_digest,
                    previous.units[0].content_digest
                );
                assert_eq!(image_inputs(current), image_inputs(previous));
                if chart {
                    assert_eq!(
                        current.declaration.source.revision,
                        updates.source_revisions[owner]
                    );
                } else {
                    assert_eq!(current.declaration.source, previous.declaration.source);
                }
                if configuration {
                    assert_eq!(
                        current.declaration.configuration.source.revision.as_str(),
                        updated.profile_revision
                    );
                } else {
                    assert_eq!(
                        current.declaration.configuration,
                        previous.declaration.configuration
                    );
                }
            }
            for source in &updated.sources {
                let previous = lock
                    .sources
                    .iter()
                    .find(|previous| previous.name == source.name)
                    .unwrap();
                assert_eq!(
                    source.images, previous.images,
                    "publication must not rebuild or relabel images"
                );
                if source.name == other {
                    assert_eq!(source, previous);
                }
            }
            let all = updated
                .components
                .iter()
                .map(|component| component.declaration.id.clone())
                .collect();
            let snapshots =
                crate::sources::resolve_locked_sources(&profile, &updated, &all).unwrap();
            let exact_roots = snapshots
                .iter()
                .map(|(identity, snapshot)| (identity.clone(), snapshot.repository.clone()))
                .collect();
            let rendered =
                compile_locked_components(&profile, &updated, &exact_roots, &all).unwrap();
            assert_eq!(
                rendered
                    .iter()
                    .map(|component| component.locked.clone())
                    .collect::<Vec<_>>(),
                updated.components
            );
            if configuration {
                let current = rendered
                    .iter()
                    .find(|component| component.locked.declaration.id.as_str() == owner)
                    .unwrap();
                assert_eq!(
                    current.units[0].objects[0]["metadata"]["annotations"]["test.example/setting"],
                    setting
                );
            }
            assert_eq!(
                crate::update_components(&profile, &updated, &requested, &updates).unwrap(),
                updated,
                "repeated publication is deterministic"
            );
            lock = updated;
        }
    }
    let profile = LoadedProfile::load(&original.path, &original.repository).unwrap();
    let requested = BTreeSet::from([ComponentId::try_from("platform".to_owned()).unwrap()]);
    assert!(
        crate::update_components(
            &profile,
            &lock,
            &requested,
            &crate::ComponentUpdates::default()
        )
        .is_err()
    );
    let mut updates = crate::ComponentUpdates {
        source_revisions: BTreeMap::from([(
            "extension".into(),
            SourceRevision::new(git(&roots["extension"], &["rev-parse", "HEAD"])).unwrap(),
        )]),
        ..Default::default()
    };
    assert!(
        crate::update_components(&profile, &lock, &requested, &updates)
            .unwrap_err()
            .to_string()
            .contains("not owned by a requested")
    );
    // A new selected chart cannot capture an unselected release's Deployment.
    let path = roots["platform"].join("chart/templates/workload.yaml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("name: {{ .Release.Name }}", "name: extension");
    fs::write(&path, text).unwrap();
    updates.source_revisions = BTreeMap::from([(
        "platform".into(),
        SourceRevision::new(commit(&roots["platform"], "overlapping chart")).unwrap(),
    )]);
    assert!(crate::update_components(&profile, &lock, &requested, &updates).is_err());
}

fn image_inputs(component: &LockedComponent) -> BTreeSet<ComponentInput> {
    component
        .declaration
        .inputs
        .iter()
        .filter(|input| matches!(input, ComponentInput::Image { .. }))
        .cloned()
        .collect()
}
