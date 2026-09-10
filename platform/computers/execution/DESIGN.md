# Private Computer Execution

Status: the codec, packaged launcher and private runtime adapter pass local and
native-provider fixtures. Public agent grants, durable execution Tasks and their
cancellation integration remain delivery work.

## Standards And Protocols

| Boundary | Selected profile |
|---|---|
| `veoveo.io/computer-execution/v1` | Private closed JSON request preceded by its four-byte unsigned big-endian length; maximum encoded body 2 MiB |
| JSON / UTF-8 / RFC 4648 Base64 | Explicit argv, relative working directory, environment overrides and finite binary stdin; no authority or provider endpoint fields |
| POSIX process execution | Direct argv execution with inherited Computer environment and explicit overrides; shell use requires an explicit interpreter in argv |
| Linux `openat2` | `RESOLVE_BENEATH` and `RESOLVE_NO_MAGICLINKS` resolve the launch directory under the retained home; ordinary internal relative symlinks remain usable |
| Linux `O_TMPFILE` and procfs descriptor reopen | Finite program stdin uses an anonymous file under the Computer's private `/tmp`, reopened read-only; no named-file fallback or detached producer |
| nix `0.31.3` | Exact syscall-wrapper pin, confirmed against the upstream changelog on September 10, 2026; selected filesystem and user APIs |
| OpenShell `0.0.116` private execution adapter | Existing authenticated stream transports a fixed launcher command and framed stdin; provider completion and interruption guarantees remain those of the qualified runtime |

## Ownership And Authority

The Computers domain authorizes and journals one execution. Its runtime sends
`/usr/local/bin/veoveo-computer-exec` with the fixed working directory
`/sandbox/persistent`. Arguments, environment values and program stdin stay inside
the private frame. This keeps request values out of the provider's command-preview
logs without modifying or recompiling that provider for each execution feature.

The launcher accepts only the selected numeric UID/GID 10001 with no other
supplementary groups. It changes no credentials and obtains no host or provider
authority. The request cannot select the retained-home root. Kernel descriptor
resolution prevents directory traversal and magic-link escape during admission.
The program may access whatever the Computer's installed isolation policy admits;
selecting a working directory does not promise isolation to that subdirectory.

## Request And Process Behavior

Requests admit up to 1,024 arguments and 32 KiB of aggregate argument bytes. Empty
non-program arguments remain significant. The executable can be an absolute path,
a relative path or a name resolved through the Computer's PATH. No shell interprets
the argument vector unless the requester explicitly selects one as the executable.

At most 64 environment overrides consume 16 KiB of key/value bytes. Names follow
the POSIX variable-name profile; values may contain newlines but cannot contain
NUL. Overrides are applied only to the selected program, after launcher admission.
They do not affect the launcher's own loader or request parsing. The selected
Computer's inherited environment remains available, including its admitted proxy
and certificate configuration. Finite program stdin is bounded at 1 MiB.

The codec rejects unknown and duplicate request fields, duplicate environment keys,
invalid versions and oversized frames. Public request types have no Debug or Display.
Encoded buffers and decoded request values are erased on ordinary drop. This is
best-effort lifetime reduction, not a guarantee against inspection by the Computer's
owner or an already privileged host process.

The selected supervisor denies `memfd_create` to prevent fileless execution.
Stdin therefore occupies a bounded anonymous file in the Computer's ephemeral
`/tmp` filesystem. `O_EXCL` prevents linking it into the directory tree. It has no
directory entry and disappears when its last descriptor
closes, including after a process crash. The launcher closes its writable handle
before execution. The program receives a read-only descriptor with exact bytes and
EOF. This does not promise forensic erasure from the host's backing storage or
immutability against another process with the Computer owner's authority.

After resolving the directory and preparing finite stdin, the single-threaded
launcher replaces itself with the requested program. Setup failures return a fixed
diagnostic and exit 125. The program's own exit code and raw output remain intact.
An exit code is not a claim that arbitrary program side effects were rolled back.

## Completion And Cancellation Boundary

This launcher introduces no cancellation or recovery authority. The domain retains
the operation identity and resource fence if native observation becomes uncertain.
The selected provider has no qualified per-command termination acknowledgement.
Computer-scoped termination must therefore settle through the existing authoritative
Stop boundary before cancellation can be reported as complete. That domain/profile
integration and its user-facing behavior remain implementation work.

## Qualification

Codec tests cover framing, exact binary input, aggregate bounds and duplicate-key
refusal. Kernel tests cover internal symlinks, escape attempts and descriptor-based
directory selection. A disposable container runs the actual launcher as UID/GID
10001 and checks argument, environment, stdin, output and exit preservation.
The native provider fixture executes the packaged launcher under the retained
profile. It preserves 100,000 binary input bytes, verifies the initial terminal
directory, rejects an escaping symlink and inspects enabled command-preview logs.
An execution timeout remains unknown. Stop then fences a detached descendant;
restart preserves its final file without allowing the old writer to resume. This
qualifies the provider boundary, not public Task cancellation or agent authority.

Template packaging uses the shared Rust artifact family. Its OCI working directory
stays `/sandbox`, which the provider reserves independently of the retained mount.
A root-owned login profile starts the shell in `/sandbox/persistent`; user-owned
login configuration can then choose a preferred directory. Historical template
fingerprints remain independent of the new launcher-bearing template.

The syscall profile follows [Linux openat2](https://man7.org/linux/man-pages/man2/openat2.2.html)
and [Linux O_TMPFILE](https://man7.org/linux/man-pages/man2/open.2.html).
The wrapper pin follows the [nix changelog](https://github.com/nix-rust/nix/blob/master/CHANGELOG.md).
