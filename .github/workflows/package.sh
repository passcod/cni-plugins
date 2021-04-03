#!/usr/bin/env bash

set -euxo pipefail

plugin="$1"
target="$2"
ref="$3"

suffix="${4:-}"
[[ ! -z "${suffix}" ]] && suffix="-${suffix}"

ext=""
[[ "${target}" == *-windows-* ]] && ext=".exe"

bindir="target/${target}/release"
version=$(echo "${ref}" | cut -d/ -f3)
dst="${plugin}-${version}-${target}${suffix}"

strip "${bindir}/${plugin}${ext}" || true
mkdir -p "${dst}"
cp "${bindir}/${plugin}${ext}" "${dst}/${plugin}${suffix}${ext}"
cp "${plugin}/README.md" "${dst}/README.md"
cp CHANGELOG.md COPYRIGHT LICENSE-MIT LICENSE-APACHE "${dst}/"

ls -shal "${dst}/"
tar cavf "${dst}.tar.zst" "${dst}"
