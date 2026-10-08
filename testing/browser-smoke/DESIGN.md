# Browser Mechanics

## Standards And Protocols

This library consumes Chromium CDP through the maintained WebSocket transport and admits headed hardware-backed WebGPU or WebGL adapters. It implements only the CDP requests used by owning browser assertions.

## Reusable Browser Mechanics

This library owns headed CDP connection, protocol transport and hardware adapter admission. Installed browser and flight assertions compile in the Bioma acceptance composition and reuse this library. A headless sampler case qualifies its behavioral contract only. Hardware-rendered acceptance requires the existing headed WebGPU or WebGL checks.

The `contract` feature exports hardware identity admission without CDP, async runtimes or test-support dependencies. `hardware.rs` owns that public value and software-renderer refusal. The default `runtime` feature adds `transport.rs` and preserves the headed CDP API exported at the crate root.
