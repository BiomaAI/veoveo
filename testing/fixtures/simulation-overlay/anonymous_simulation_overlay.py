"""Repository-neutral simulator overlay used by GPU acceptance."""

from __future__ import annotations


def prove_overlay(wp: object) -> str:
    """Run one overlay-owned CUDA operation and return the declared marker."""

    values = wp.array([4.0, 5.0, 6.0], dtype=wp.float32, device="cuda:0")
    if values.numpy().tolist() != [4.0, 5.0, 6.0]:
        raise RuntimeError("anonymous overlay CUDA allocation did not round trip")
    return "anonymous-simulation-overlay"
