# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
# The official image catalog still publishes 1.98.0 on 2026-09-08. Use its
# Bookworm environment and rustup only; all compilation uses stable 1.98.1.
FROM docker.io/library/rust:1.98.0-slim-bookworm@sha256:1469a27c125cb5a3aebfa4f4e4665d935b02fb72cc093b2c974b3d740e43f157

RUN rustup set auto-self-update disable \
    && rustup toolchain install 1.98.1 --profile minimal --no-self-update \
    && rustup default 1.98.1 \
    && rustup toolchain uninstall 1.98.0 \
    && rustc --version \
    && cargo --version

ENV RUST_VERSION=1.98.1
