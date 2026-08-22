#!/usr/bin/env python3
"""Apply the pinned Isaac Sim 6.0.1 tensor-view bridge for Newton 1.5."""

from __future__ import annotations

import sys
from pathlib import Path

BEFORE = '''        # Create simulation views (unified path for all backends)
        cls._physics_sim_view__warp = omni.physics.tensors.create_simulation_view(
            "warp", stage_id=stage_id, backend=cls._engine
        )
        cls._physics_sim_view__warp.set_subspace_roots("/")
        if create_simulation_view:
            cls._physics_sim_view = omni.physics.tensors.create_simulation_view(
                cls.get_backend(), stage_id=stage_id, backend=cls._engine
            )
            cls._physics_sim_view.set_subspace_roots("/")
'''

AFTER = '''        # Create simulation views (unified path for all backends)
        if cls._engine == "newton":
            import isaacsim.physics.newton
            import isaacsim.physics.newton.tensors

            newton_stage = isaacsim.physics.newton.acquire_stage()
            if newton_stage is None:
                raise RuntimeError("Newton stage is unavailable during tensor-view creation")

            def create_view(frontend_name: str):
                return isaacsim.physics.newton.tensors.create_simulation_view(
                    frontend_name=frontend_name,
                    stage_id=stage_id,
                    newton_stage=newton_stage,
                )
        else:

            def create_view(frontend_name: str):
                return omni.physics.tensors.create_simulation_view(
                    frontend_name, stage_id=stage_id, backend=cls._engine
                )

        cls._physics_sim_view__warp = create_view("warp")
        cls._physics_sim_view__warp.set_subspace_roots("/")
        if create_simulation_view:
            cls._physics_sim_view = create_view(cls.get_backend())
            cls._physics_sim_view.set_subspace_roots("/")
'''


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: apply_isaac_newton_1_5.py SIMULATION_MANAGER")
    target = Path(sys.argv[1])
    source = target.read_text(encoding="utf-8")
    matches = source.count(BEFORE)
    if matches != 1:
        raise RuntimeError(
            f"expected one Isaac Sim 6.0.1 tensor-view block, found {matches}"
        )
    target.write_text(source.replace(BEFORE, AFTER), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
