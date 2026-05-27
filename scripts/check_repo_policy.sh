#!/usr/bin/env bash
set -euo pipefail

if git ls-files --error-unmatch bookmarks.json >/dev/null 2>&1; then
  echo "FAIL: bookmarks.json is tracked"
  exit 1
fi

if ! git ls-files --error-unmatch Cargo.lock >/dev/null 2>&1; then
  echo "FAIL: Cargo.lock is not tracked"
  exit 1
fi

echo "PASS: repo policy checks succeeded"
