"""Hardware-only Isaac/RTX implementation of Veoveo Simulation View."""

from .config import RendererConfig


def run(config: RendererConfig) -> None:
    from .runtime import run as run_renderer

    run_renderer(config)


__all__ = ["RendererConfig", "run"]
