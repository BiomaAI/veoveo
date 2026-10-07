# Offline Bundle

## Standards And Protocols

The builder and loader use JSON lock and bundle metadata revision 2. Controlled
members use camelCase. Both receivers reject other revisions and unknown members,
including nested image declarations. OCI image references and SHA-256 image
identities retain their existing spelling. Tar archives, SHA256SUMS file checksums
and SPDX JSON SBOMs carry the existing packaging profile.

## Admission And Installation

`admission.jq` supplies one receiving profile for the builder and loader. The lock
selects unique tagged references and digest-pinned external sources. Bundle metadata
must contain exactly one resolved image identity for every selected reference.
The loader also compares its embedded lock with the separately checksummed lock.
It uses its own co-located admission program, rather than a program supplied by the
archive. Admission completes before Docker or containerd import and before writing
the installation destination. File checksum verification precedes admission.

This unreleased format uses a coordinated fresh bundle cut. Producers and loaders
must use revision 2 together; there is no historical decoder. Existing image pins,
archive contents and checksum framing do not change. Checksums detect corruption;
they do not authenticate a publisher. Sites must trust the bundle delivery source.

The supported platforms are linux/amd64 and linux/arm64. Docker verifies resolved
image IDs after import. The containerd path verifies reference presence using its
existing namespace profile. GPU workloads require their declared NVIDIA hardware
and site prerequisites; a successful import does not qualify execution.

The builder and loader slurp each lock and bundle input before shared admission. Each input must contain exactly one JSON document. A second document refuses the stream even when both documents independently satisfy the current profile or the last document matches the selected images. Admission precedes build, image-runtime and installation-destination effects.
