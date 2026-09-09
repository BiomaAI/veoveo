# Console Artifact Uploads

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo resumable upload HTTP | Typed policy, UUIDv7 admission identity, one-based part PUT, paged status, completion, and cancellation from the shared Rust contract |
| Console authentication | Same-origin BFF cookie session and ephemeral `x-veoveo-csrf-token`; profile and authority remain server-derived |
| Browser File and Blob | Native file selection and bounded slices; file handles remain in memory |
| Web Workers and Web Cryptography | Worker-local SHA-256 of one admitted part per job, without a preliminary whole-file scan |
| XMLHttpRequest | Upload progress and abort for raw-body parts; sent bytes and durable receipts are separate counters |
| Browser storage | Small versioned descriptors scoped by origin, tenant, actor, and Work Context; no tokens, file bodies, or part payloads |
| Accessible dialog | Native labelled input, modal focus containment, Escape dismissal, focus restoration, and phase-only live announcements |

The application shell owns `UploadQueue`. Closing its panel or navigating to another
Console page does not destroy the queue. A scope change disposes the old queue,
aborts local work, and drops file handles. The new scope reads only its own saved
descriptors. Restored metadata is validated before an authenticated status request;
only the server can establish a completed receipt.

Selection and drag/drop share validation and require an explicit start. The queue
suppresses accidental duplicate selection and offers an explicit additional copy.
Each file has its own admission key. One active file uses the negotiated parallel
part window, further bounded by the profile memory allowance. Files waiting for
server verification release that transfer slot. Failed files retain their progress
while other queued files continue.

`hash.worker.ts` computes a part digest before its transfer. Pause aborts active
requests and terminates their hash worker. Resume reads accepted parts from the
durable ledger and transfers missing parts. A newly selected file must match size
and every accepted part digest before any missing part is sent. Wrong bytes preserve
the server session and require another file selection.

An attempted admission is persisted before sending its request. Cancellation can
therefore replay the same admission key after a lost acknowledgement, recover the
upload ID, and request deletion. The row shows Cancelling until the server responds.
If completion wins, the queue recovers the receipt and shows Ready. Clearing a
finished row changes browser metadata only.

Transfer progress reports sent and accepted bytes. A fully transferred file remains
Finishing upload until the verified receipt arrives. Status recovery reads Veoveo's
own durable upload ledger; it does not query an external provider job. Completed
receipts patch matching artifact identities and refresh canonical Console metadata.
Opening an artifact is an explicit row action and preserves current page filters.

The Node queue tests use controlled HTTP and worker doubles to exercise missing-part
resume, wrong-file rejection, cancellation races, and identity isolation. They are
state-machine evidence. Deployed acceptance additionally requires real multi-GB
transfers and a headed browser with hardware-backed WebGPU or WebGL.
