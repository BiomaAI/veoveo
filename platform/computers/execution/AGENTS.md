# Computer Execution Instructions

Follow the root and owning DESIGN.md. This crate is a private request codec and
guest launcher. It does not authorize principals, settle Tasks or cancel provider
work. Domain authority remains in Computers and transport remains in its runtime.

Keep the provider command fixed. Request values travel only through bounded stdin,
never process arguments, logs or public diagnostics. Program execution inherits the
Computer's confinement; a working directory is not an arbitrary-code security scope.
Use kernel-confined descriptor resolution. Do not introduce a path-string fallback.

