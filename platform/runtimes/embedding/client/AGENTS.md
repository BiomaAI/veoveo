# Embedding Client

Follow the repository instructions and [client design](DESIGN.md).

- Keep model-independent values in the sibling contract crate. The client owns HTTP,
  Qwen query formatting and vLLM request fields; it has no MCP or Store dependency.
- Share one client per consumer replica. Clones must share request permits.
- Preserve deadlines through permit waits and response streaming. Errors never contain
  input text, provider bodies or API keys. Do not enable redirects or automatic retries.
- Native HTTP fixtures use synthetic vectors. GPU and model acceptance must use the
  runtime's hardware qualification and pinned checkpoint.
- Run `cargo test -p veoveo-embedding-client -p veoveo-embedding-contract` and strict
  all-target Clippy for changes to request admission or transport behavior.
