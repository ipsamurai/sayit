#!/usr/bin/env bash
# Downloads speech models into the sayit data dir and verifies SHA-256.
# This is the ONLY component of sayit that touches the network.
# The sayit binary itself contains no networking code.
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
  curl -fL --proto '=https' --tlsv1.2 --retry 3 -C - -o "$dest.part" "$url"
  local got; got="$(sha256 "$dest.part")"
  if [[ "$got" != "$want" ]]; then
    echo "CHECKSUM MISMATCH for $dest: got $got want $want" >&2
    rm -f "$dest.part"; exit 1
  fi
  mv "$dest.part" "$dest"
  echo "verified $(basename "$dest")"
}

parakeet_v2() {
  # NVIDIA Parakeet TDT 0.6B v2 (CC-BY-4.0), English. Chosen after benchmarking
  # against v3, Moonshine v2 and Cohere Transcribe on an 8 GB laptop (see PLAN.md).
  # Pinned to an exact HuggingFace revision so the files can never change underneath us.
  local rev=0bbb45a3365852604aef28b538a8f066f4ccaa85
  local base="https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/$rev"
  local dir="$DATA_DIR/models/parakeet-tdt-0.6b-v2-int8"
  mkdir -p "$dir"
  fetch "$base/encoder-model.int8.onnx"       "$dir/encoder-model.int8.onnx"       3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155
  fetch "$base/decoder_joint-model.int8.onnx" "$dir/decoder_joint-model.int8.onnx" a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3
  fetch "$base/nemo128.onnx"                  "$dir/nemo128.onnx"                  a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f
  fetch "$base/vocab.txt"                     "$dir/vocab.txt"                     ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d
}

AVAILABLE="parakeet-v2"
[[ $# -gt 0 ]] || set -- parakeet-v2
mkdir -p "$DATA_DIR/models"
for model in "$@"; do
  case "$model" in
    parakeet-v2) parakeet_v2 ;;
    *) echo "unknown model: $model (available: $AVAILABLE)" >&2; exit 2 ;;
  esac
done
chmod -R go-rwx "$DATA_DIR"
echo "models in: $DATA_DIR/models"
