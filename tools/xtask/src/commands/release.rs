use std::{collections::BTreeMap, ffi::OsString, fs, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use veoveo_deploy_contract::LoadedProfile;

use crate::{
    ReleaseImagesArgs, ReleasePythonSdkArgs,
    commands::{
        builder,
        image::{self, OutputMode, Selection},
        python,
        source::PublicationSource,
    },
    context::RepositoryContext,
};

pub(crate) fn python_sdk(
    repository: &RepositoryContext,
    args: &ReleasePythonSdkArgs,
) -> Result<()> {
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let artifacts = python::build_and_verify(publication.path())?;
    let output_root = if args.output_dir.is_absolute() {
        args.output_dir.clone()
    } else {
        repository.root().join(&args.output_dir)
    };
    let output = output_root.join(publication.revision());
    let published = python::write_release_bundle(&artifacts, publication.revision(), &output)?;

    if let Some(publish_url) = &args.publish_url {
        python::validate_publish_url(publish_url)?;
        if let Some(check_url) = &args.check_url {
            python::validate_index_url(check_url)?;
        }
        let mut publish_args = vec![
            OsString::from("publish"),
            OsString::from("--publish-url"),
            OsString::from(publish_url),
        ];
        if let Some(check_url) = &args.check_url {
            publish_args.push(OsString::from("--check-url"));
            publish_args.push(OsString::from(check_url));
        }
        if args.dry_run {
            publish_args.push(OsString::from("--dry-run"));
        }
        publish_args.extend(published.distributions.iter().map(OsString::from));
        crate::process::status("uv", publish_args, Some(repository.root()))?;
    }

    println!("Python SDK release bundle: {}", output.display());
    Ok(())
}

pub(crate) fn images(repository: &RepositoryContext, args: &ReleaseImagesArgs) -> Result<()> {
    builder::ensure(repository)?;
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let selected_repository = RepositoryContext::discover(publication.path())?;
    let selections;
    let registry;

    if let Some(profile) = &args.profile {
        ensure!(
            args.target.is_none() && args.group.is_none() && args.registry.is_none(),
            "--profile cannot be combined with --target, --group, or --registry"
        );
        let relative = profile_relative_path(repository, profile)?;
        let selected_profile = publication.path().join(relative);
        let profile = LoadedProfile::load(&selected_profile, publication.path())?;
        selections = profile
            .definition
            .image_groups
            .iter()
            .map(|group| Selection::group(group))
            .collect::<Result<Vec<_>>>()?;
        registry = profile.definition.registry.address;
    } else {
        let selection = match (&args.target, &args.group) {
            (Some(target), None) => Selection::target(target)?,
            (None, Some(group)) => Selection::group(group)?,
            _ => bail!("release images requires --profile or exactly one --target/--group"),
        };
        selections = vec![selection];
        registry = args
            .registry
            .clone()
            .context("a direct target or group release requires --registry")?;
    }
    validate_registry(&registry)?;

    let environment = BTreeMap::from([
        ("VEOVEO_REGISTRY".to_owned(), registry.clone()),
        (
            "VEOVEO_IMAGE_TAG".to_owned(),
            publication.revision().to_owned(),
        ),
    ]);
    let phase_count = selections.len();
    for (index, selection) in selections.into_iter().enumerate() {
        println!(
            "Publishing image phase {}/{}: {}",
            index + 1,
            phase_count,
            selection.name
        );
        let prepared = image::prepare(&selected_repository, selection, &environment)?;
        let evidence = image::evidence_run(repository, &prepared.plan, "release")?;
        image::execute(
            &selected_repository,
            &prepared,
            &environment,
            OutputMode::Push,
            &evidence,
        )?;
        println!("Release evidence: {}", evidence.directory().display());
    }
    println!(
        "Published immutable revision {} to {}",
        publication.revision(),
        registry
    );
    Ok(())
}

fn profile_relative_path(repository: &RepositoryContext, profile: &PathBuf) -> Result<PathBuf> {
    let candidate = if profile.is_absolute() {
        profile.clone()
    } else {
        repository.root().join(profile)
    };
    let candidate = fs::canonicalize(&candidate)
        .with_context(|| format!("resolving deployment profile {}", candidate.display()))?;
    candidate
        .strip_prefix(repository.root())
        .map(PathBuf::from)
        .with_context(|| {
            format!(
                "deployment profile {} is outside repository {}",
                candidate.display(),
                repository.root().display()
            )
        })
}

fn validate_registry(registry: &str) -> Result<()> {
    ensure!(!registry.trim().is_empty(), "registry cannot be empty");
    ensure!(
        !registry.contains("://"),
        "registry must not include a URL scheme"
    );
    ensure!(
        !registry.ends_with('/'),
        "registry must not end with a slash"
    );
    ensure!(
        !registry.chars().any(char::is_whitespace),
        "registry must not contain whitespace"
    );
    Ok(())
}
