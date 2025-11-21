#!/usr/bin/env bash
set -euo pipefail

: "${PLATFORM:=linux/amd64}"
: "${OUTPUT_NAME:=simple_git_cicd}"

echo "Building ${OUTPUT_NAME} for platform '${PLATFORM}' via Docker..."

docker build \
  --platform "${PLATFORM}" \
  --target artifact \
  --output type=local,dest=. \
  .

ARTIFACT_PATH="./simple_git_cicd"
if [[ ! -f "${ARTIFACT_PATH}" ]]; then
  echo "Expected artifact not found at ${ARTIFACT_PATH}" >&2
  exit 1
fi

TARGET_NAME="${OUTPUT_NAME}-${PLATFORM//\//-}"
mv "${ARTIFACT_PATH}" "${TARGET_NAME}"
chmod +x "${TARGET_NAME}"

echo "Binary copied to ${TARGET_NAME}"
