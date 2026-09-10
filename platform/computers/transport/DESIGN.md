# Computer Terminal Relays

## Standards And Protocols

| Boundary | Qualified profile |
|---|---|
| WebSocket RFC 6455 | HTTP/1.1 upgrade with the owning application's TLS trust; no generic tunnel or negotiated extensions |
| Veoveo terminal v2 | Closed first-frame attach, resize, Ready, replay fence and sequenced lease controls; separate binary terminal bytes |
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
