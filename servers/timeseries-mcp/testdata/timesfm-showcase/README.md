# TimesFM Showcase Fixtures

These CSVs are TimesFM showcase-style examples used by Veoveo timeseries smoke tests.

All files use the same columns:

- `timestamp`: source event time
- `value`: numeric observed value
- `split`: `context` or `holdout`

Smoke tests should use the manifest instead of hard-coding filenames, column
names, or horizons.
