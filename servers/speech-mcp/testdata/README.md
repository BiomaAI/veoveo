# Speech Fixtures

`english.wav` is NVIDIA's public Parakeet quickstart sample, LibriSpeech utterance
`2086-149220-0033`, downloaded from
<https://dldata-public.s3.us-east-2.amazonaws.com/2086-149220-0033.wav>.
SHA-256: `5fceacff0315d49cb59fcc505bcecf1ed5f2f35c2897b1e65a59f30e5d922150`.
The fixture contains 7.435 seconds of mono 16 kHz PCM16 speech. Native acceptance
checks the final phrase and word timestamps, then streams the same recording as live
PCM at its actual cadence.

`spanish.wav` is a Latin American Spanish test example from Google's
[FLEURS dataset](https://huggingface.co/datasets/google/fleurs), CC-BY-4.0.
`spanish.json` records its exact repository revision, original filename, reference
text and SHA-256. Attribution: Google FLEURS contributors. The recording is copied
without alteration. Native acceptance checks recognized phrases against the reference.

These public fixtures contain no customer audio. Their recognition checks establish
functional language support, not population-level accuracy or a word-error benchmark.
