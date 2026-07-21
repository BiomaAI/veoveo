"""Typed per-task world-model runner for the Veoveo reason server.

The Rust server owns authorization, durable tasks, and artifact publication.
This process owns one bounded inference pass: read the typed request, decode
and sample observation frames, run the world model through the image's vLLM
runtime, and write the typed response file. Stdout stays empty by contract;
diagnostics go to stderr.
"""
