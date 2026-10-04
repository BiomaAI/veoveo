# Speech HTTP Admission

## Standards And Protocols

The public native projection uses HTTP JSON controls and binary little-endian float32
mono PCM. Shared shapes live in `veoveo-speech-contract`. It maps to Speech MCP
2026-07-28 tools and resources. Internal forwarding uses the existing signed gateway
assertion and catalog-scoped upstream TLS pool.

## Authority And Bounds

Routes fix the server to `speech`. Start and chunks require `start_dictation` policy;
finish and cancel require their matching tools. Receipt reads require the exact
`speech://dictation/{id}` resource. The current actor must be a human Contributor
with a browser session family. Each chunk receives a fresh policy decision and
assertion. The gateway records each denial; Speech records session admission and
one terminal summary with chunk and duration counts. Allowed chunks produce no
individual audit rows. The domain binds the receipt to that actor and session.

HTTP route admission uses Speech's `DictationSessionId`. Receipt policy targets use
its `DictationUri` builder. Responses decode the same owner contract, which requires
the returned ID and resource address to name the same session.

The projection accepts at most 192000 input bytes, limits requests to 25 seconds and
validates the returned session ID and transcript bounds. No browser-selected upstream
URL, profile override in a query, provider credential or Artifact byte path is admitted.
The service owns ephemeral audio, cancellation and capacity; the gateway owns no
Speech state. The browser edge fixes the Workspace profile and uses existing cookies
and CSRF checks. Only the Workspace HTML grants same-origin microphone permission.

## Registration And Shutdown

The owner exports its gateway router through the `gateway` feature. Composition
registers a deferred factory with the shared gateway HTTP module registry. Profile
authentication uses the registered raw profile spelling and current shared catalog.
The factory starts no work until required bindings pass validation. Request workers,
response streams and upgrades use its module scope; shutdown closes admission and
cancels transport without inventing an operation settlement.
