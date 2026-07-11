# Perception runtime configuration

Copy `catalog.example.json` to `catalog.json` in the directory mounted as
`PERCEPTION_CONFIG_DIR`. Put the referenced TensorRT engine and its label files
under `PERCEPTION_MODEL_DIR`.

`primary-detector.txt` is the model-specific `gst-nvinfer` configuration. It
must describe the exact engine outputs, labels, class count, preprocessing, and
parser library. The runner overrides `model-engine-file`, `batch-size`,
`process-mode`, and `interval` from the typed catalog, so the catalog's
`model_path` is the canonical model identity.

For tracking, copy a DeepStream 9 low-level tracker YAML into this directory as
`tracker.yml`. NVIDIA's NvSORT and NvDCF samples are suitable starting points;
tracker width and height in the catalog must be positive multiples of 32.
Delete the `detect-and-track` pipeline from `catalog.json` when tracking is not
deployed.

The server validates every catalog, engine, inference config, and tracker path
at startup. Missing files are deployment failures, not deferred fallbacks.
