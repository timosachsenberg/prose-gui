#!/bin/bash
# GCC 13 for the Linux builds (Ubuntu 22.04), from the files that native/toolchain/jammy-gcc13.lock names: the
# packages of GCC 13 and of the runtime libraries it needs, as ppa:ubuntu-toolchain-r/test builds them for
# 22.04, and their source packages (native/scripts/toolchain_lock.py writes the lock).
#
#   gcc13.sh install [<dir>]         installs the packages of this machine's architecture with apt. Without
#                                    <dir> they come from the repository's own copy, the release the lock
#                                    names (.github/workflows/toolchain.yml makes it), and from Launchpad's
#                                    pool when that cannot be had; with <dir>, from that folder.
#   gcc13.sh fetch <kind> <dir>      the files of <kind> (amd64, arm64 or source) from Launchpad's pool
#   gcc13.sh check <kind> <dir>      <dir> holds the lock's files of <kind>, each of its size and SHA-256,
#                                    and nothing else
#   gcc13.sh pack <kind> <dir> <out> <out>/gcc13-jammy-<kind>.tar of <dir>'s files: what the release holds
#   gcc13.sh release                 the name of the release
#   gcc13.sh test                    the installed g++-13 compiles native/toolchain/test.cpp (C++23, OpenMP) and
#                                    the program runs; apt has no source at Launchpad
#
# Why a copy: a build that has to compile OpenMS (a new pin) needs these packages, and Launchpad was out of
# reach for hours more than once (the builds of 0.7.0 waited more than four hours for it). The copy is on GitHub, as the
# build itself is. Whatever the files come from, only files with the lock's SHA-256 are installed, and the lock's
# hashes are those of the PPA's signed index.
#
# GCC13_RELEASES: where the releases are (default: this project's, https://github.com/.../releases/download).
# GCC13_ONLY_RELEASE=1: `install` without <dir> fails when the release cannot be had, instead of going to
# Launchpad (the workflow's test of the release).
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd -P)
LOCK=${GCC13_LOCK:-$HERE/../toolchain/jammy-gcc13.lock}
RELEASES=${GCC13_RELEASES:-https://github.com/timosachsenberg/prose-gui/releases/download}
# the PPA's two hosts: Launchpad's current one and its older front end, which served while the other did not
POOLS=(
  https://ppa.launchpadcontent.net/ubuntu-toolchain-r/test/ubuntu
  http://ppa.launchpad.net/ubuntu-toolchain-r/test/ubuntu
)

usage() {
  sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//' >&2
  exit 2
}
[ -f "$LOCK" ] || { echo "error: no lock file $LOCK" >&2; exit 1; }

release() { awk '$1 == "release" { print $2 }' "$LOCK"; }

is_kind() {
  case "$1" in amd64 | arm64 | source) ;; *) echo "error: '$1' is not amd64, arm64 or source" >&2; exit 2 ;; esac
}

# rows <kind>: "<SHA-256> <size> <path below the PPA>" of each file of <kind>
rows() { awk -v kind="$1" '$1 == kind { print $2, $3, $4 }' "$LOCK"; }

# is <file> the lock's file of <sha> and <size>?
is_file() {
  [ -f "$1" ] && [ "$(stat -c %s "$1")" = "$3" ] && [ "$(sha256sum "$1" | cut -d' ' -f1)" = "$2" ]
}

download() { # <url> <into>
  curl --fail --silent --show-error --location --retry 2 --retry-delay 5 --retry-all-errors \
    --connect-timeout 20 --max-time 900 -o "$2" "$1"
}

check() { # <kind> <dir>
  local kind=$1 dir=$2 sha size path wanted=0 bad=0
  is_kind "$kind"
  [ -d "$dir" ] || { echo "error: no folder $dir" >&2; return 1; }
  while read -r sha size path; do
    wanted=$((wanted + 1))
    if ! is_file "$dir/${path##*/}" "$sha" "$size"; then
      echo "error: $dir/${path##*/} is missing or is not the lock's file (size $size, SHA-256 $sha)" >&2
      bad=$((bad + 1))
    fi
  done < <(rows "$kind")
  [ "$wanted" -gt 0 ] || { echo "error: the lock names no file of $kind" >&2; return 1; }
  [ "$bad" -eq 0 ] || return 1
  local have
  have=$(find "$dir" -mindepth 1 | wc -l)
  if [ "$have" -ne "$wanted" ]; then
    echo "error: $dir holds $have entries, the lock names $wanted files of $kind:" >&2
    find "$dir" -mindepth 1 >&2
    return 1
  fi
}

fetch() { # <kind> <dir>
  local kind=$1 dir=$2 sha size path file pool got pools=("${POOLS[@]}")
  is_kind "$kind"
  mkdir -p "$dir"
  while read -r sha size path; do
    file=$dir/${path##*/}
    is_file "$file" "$sha" "$size" && continue
    got=0
    for pool in "${pools[@]}"; do
      if download "$pool/$path" "$file.part" && is_file "$file.part" "$sha" "$size"; then
        mv "$file.part" "$file"
        got=1
        break
      fi
      rm -f "$file.part"
      echo "(not from $pool: ${path##*/})" >&2
    done
    if [ "$got" -ne 1 ]; then
      echo "error: Launchpad's pool does not serve the lock's ${path##*/} (down, or the PPA has moved on to newer packages: native/scripts/toolchain_lock.py)" >&2
      return 1
    fi
    # the hosts that failed for this file are not asked for the files after it (their time-outs again, for each)
    while [ "${pools[0]}" != "$pool" ]; do pools=("${pools[@]:1}"); done
  done < <(rows "$kind")
  check "$kind" "$dir"
}

pack() { # <kind> <dir> <out>
  local kind=$1 dir=$2 out=$3
  check "$kind" "$dir"
  mkdir -p "$out"
  # (the same bytes whoever packs them: names in order, no owner, one date)
  tar -C "$dir" --sort=name --owner=0 --group=0 --numeric-owner --mtime='2000-01-01 00:00:00Z' \
    -cf "$out/gcc13-jammy-$kind.tar" .
  ls -l "$out/gcc13-jammy-$kind.tar"
}

# the files of <kind> from the repository's release into <dir> (which is empty)
from_release() { # <kind> <dir>
  local kind=$1 dir=$2 url
  url=$RELEASES/$(release)/gcc13-jammy-$kind.tar
  download "$url" "$dir.tar" || { rm -f "$dir.tar"; echo "(not from $url)" >&2; return 1; }
  # (files only, as the current user; what does not belong there fails the check)
  tar -C "$dir" --no-same-owner --no-same-permissions -xf "$dir.tar"
  rm -f "$dir.tar"
  check "$kind" "$dir"
}

TEMP=""
trap '[ -z "$TEMP" ] || rm -rf "$TEMP"' EXIT

install() { # [<dir>]
  local arch dir=${1:-} sudo=()
  arch=$(dpkg --print-architecture)
  if [ -z "$(rows "$arch")" ]; then
    echo "error: the lock names no packages for $arch (amd64 and arm64 only)" >&2
    exit 1
  fi
  [ "$(id -u)" -eq 0 ] || sudo=(sudo)
  if [ -n "$dir" ]; then
    check "$arch" "$dir"
    echo "GCC 13: the files of $dir"
  else
    TEMP=$(mktemp -d)
    dir=$TEMP/debs
    mkdir "$dir"
    if from_release "$arch" "$dir"; then
      echo "GCC 13: the repository's copy, release $(release)"
    elif [ "${GCC13_ONLY_RELEASE:-}" = 1 ]; then
      echo "error: the repository's copy (release $(release)) could not be had" >&2
      exit 1
    else
      echo "::warning::GCC 13: the repository's copy (release $(release)) could not be had; taking the files from Launchpad's pool"
      rm -rf "$dir"
      fetch "$arch" "$dir"
      echo "GCC 13: Launchpad's pool"
    fi
    # (apt reads them as the user _apt)
    chmod -R a+rX "$TEMP"
  fi
  dir=$(cd "$dir" && pwd -P)
  # the other packages they need (binutils, libc6-dev, libisl, libmpc, ...) are Ubuntu's own
  "${sudo[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$dir"/*.deb
  # exactly the lock's packages are the installed ones
  local deb name version have
  for deb in "$dir"/*.deb; do
    name=$(dpkg-deb -f "$deb" Package)
    version=$(dpkg-deb -f "$deb" Version)
    have=$(dpkg-query -W -f='${Version}' "$name" 2>/dev/null || true)
    if [ "$have" != "$version" ]; then
      echo "error: $name is installed as '${have:-nothing}', the lock's is $version" >&2
      exit 1
    fi
  done
  gcc-13 --version | head -1
  g++-13 --version | head -1
}

test_compiler() {
  local out
  out=$(mktemp -d)
  TEMP=$out
  g++-13 -std=c++23 -O2 -fopenmp -Wall -Wextra -Werror "$HERE/../toolchain/test.cpp" -o "$out/test"
  OMP_NUM_THREADS=2 "$out/test"
  # (the packages came as files: nothing of apt's points at Launchpad, so nothing is taken from it later either)
  if grep -rs '^[^#]*launchpad' /etc/apt/sources.list /etc/apt/sources.list.d; then
    echo "error: apt has a source at Launchpad (above)" >&2
    exit 1
  fi
}

case "${1:-}" in
  install) [ "$#" -le 2 ] || usage; install "${2:-}" ;;
  test) [ "$#" -eq 1 ] || usage; test_compiler ;;
  fetch) [ "$#" -eq 3 ] || usage; fetch "$2" "$3" ;;
  check) [ "$#" -eq 3 ] || usage; check "$2" "$3" ;;
  pack) [ "$#" -eq 4 ] || usage; pack "$2" "$3" "$4" ;;
  release) [ "$#" -eq 1 ] || usage; release ;;
  *) usage ;;
esac
