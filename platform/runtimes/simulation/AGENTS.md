# Simulation Runtime Instructions

- Keep this image provider-neutral at the overlay boundary. Domain code, assets,
  entrypoints, simulators, and customer identities do not belong here.
- Advance the tuple as one compatibility release. Never update Isaac, Isaac Lab,
  Warp, Newton, MuJoCo, Torch, Python, CUDA, or Kit independently without rebuilding
  and rerunning every hardware and overlay gate.
- Keep one authoritative Warp and Newton module root before and after Kit launch.
- Treat `simulation-runtime.lock.json` and `requirements.lock` as build inputs. Verify every
  downloaded archive or wheel by SHA-256.
- Hardware CUDA, RTX output, and NVENC are mandatory. A CPU or software renderer run is
  never acceptance evidence.
- Do not add an application entrypoint or compatibility alias to this base.
