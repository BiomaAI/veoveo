#!/usr/bin/env python3
from __future__ import annotations

import logging

from veoveo_simulation_view import RendererConfig, run


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    run(RendererConfig.from_environment())


if __name__ == "__main__":
    main()
