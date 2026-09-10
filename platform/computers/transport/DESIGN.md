# Computer Access Relays

## Standards And Protocols

| Boundary | Qualified profile |
|---|---|
| WebSocket RFC 6455 | HTTP/1.1 upgrade with the owning application's TLS trust; no generic tunnel or negotiated extensions |
| Veoveo terminal v2 | Closed first-frame attach, resize, Ready, replay fence and sequenced lease controls; separate binary terminal bytes |
| Veoveo private CLI relay v1 | The terminal-v2 Ready/Lease envelope only, plus binary stream bytes; trusted internal hops preserve controls and the public edge removes them |
| OpenShell `0.0.116` edge tunnel | Stock client binary gRPC transport; no Veoveo controls reach that client, which also interprets received text frames as stream bytes |
| RFC 3339 | Absolute service-issued authority deadlines, converted to local monotonic enforcement |
| [reqwest-websocket 0.6.0](https://docs.rs/reqwest-websocket/0.6.0/reqwest_websocket/) | Current stable adapter verified through crates.io on 2026-09-10; uses its upstream-selected Tungstenite 0.28 configuration and wire engine |

The gateway and Console BFF share this library. It adds no deployment or provider
dependency. Their existing authentication, endpoint selection and TLS clients remain
the admission boundary. The caller supplies a builder containing its TLS trust. The transport client enforces
HTTP/1.1 and disables redirects. An upstream request admits only the selected URL,
Host, Origin and authorization header; query credentials and URL user information
are rejected. The library never receives a destination from browser input.
reqwest-websocket owns the standards handshake and retains those TLS settings.

The first message is capped at 1 KiB and must arrive within five seconds. Its Computer
must match the admitted route. Ready must arrive within thirty seconds after the first message is forwarded; no terminal output is delivered before it. Subsequent service-issued lease
sequences increase strictly. A missed lease closes the connection. An expired relay
cannot revive when a delayed update arrives. Relays preserve the original control
bytes and deadline rather than issuing their own renewal.

Gateway, BFF and Computers service clocks must remain within one second of each other.
Each relay subtracts one second from the advertised remaining lifetime and rejects a
timestamp more than thirty-one seconds in the future. This allowance is part of the
thirty-second authority bound. It is not browser-clock synchronization: the user's
device does not grant relay authority. All enforcement after conversion is monotonic.
The installation must qualify this clock bound before admitting the public profile.

Input and output use independent pumps. Each direction has two queued messages, capped
at 64 KiB each; the wire write buffer is capped at 128 KiB. A slow consumer applies
backpressure to its own direction. The deadline guard continues running during all
reads, sends and queued delivery. It drops both streams at expiry. Buffered data does
not override an expired lease. A replay fence enables input only after downstream
delivery; the Console additionally drains historical rendering before keyboard input
or terminal responses. Output, pings, resizes and relay timers never authorize renewal.

Verification remains local until the gateway/BFF chain and installed headed journey
are qualified. The native provider is outside this library's tests.

## Stock CLI Relay Profile

`connect_cli` uses the owning application's selected trust and authorization header.
It sends no browser Origin and accepts no cookie input. Its URL restrictions, bounded
upgrade and wire limits match the browser client. The owning edge must reject browser
Origin, authorize the fixed route with the narrow CLI grant, and keep browser-cookie
authority out of that admission. This library cannot grant access or choose a target.

`relay_cli` carries a byte stream for the worker's restricted gRPC facade. It does not
implement a public arbitrary TCP proxy. CLI input may arrive immediately after upgrade;
the relay buffers it within its two-message queue until an upstream Ready establishes
authority. Service output before Ready fails. The internal profile accepts only Ready
and strictly sequenced Lease controls. Replay fences and client text frames are invalid.

An internal hop forwards original control bytes without extending their deadlines.
The public-client hop consumes those controls and delivers only binary stream bytes
to the stock CLI. Both hops apply the existing one-second clock allowance and monotonic
expiry guard independently of input/output backpressure. A blocked consumer cannot
extend authority; delayed controls after expiry cannot revive the socket. These shared
controls introduce no new browser-terminal version or client requirement.

The owning worker must maintain grant/policy renewal and the matching provider lease.
The gateway and BFF must select their respective relay mode explicitly. Roll out all
three before exposing the CLI routes; an older endpoint cannot interpret this private
profile. Existing browser relays retain their terminal-v2 replay and input rules.

Local tests exercise two real WebSocket relay hops, early HTTP/2 input, byte preservation
across renewal, control stripping, forged input, invalid service order and expiry with
either direction blocked. They use synthetic authority and do not establish pairing,
provider method restrictions, actual stock-client behavior or public ingress acceptance.
