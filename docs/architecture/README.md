# Veoveo reference architecture

This directory is the isolated Veoveo UAF 1.3 and SysML 1.6 reference
architecture. It contains no client-specific mission data and is not integrated
into the whitepaper or a white-label publication.

The architecture covers every buildable first-party component and every
external runtime that participates in the stated platform architecture. The
component catalog includes all 32 Rust workspace packages, the React console,
the Python SDK and hosted-server template, internal Python and C++ executors,
deployment and verification components, and required or optional external
runtimes.

## Review files

- `index.html` is the offline browser portal and canonical report source.
- `veoveo-reference-architecture.pdf` is the fixed-layout formal review copy.
- `diagrams/*.svg` contains eleven individually reusable vector views.
- `catalogs/software-components.csv` enumerates 56 scoped software resources.
- `catalogs/interfaces-and-protocols.csv` defines 38 canonical interfaces.
- `catalogs/requirements-traceability.csv` traces 18 requirements to capability,
  activity, service, resource, and evidence.
- `catalogs/model-glossary.csv` fixes the terms used across the package.

Open the portal directly:

```bash
open docs/architecture/index.html
```

Open an individual vector view in a browser or vector application:

```bash
open docs/architecture/diagrams/05-software-resource-structure.svg
```

## Model exchange

`model/veoveo-uaf-sysml.xmi` is the tool-neutral semantic exchange source. It
applies the official OMG UAF 1.3 and SysML 1.6 profile URIs and contains stable
capability, operational activity, service, software, actual-resource,
requirement, and traceability identities. `model/model-manifest.yaml` records
the standards, provenance, validation posture, and known exchange boundary.

The XMI deliberately contains no vendor-specific diagram notation. Import it
into a UAF 1.3-capable modeling tool, resolve the official profiles, run that
tool's UAF/SysML validation, and save the resulting native project. The
published SVGs remain the layout-controlled review views. See
`model/MODEL-IMPORT.md`.

This release validates XML well-formedness, unique XMI identifiers, internal
references, catalog coverage, cross-catalog identifiers, HTML links, PDF text,
and rendered page layout. It does not claim vendor-certified UAF conformance or
native-project round-trip fidelity before the documented import validation.

## Client sharing

The generated release directory contains two packages:

- `veoveo-reference-architecture-0.1.0-review.zip` contains HTML, PDF, SVG,
  catalogs, the README, release manifest, and checksums. It requires no modeling
  software.
- `veoveo-reference-architecture-0.1.0-model-exchange.zip` adds XMI, the model
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

Render the PDF from `index.html` with headless Chrome after regeneration. Never
edit the generated SVG, XMI, HTML, PDF, or release archives as an alternate
source of truth.

Render every PDF page and a review contact sheet through the pinned PDFium and
Pillow stack:

```bash
uv run --project docs/architecture --locked python docs/architecture/tools/qa.py --clean
```

Client-specific generators, recipes, assets, and identifiers belong only under
the ignored `docs/whitelabel/` boundary. The generic architecture tools do not
discover, name, render, validate, or package client editions. Validation also
enforces the canonical generic-tool allowlist and rejects additional Python
modules in this directory.

## Model identity

- Architecture: `VV-MODEL-001`
- Version: `0.1.0`
- Revision: `2026-07-17`
- Source commit: `f19c1b2d68c1bf4f6ad133712746c5e377bb4f3a`
- Governing framework: OMG UAF 1.3
- Detailed systems language: OMG SysML 1.6
