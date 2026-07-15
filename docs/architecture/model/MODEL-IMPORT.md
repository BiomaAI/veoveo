# Model import and validation

`veoveo-uaf-sysml.xmi` is a tool-neutral exchange baseline. It carries the
semantic model and stable identifiers used by the HTML, PDF, SVG views, and CSV
catalogs. It does not contain vendor-specific diagram notation.

## Required modeling capability

Use a modeling environment that supports UAF 1.3, SysML 1.6, UML 2.5.1, XMI
import, profile validation, and controlled export. Confirm offline or air-gapped
operation when the client environment requires it. Confirm that the selected
edition supports the official UAF 1.3 profile rather than only UAF 1.2 or UPDM.

The exchange references these official profiles:

```text
https://www.omg.org/spec/UAF/20241101/UAF.xml#UAF
https://www.omg.org/spec/SysML/20181001/SysML.xmi#SysML
```

## Import procedure

1. Create an empty UAF 1.3 project with SysML 1.6 enabled.
2. Register or map the official OMG profile URIs if the tool uses a local
   standard-profile library.
3. Import `veoveo-uaf-sysml.xmi` without changing its stable `VV-*` identifiers.
4. Confirm that the packages Strategic, Operational, Services, Resources,
   Actual Resources, Requirements, Operational Performers and Roles, and
   Traceability appear.
5. Run UAF and SysML profile validation. Treat unresolved stereotypes,
   references, or constraints as import failures.
6. Compare imported element counts with `model-manifest.yaml` and the CSV
   catalogs.
7. Save the result in the tool's native format and record the tool, edition,
   version, plug-in version, import date, warnings, and model checksum.
8. Export a fresh XMI copy, compare stable identifiers and relationships, and
   retain the comparison as round-trip evidence.

## Diagram handling

The SVG files are controlled review layouts, not editable vendor diagram
notation. Reconstruct native UAF/SysML views from the imported semantic model
when the client requires model-native navigation. Keep each native view tied to
its `VV-VIEW-*` identity and preserve the title, viewpoint, concern, and model
element selection shown in the reference report.

## Acceptance record

Record these fields beside the imported native project:

```text
Tool and edition:
Tool version:
UAF plug-in version:
SysML plug-in version:
Profile URI resolution:
Validation result:
Import warnings:
Round-trip result:
Native project SHA-256:
Exported XMI SHA-256:
Reviewer:
Review date:
```

The native project becomes a client exchange artifact only after this record is
complete. The generic repository baseline remains tool-neutral.
