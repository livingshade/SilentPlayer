#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p test-assets/audio

curl -L --fail --show-error \
  --output test-assets/audio/into_the_oceans_chorus.ogg \
  'https://commons.wikimedia.org/wiki/Special:Redirect/file/%22Into_the_Oceans_and_the_Air%22_%28chorus%29.ogg'

curl -L --fail --show-error \
  --output test-assets/audio/into_the_oceans_instrumental.ogg \
  'https://commons.wikimedia.org/wiki/Special:Redirect/file/%22Into_the_Oceans_and_the_Air%22.ogg'

curl -L --fail --show-error \
  --output test-assets/audio/funk_room_reverb.ogg \
  'https://upload.wikimedia.org/wikipedia/commons/4/42/Room_reverb_effect_in_a_mix_-_longer_funk_example.ogg'

file test-assets/audio/*
