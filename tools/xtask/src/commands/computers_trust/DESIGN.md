# Computers Trust Enrollment

## Standards And Protocols

The command generates X.509 P-256 CA and mutual-TLS leaf certificates in PEM, using
the workspace-qualified Rcgen pin. Provider JWT keys use Ed25519 PKCS#8 and SPKI PEM.
The fixed filenames implement the private host and Computers service trust boundary.
This is fresh enrollment, with no public authentication protocol or rotation API.

`cargo xtask release computers-trust --output <new-directory> --host-name <private-DNS>`
creates private files with exclusive creation. Host, worker and operator directories
have mode 0700; all files use 0600. An existing output is rejected before mutation.
Invalid hostnames are rejected before creating the directory. An interrupted output
is incomplete and must never be installed as a complete bundle.

Provider and storage have separate CA keys, kept only in the operator directory.
Workers receive separate client keys for each authority. Only the provider CA signs
the supervisor guest certificate. Host files contain no worker keys or CA private
keys. Provider user admission still requires the worker Common Name and the guest
still needs its sandbox JWT. Worker endpoints validate the explicit private DNS SAN;
the provider certificate also admits its internal supervisor hostname.

Leaf certificates expire after 90 days; CA certificates after ten years. Back up the
bundle using installation-owned encryption and record the renewal deadline. This
command never replaces an installed trust bundle. Rotation and retained JWT adoption
require separately qualified maintenance before their deadline. The command performs
no Kubernetes write and prints no credential bytes. Tests cover authority separation,
file permissions, invalid destinations and refusal to overwrite existing credentials.
