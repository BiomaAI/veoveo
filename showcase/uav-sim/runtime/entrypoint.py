#!/usr/bin/env python3
from __future__ import annotations

import logging

from veoveo_uav_sim import RuntimeConfig
from veoveo_uav_sim.app import run


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    run(RuntimeConfig.from_environment())


if __name__ == "__main__":
    main()
