#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Publishes docs/wiki/*.md to the repository's GitHub Wiki tab.
# Run from anywhere inside the repo, with git access to GitHub (e.g. `gh auth login`).
# The wiki must already exist: create any first page in the Wiki tab once.
set -euo pipefail

cd "$(dirname "$0")/.."
REMOTE=$(git remote get-url origin)
WIKI="${REMOTE%.git}.wiki.git"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "==> Cloning $WIKI"
git clone --quiet "$WIKI" "$TMP"

# Pages that exist in the wiki but not in docs/wiki are removed (they stay in the wiki's history).
for page in "$TMP"/*.md; do
  [[ -e "$page" ]] || continue
  [[ -e "docs/wiki/$(basename "$page")" ]] || git -C "$TMP" rm --quiet "$(basename "$page")"
done
cp docs/wiki/*.md "$TMP"/

cd "$TMP"
git add -A
if git diff --cached --quiet; then
  echo "Wiki is already up to date."
  exit 0
fi
git commit --quiet -m "Update wiki from docs/wiki"
git push --quiet
echo "==> Published. See ${REMOTE%.git}/wiki"
