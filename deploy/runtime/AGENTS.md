# Deployment Runtime

This crate owns process execution for disposable Veoveo deployment profiles. Keep
schemas, ownership types, and pure planning in `../contract`. Enterprise installations
retain their declared GitOps owner.

Keep source resolution, Helm inputs, configuration preparation, cluster lifecycle, and
GPU allocation in their own modules. A public helper must have a concrete caller. The
release publisher and profile installer share chart locking through this crate.

Changes to mutation selection must cover the full installation boundary, including
namespace creation, node bootstrap, allocator releases, persistent claims, public
configuration, and Helm releases. A release-loop filter does not establish that boundary.

The deployment smoke binary calls this library and owns scenario assertions and evidence.
Run the library tests and the focused deployment smoke checks through the repository test
recorder after changing this execution boundary. GPU runtime acceptance still requires
hardware evidence under the repository instructions.
