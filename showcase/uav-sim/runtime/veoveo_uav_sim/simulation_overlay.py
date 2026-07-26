"""Hardware identity probe for the Veoveo UAV simulation overlay."""

from __future__ import annotations


def prove_overlay(wp: object) -> str:
    """Run one overlay-owned CUDA operation and return the declared marker."""

    values = wp.array([1.0, 2.0, 3.0], dtype=wp.float32, device="cuda:0")
    if values.numpy().tolist() != [1.0, 2.0, 3.0]:
        raise RuntimeError("UAV overlay CUDA allocation did not round trip")
    return "veoveo-uav-simulation"
