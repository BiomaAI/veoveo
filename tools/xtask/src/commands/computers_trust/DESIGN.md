# Computers Trust Enrollment

## Standards And Protocols

The command generates X.509 P-256 CA and mutual-TLS leaf certificates in PEM, using
the workspace-qualified Rcgen pin. Provider JWT keys use Ed25519 PKCS#8 and SPKI PEM.
Command payload encryption uses a 32-byte OS-random key and a UUID key identifier.
The fixed filenames implement the private host and Computers service trust boundary.
This is fresh enrollment, with no public authentication protocol or rotation API.

`cargo xtask release computers-trust --output <new-directory> --host-name <private-DNS>`
creates private files with exclusive creation. Host, worker and operator directories
have mode 0700; all files use 0600. An existing output is rejected before mutation.
Invalid hostnames are rejected before creating the directory. An interrupted output
is incomplete and must never be installed as a complete bundle.

Provider server, worker client and supervisor client certificates have distinct CA
keys, kept only in the operator directory. `provider-ca.pem` authenticates the server
to the worker and supervisor. Host inputs `worker-client-ca.pem` and
`supervisor-client-ca.pem` supply the client roots; the Host assembles its listener
bundle from these two roots. The worker certificate `provider-worker.pem` is signed
by the worker client CA, and `guest.pem` is signed by the supervisor client CA.
Storage keeps its independent CA and worker identity.

Host files contain no worker keys, command encryption keys or CA private keys.
Worker output includes `command-key.bin` and its public `command-key-id`. Copy that
identifier into the service configuration active/key entries and reference the
mounted binary key file. The identifier is public configuration; key bytes stay in
the installation Secret and encrypted backup. Provider user admission requires the
installation's private worker OAuth registration and configured OIDC roles; client
certificates grant no user role. Supervisors require their Sandbox JWT. Worker
endpoints validate the explicit private DNS SAN; the provider certificate also
admits its internal supervisor hostname.

Leaf certificates expire after 90 days; CA certificates after ten years. Back up the
bundle using installation-owned encryption and record the renewal deadline. This
command never replaces an installed trust bundle. Rotation and retained JWT adoption
require separately qualified maintenance before their deadline. The command performs
no Kubernetes write and prints no credential bytes. Tests cover authority separation,
file permissions, invalid destinations and refusal to overwrite existing credentials.
