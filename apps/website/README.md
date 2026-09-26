# Veoveo Website

The public site at [veoveo.ai](https://veoveo.ai). It presents the platform, links to
this repository for installation, and serves the Autonomy Harness whitepaper.

The site is static HTML and CSS in `public/`. `build.mjs` copies it to `dist/` and adds
the whitepaper from `docs/veoveo-whitepaper.html`, its figures, and its PDF, so the
paper keeps one source in `docs/`.

A Cloudflare Worker with static assets serves the site as `veoveo-website`, configured
in `wrangler.jsonc`. `src/worker.js` runs only for the use-case videos and answers HTTP
Range requests, which Safari needs before it plays an MP4. Deploy with the repository's Cloudflare credentials in the
environment:

```sh
npm ci
npm run deploy
```

`npm run deploy` builds `dist/` and publishes it. The `veoveo.ai` and `www.veoveo.ai`
custom domains route to the Worker.

The hero image comes from WaveSpeed's `openai/gpt-image-2/text-to-image` model. Its
prompt is in `art/generate.py`:

```sh
uv run --env-file ../../.env --python 3.13 art/generate.py
```

The use-case videos come from `art/usecases.py`: a still per use case from the same
image model, then an 8-second, three-shot clip from Seedance 2.5 image-to-video. The
script's docstring has the commands and the encoding settings for `public/assets/cards/`.
