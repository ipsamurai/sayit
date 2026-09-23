#!/usr/bin/env bash
# Downloads speech models into the sayit data dir and verifies SHA-256.
# This is the ONLY component of sayit that touches the network; the sayit app
# runs it (from inside the signed bundle) when you click Download. The sayit
# binary itself contains no networking code.
#
#   fetch-models.sh [parakeet-v2|parakeet-v3|moonshine-medium|moonshine-small ...]
set -euo pipefail

case "$(uname -s)" in
  Darwin) DATA_DIR="${SAYIT_DATA_DIR:-$HOME/Library/Application Support/sayit}" ;;
  *)      DATA_DIR="${SAYIT_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/sayit}" ;;
esac

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# fetch <url> <dest> <sha256>
fetch() {
  local url="$1" dest="$2" want="$3"
  if [[ -f "$dest" ]] && [[ "$(sha256 "$dest")" == "$want" ]]; then
    echo "ok       $(basename "$dest")"; return
  fi
  echo "fetching $(basename "$dest")"
  # Progress bar in a terminal; quiet (errors only) when run by the app.
  local progress=(--progress-bar); [[ -t 2 ]] || progress=(-sS)
  curl -fL "${progress[@]}" --proto '=https' --tlsv1.2 --retry 3 -C - -o "$dest.part" "$url"
  local got; got="$(sha256 "$dest.part")"
  if [[ "$got" != "$want" ]]; then
    echo "checksum mismatch for $(basename "$dest"), so the file was discarded. Try again." >&2
    rm -f "$dest.part"; exit 1
  fi
  mv "$dest.part" "$dest"
  echo "verified $(basename "$dest")"
}

parakeet_v2() {
  # NVIDIA Parakeet TDT 0.6B v2 (CC-BY-4.0), English. The default: fastest and
  # most accurate in our benchmarks (see PLAN.md). Every model is pinned to an
  # exact revision or checksum, so the files can never change underneath us.
  local rev=0bbb45a3365852604aef28b538a8f066f4ccaa85
  local base="https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/$rev"
  local dir="$DATA_DIR/models/parakeet-tdt-0.6b-v2-int8"
  mkdir -p "$dir"
  fetch "$base/encoder-model.int8.onnx"       "$dir/encoder-model.int8.onnx"       3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155
  fetch "$base/decoder_joint-model.int8.onnx" "$dir/decoder_joint-model.int8.onnx" a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3
  fetch "$base/nemo128.onnx"                  "$dir/nemo128.onnx"                  a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f
  fetch "$base/vocab.txt"                     "$dir/vocab.txt"                     ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d
}

parakeet_v3() {
  # NVIDIA Parakeet TDT 0.6B v3 (CC-BY-4.0), 25 European languages.
  local rev=8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce
  local base="https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/$rev"
  local dir="$DATA_DIR/models/parakeet-tdt-0.6b-v3-int8"
  mkdir -p "$dir"
  fetch "$base/encoder-model.int8.onnx"       "$dir/encoder-model.int8.onnx"       6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09
  fetch "$base/decoder_joint-model.int8.onnx" "$dir/decoder_joint-model.int8.onnx" eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70
  fetch "$base/nemo128.onnx"                  "$dir/nemo128.onnx"                  a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f
  fetch "$base/vocab.txt"                     "$dir/vocab.txt"                     d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d
}

# fetch_tarball <url> <sha256>: verified download, then extract. Extraction
# goes to a temporary folder that is moved into place only when complete, so
# an interrupted run never leaves a half-extracted model behind.
fetch_tarball() {
  local url="$1" want="$2" models="$DATA_DIR/models"
  local tgz="$models/$(basename "$url")"
  fetch "$url" "$tgz" "$want"
  local tmp; tmp="$(mktemp -d "$models/.extract.XXXXXX")"
  tar -xzf "$tgz" -C "$tmp" --no-same-owner
  local name; name="$(basename "$url" .tar.gz)"
  rm -rf "${models:?}/$name"
  mv "$tmp/$name" "$models/$name"
  rm -rf "$tmp" "$tgz"
}

moonshine_medium() {
  # Moonshine v2 Medium streaming (MIT), English.
  fetch_tarball https://blob.handy.computer/moonshine-medium-streaming-en.tar.gz \
    07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13
}

moonshine_small() {
  # Moonshine v2 Small streaming (MIT), English.
  fetch_tarball https://blob.handy.computer/moonshine-small-streaming-en.tar.gz \
    dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5
}

AVAILABLE="parakeet-v2 parakeet-v3 moonshine-medium moonshine-small"
[[ $# -gt 0 ]] || set -- parakeet-v2
mkdir -p "$DATA_DIR/models"
for model in "$@"; do
  case "$model" in
    parakeet-v2)      parakeet_v2 ;;
    parakeet-v3)      parakeet_v3 ;;
    moonshine-medium) moonshine_medium ;;
    moonshine-small)  moonshine_small ;;
    *) echo "unknown model: $model (available: $AVAILABLE)" >&2; exit 2 ;;
  esac
done
chmod -R go-rwx "$DATA_DIR"
echo "models in: $DATA_DIR/models"
