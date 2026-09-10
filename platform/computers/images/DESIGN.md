# Computer Image

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| OCI image / Linux AMD64 | Ubuntu 26.04 image manifest `sha256:e5a4d6262ab5dbc25a85e60550dd7c87fd41a74fe43881534ed8288b2a7a3f8d`, published September 1, 2026 |
| Ubuntu signed archive | Snapshot `20260909T000000Z` fixes distro package inputs; package inventory is retained in the image |
| CA bootstrap | Snapshot package `ca-certificates` 20260223, SHA-256 `f7025ab9b24cd73215510931037b02d6960d89584d0d00afba81851abdbe6ef1`; its Mozilla roots enable verified HTTPS before APT installs/configures the same package |
| OpenShell process policy | Private supervisor applies confinement and runs the personal process as numeric UID/GID 10001 |
| Retained Computer home | `/sandbox/persistent`, mounted by the selected provider/storage adapter; image contents do not establish persistence or capacity enforcement |

The candidate supplies a shell, Git, Python and SFTP with the network utilities
required by the supervisor. It has no installation credentials or host control
socket. Installed policy governs filesystem and network access. The provider's
control process and retained allocator are separate trust boundaries.

The upstream community image contains a wider agent/tool set and pins older tool
versions. The initial Veoveo image owns its smaller package closure. Additional
developer tools can become admitted templates after their inputs and behavior are
qualified. The image remains a candidate until native containment, terminal,
retention and deployed acceptance pass.

The base release is verified against [Ubuntu's release catalog](https://releases.ubuntu.com/).
Snapshot behavior follows the [Ubuntu snapshot service](https://snapshot.ubuntu.com/).
