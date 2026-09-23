# Veoveo reference architecture

This directory holds a published snapshot of the Veoveo reference architecture,
modeled in UAF 1.3 and SysML 1.6. It contains no client-specific mission data and
is separate from the whitepaper and any white-label publication.

The snapshot describes the components as they were at publication and is not
regenerated from the current checkout. For Workspace, Computers, and later
ownership changes, read the current [code map](../CODEMAP.md) and
[technical design](../TECH_DESIGN.md). The published catalog includes 42 Rust
workspace packages, the React console, the Python SDK and hosted-server template,
internal Python and C++ executors,
deployment and verification components, and required or optional external
runtimes.

## Review files

- `index.html` is the offline browser portal and the source the PDF is rendered from.
- `veoveo-reference-architecture.pdf` is the fixed-layout formal review copy.
- `diagrams/*.svg` contains eleven individually reusable vector views.
- `catalogs/software-components.csv` enumerates 68 scoped software resources.
- `catalogs/interfaces-and-protocols.csv` defines 43 interfaces.
- `catalogs/requirements-traceability.csv` traces 20 requirements to capability,
  activity, service, resource, and evidence.
- `catalogs/model-glossary.csv` defines the terms used across the package.

Open the portal directly:

```bash
open docs/architecture/index.html
```

Open an individual vector view in a browser or vector application:

```bash
open docs/architecture/diagrams/05-software-resource-structure.svg
```

## Model exchange

`model/veoveo-uaf-sysml.xmi` is the tool-neutral model for exchange with other
modeling tools. It uses the official OMG UAF 1.3 and SysML 1.6 profile URIs and
gives stable identifiers to every capability, operational activity, service,
software component, actual resource, requirement, and trace link.
`model/model-manifest.yaml` records the standards, provenance, what has been
validated, and the known limits of the exchange.

The XMI contains no vendor-specific diagram notation, so any UAF 1.3-capable
tool can import it. Import it, resolve the official profiles, run that tool's
UAF/SysML validation, and save the resulting native project. The published SVGs
are the review views with fixed layout. See `model/MODEL-IMPORT.md`.

This release validates XML well-formedness, unique XMI identifiers, internal
references, catalog coverage, cross-catalog identifiers, HTML links, PDF text,
and rendered page layout. It does not claim vendor-certified UAF conformance or
native-project round-trip fidelity before the documented import validation.

## Client sharing

The generated release directory contains two packages:

- `veoveo-reference-architecture-0.3.0-review.zip` contains HTML, PDF, SVG,
  catalogs, the README, release manifest, and checksums. It requires no modeling
  software.
- `veoveo-reference-architecture-0.3.0-model-exchange.zip` adds XMI, the model
  manifest, and import guidance for a client's architecture team.

Before release, apply the contract's distribution, export-control, CUI, and
classification markings. Put client-specific capabilities, organizations,
systems, configurations, requirements, and risks in a separate overlay. Send
controlled material only through the client's approved exchange environment.

## Regeneration

The architecture tools use the locked `uv` project in this directory. The lock
pins PDF parsing, PDFium rendering, image processing, and code-quality tools;
the scripts do not depend on default site packages or an ambient virtual
environment.

Install the exact environment and render the generic model:

```bash
uv sync --project docs/architecture --locked
uv run --project docs/architecture --locked python docs/architecture/tools/render.py
uv run --project docs/architecture --locked python docs/architecture/tools/validate.py
```

Render the PDF from `index.html` with headed hardware-backed Chrome after
regeneration. Before `Page.printToPDF`, probe WebGPU and WebGL when the browser
exposes them and prove that at least one API reaches hardware. Stop when neither
API does. SwiftShader, llvmpipe, and other software renderers do not count as
hardware. Change the sources and regenerate; never edit the generated SVG, XMI,
HTML, PDF, or release archives by hand.

Render every PDF page and a review contact sheet through the pinned PDFium and
Pillow stack:

```bash
uv run --project docs/architecture --locked python docs/architecture/tools/qa.py --clean
```

Client-specific generators, recipes, assets, and identifiers belong only under
the git-ignored `docs/whitelabel/` directory. The generic architecture tools have
no knowledge of client editions. Validation also checks this directory against an
allowlist of generic tools and rejects any other Python module.

## Model identity

- Architecture: `VV-MODEL-001`
- Version: `0.3.0`
- Revision: `2026-08-21`
- Source commit: `92bf00a93147ecec552c32e4a3cb75d1dcce9439`
- Governing framework: OMG UAF 1.3
- Detailed systems language: OMG SysML 1.6
