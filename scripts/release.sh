#!/bin/sh
set -eu

usage='usage: scripts/release.sh 0.2.0 "what changed"'
version="${1:?$usage}"
notes="${2:?$usage}"
version="${version#v}"

cd "$(dirname "$0")/.."

if [ -n "$(git status --porcelain)" ]; then
  echo "Commit or stash your changes first." >&2
  exit 1
fi

sed -i.bak "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
rm Cargo.toml.bak
cargo build --quiet
cargo test --quiet

git add Cargo.toml Cargo.lock
git commit -m "Release v$version"
git tag "v$version"
git push origin HEAD "v$version"
gh release create "v$version" --title "pebl $version" --notes "$notes"

echo "Released pebl $version. Everyone's pebl shows it within a day, and pebl update installs it."
