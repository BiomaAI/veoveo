# vendor-three

This directory builds the committed vendor artifact
`servers/view-mcp/assets/vendor/three-bundle.min.js`, the self-contained
three.js bundle that the view MCP app inlines into its HTML. The artifact is
plain classic-script JavaScript. Evaluating it defines three globals:
`VENDOR.THREE` (the full three.js namespace), `VENDOR.OrbitControls`, and
`DracoDecoderModule` (the JS-only Emscripten draco decoder factory shipped
inside the three package).

## Version pins

`three` is pinned to exactly `0.180.0` because the app's scene code and the
draco decoder path were validated against revision 180, and three publishes
breaking changes on its ordinary monthly cadence without semver signaling.
`esbuild` is pinned to exactly `0.28.1`, the latest stable release when this
pipeline was created. Both pins live in `package.json` with no range operators,
and `package-lock.json` freezes the full resolution so `npm ci` reproduces the
identical tree. Rebuilding from a clean checkout yields a byte-identical
artifact. Bumping either pin is a deliberate change: update `package.json`,
delete `package-lock.json`, rerun the build to regenerate it, and commit the
lock file together with the rebuilt artifact.

## Rebuilding

```sh
cd tools/vendor-three
./build.sh        # or: npm run build
```

The script runs `npm ci` against the committed lock file, bundles `entry.mjs`
with esbuild (`--bundle --minify --format=iife --global-name=VENDOR`), and
appends `node_modules/three/examples/jsm/libs/draco/gltf/draco_decoder.js`
after the esbuild output. It then escapes any `</script` substring, validates
the result with `new Function`, evaluates it under Node with a minimal
`self`/`window` shim to prove all three globals appear without any network
call, and finally enforces the size ceiling. Any failed check aborts the build
with a nonzero exit.

## Expected sizes

| Part | Bytes |
| --- | --- |
| esbuild bundle (three core + OrbitControls) | 720,474 |
| draco_decoder.js (JS-only Emscripten build) | 512,465 |
| final artifact | 1,232,940 |

The build fails if the final artifact exceeds 1,700,000 bytes. A rebuild that
lands more than a few kilobytes away from these figures without a version bump
deserves investigation before commit.

## The `</script` constraint

The app inlines this file directly inside an HTML `<script>` tag, and an HTML
parser terminates a script element at the first case-insensitive `</script`
sequence regardless of JavaScript string or comment context. The build
therefore rewrites every occurrence to `<\/script`, which is byte-safe inside
JS string and regex literals, and fails if the sequence survives. The current
inputs contain no occurrence at all, so the transform is a guard rather than
an active rewrite. Keep the guarantee in mind when touching `entry.mjs` or the
concatenation step: never emit content that could reintroduce the sequence
unescaped.
