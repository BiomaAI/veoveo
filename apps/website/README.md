# Veoveo Website

The public site at [veoveo.ai](https://veoveo.ai). It presents the platform, links to
this repository for installation, and serves the Autonomy Harness whitepaper.

The site is static HTML and CSS in `public/`. `build.mjs` copies it to `dist/` and adds
the whitepaper from `docs/veoveo-whitepaper.html`, its figures, and its PDF, so the
paper keeps one source in `docs/`.

A Cloudflare Worker with static assets serves the site as `veoveo-website`, configured
in `wrangler.jsonc`. `src/worker.js` runs only for the card videos and answers HTTP
Range requests, which Safari needs before it plays an MP4. Deploy with the repository's Cloudflare credentials in the
environment:

```sh
npm ci
npm run deploy
```

`npm run deploy` builds `dist/` and publishes it. Preview URLs are disabled, so the
workers.dev address and veoveo.ai serve only the published version. The `veoveo.ai` and `www.veoveo.ai`
custom domains route to the Worker.

The hero image comes from WaveSpeed's `openai/gpt-image-2/text-to-image` model. Its
prompt is in `art/generate.py`:

```sh
uv run --env-file ../../.env --python 3.13 art/generate.py
```

The use-case videos come from `art/usecases.py`: a still per use case from the same
image model, then an 8-second, three-shot clip from Seedance 2.5 image-to-video. The
script's docstring has the commands and the encoding settings for `public/assets/cards/`.

The hero's film card loops a silent 13.5-second cut of the Veoveo film. `art/film.py` cuts
it from a render of the film into `public/assets/cards/film.mp4` with the same encoding.
Clicking the card plays the whole film from YouTube in a modal, and the page loads the
YouTube player only on that click.

The home page renders a live 3D world behind its content: a point-cloud terrain with
scan pulses, the world model, and robots that move through it. `public/js/world.js`
builds the scene with Three.js, which `package.json` pins and `build.mjs` copies into
`dist/vendor/three/`. The models in `public/assets/models/` come from `art/models.py`.
Browsers without WebGL2 get the page without the scene.
