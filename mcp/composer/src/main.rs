use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::Parser;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    CompositionDigest, GATEWAY_BINDING_SCHEMA, GATEWAY_SERVER_FRAGMENT_SCHEMA, GatewayBinding,
    GatewayCompositionInput, GatewayCompositionInputKind, GatewayCompositionProvenance,
    GatewayCompositionProvenanceSchema, GatewayControlPlane, GatewayServerFragment,
    compose_gateway_control_plane,
};

const CONTROL_PLANE_SCHEMA: &str = "veoveo.io/gateway-control-plane/v1";

#[derive(Debug, Parser)]
#[command(
    name = "gateway-compose",
    about = "Compose a Veoveo gateway control plane offline"
)]
struct Args {
    /// Installation-owned complete base control plane.
    #[arg(long)]
    base: PathBuf,
    /// Extension-owned server fragment. Repeat for each extension.
    #[arg(long = "fragment", required = true)]
    fragments: Vec<PathBuf>,
    /// Installation-owned binding. Repeat for each extension.
    #[arg(long = "binding", required = true)]
    bindings: Vec<PathBuf>,
    /// Composed ordinary GatewayControlPlane JSON.
    #[arg(long)]
    output: PathBuf,
    /// Aggregate platform-capability and artifact-audience requirements.
    #[arg(long)]
    requirements: PathBuf,
    /// Deterministic composition provenance.
    #[arg(long)]
    provenance: PathBuf,
}

struct InputDocument<T> {
    bytes: Vec<u8>,
    value: T,
}

fn main() -> Result<()> {
    let args = Args::parse();
    validate_output_paths(&args)?;

    let base = read_document::<GatewayControlPlane>(&args.base, "base control plane")?;
    base.value
        .validate()
        .context("validating base gateway control plane")?;

    let mut fragments = args
        .fragments
        .iter()
        .map(|path| {
            read_document::<GatewayServerFragment>(path, "gateway server fragment")
                .map(|document| (path, document))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut bindings = args
        .bindings
        .iter()
        .map(|path| {
            read_document::<GatewayBinding>(path, "gateway binding")
                .map(|document| (path, document))
        })
        .collect::<Result<Vec<_>>>()?;
    fragments.sort_by(|left, right| left.1.value.server.slug.cmp(&right.1.value.server.slug));
    bindings.sort_by(|left, right| left.1.value.server.cmp(&right.1.value.server));

    let mut inputs = vec![GatewayCompositionInput {
        kind: GatewayCompositionInputKind::BaseControlPlane,
        identity: "base-control-plane".to_owned(),
        schema_version: CONTROL_PLANE_SCHEMA.to_owned(),
        sha256: digest(&base.bytes)?,
    }];
    inputs.extend(
        fragments
            .iter()
            .map(|(_, document)| GatewayCompositionInput {
                kind: GatewayCompositionInputKind::ServerFragment,
                identity: document.value.server.slug.to_string(),
                schema_version: GATEWAY_SERVER_FRAGMENT_SCHEMA.to_owned(),
                sha256: digest(&document.bytes).expect("SHA-256 construction is infallible"),
            }),
    );
    inputs.extend(
        bindings
            .iter()
            .map(|(_, document)| GatewayCompositionInput {
                kind: GatewayCompositionInputKind::Binding,
                identity: document.value.server.to_string(),
                schema_version: GATEWAY_BINDING_SCHEMA.to_owned(),
                sha256: digest(&document.bytes).expect("SHA-256 construction is infallible"),
            }),
    );
    inputs.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.identity.cmp(&right.identity))
    });

    let composed = compose_gateway_control_plane(
        base.value,
        fragments
            .into_iter()
            .map(|(_, document)| document.value)
            .collect(),
        bindings
            .into_iter()
            .map(|(_, document)| document.value)
            .collect(),
    )
    .context("composing gateway control plane")?;
    let control_plane_bytes = pretty_json(&composed.control_plane)?;
    let requirements_bytes = pretty_json(&composed.requirements)?;
    let provenance = GatewayCompositionProvenance {
        schema_version: GatewayCompositionProvenanceSchema::V1,
        output_sha256: digest(&control_plane_bytes)?,
        inputs,
        contributions: composed.contributions,
        requirements: composed.requirements,
    };
    let provenance_bytes = pretty_json(&provenance)?;

    write_output(&args.output, &control_plane_bytes)?;
    write_output(&args.requirements, &requirements_bytes)?;
    write_output(&args.provenance, &provenance_bytes)?;
    Ok(())
}

fn validate_output_paths(args: &Args) -> Result<()> {
    let inputs = std::iter::once(&args.base)
        .chain(args.fragments.iter())
        .chain(args.bindings.iter())
        .collect::<Vec<_>>();
    let outputs = [&args.output, &args.requirements, &args.provenance];
    for (index, output) in outputs.iter().enumerate() {
        ensure!(
            !inputs.iter().any(|input| paths_equal(input, output)),
            "output {} cannot overwrite an input",
            output.display()
        );
        ensure!(
            !outputs
                .iter()
                .skip(index + 1)
                .any(|candidate| paths_equal(candidate, output)),
            "composition outputs must use distinct paths"
        );
    }
    Ok(())
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn read_document<T: DeserializeOwned>(path: &Path, kind: &str) -> Result<InputDocument<T>> {
    let bytes = fs::read(path).with_context(|| format!("reading {kind} {}", path.display()))?;
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("decoding {kind} {}", path.display()))?;
    Ok(InputDocument { bytes, value })
}

fn pretty_json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> Result<CompositionDigest> {
    CompositionDigest::new(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
        .context("constructing SHA-256 identity")
}

fn write_output(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
