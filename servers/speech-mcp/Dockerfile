# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
# Staging resolves this dependency to an already normalized immutable OCI digest.
FROM speech-dependencies

# Independent layers keep Rust/Python edits from unpacking or rewriting CUDA.
COPY --link --from=veoveo-image-assets /servers/speech-mcp/runner/src /opt/speech-source
COPY --link --from=veoveo-image-assets /servers/speech-mcp/runner/ATTRIBUTION.md /usr/share/doc/speech/ATTRIBUTION.md
COPY --link --from=veoveo-rust-artifacts /bin/speech-mcp /usr/local/bin/speech-mcp
EXPOSE 8811
ENTRYPOINT ["/usr/local/bin/speech-mcp"]
