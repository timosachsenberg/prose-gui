#!/usr/bin/env python3
"""Writes native/toolchain/jammy-gcc13.lock: the files of GCC 13 for the Linux builds, pinned.

    toolchain_lock.py [--release TAG] [--out FILE]

The Linux packages are built on Ubuntu 22.04 (glibc 2.35), whose own archive ends at GCC 12; OpenMS needs GCC 13
(C++23), which Ubuntu builds for 22.04 only in the toolchain PPA (ppa:ubuntu-toolchain-r/test, on Launchpad).
The lock names the package files `apt-get install gcc-13 g++-13` takes from that PPA on GitHub's ubuntu-22.04
runners (GCC 13 itself, and the newer runtime libraries it asks for, which the PPA builds from its newest GCC),
for amd64 and arm64, and the source packages they were built from, each with its size and SHA-256.
native/scripts/gcc13.sh installs exactly these files, from the copy of them the repository keeps as a release
(.github/workflows/toolchain.yml makes it), so that a build does not need Launchpad.

The sizes and hashes come from the PPA's index, whose signature (InRelease) is checked against the PPA's key,
native/toolchain/ubuntu-toolchain-r-test.asc (fingerprint below; Launchpad shows it on the PPA's page). Run it
to move to newer packages of the PPA (it needs gpgv and network access to Launchpad), then run the "Toolchain
packages" workflow, which makes the release the new lock names. A release is never changed: a new lock gets a
new release name, by default the date of the PPA's index (--release gives another, for a lock of other packages
from the same index).
"""

import argparse
import email.utils
import hashlib
import lzma
import os
import re
import subprocess
import sys
import tempfile
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLCHAIN = os.path.join(HERE, "..", "toolchain")
KEY = os.path.join(TOOLCHAIN, "ubuntu-toolchain-r-test.asc")
FINGERPRINT = "C8EC952E2A0E1FBDC5090F6A2C277A0A352154E5"
# the PPA's two hosts: Launchpad's current one and its older front end, which served while the other did not
BASES = [
    "https://ppa.launchpadcontent.net/ubuntu-toolchain-r/test/ubuntu",
    "http://ppa.launchpad.net/ubuntu-toolchain-r/test/ubuntu",
]
SUITE = "jammy"
ARCHS = {"amd64": "x86-64-linux-gnu", "arm64": "aarch64-linux-gnu"}
# What `apt-get install gcc-13 g++-13` takes from the PPA on the runner images ({triplet}: ARCHS). The runtime
# libraries are the PPA's newest (GCC 13's packages ask for at least their own version of them, and apt takes
# the candidate, which is the PPA's).
PACKAGES = [
    "gcc-13", "g++-13", "cpp-13", "gcc-13-base", "gcc-13-{triplet}", "g++-13-{triplet}", "cpp-13-{triplet}",
    "libgcc-13-dev", "libstdc++-13-dev",
    "libgcc-s1", "libstdc++6", "libgomp1", "libitm1", "libatomic1", "libcc1-0",
    "libasan8", "liblsan0", "libtsan2", "libubsan1", "libhwasan0", "libquadmath0",
]
# (no such library on that architecture)
ABSENT = {"arm64": {"libquadmath0"}}


def fetch(path, into):
    """The PPA's file `path`, from the first host that serves it."""
    problems = []
    for base in BASES:
        try:
            with urllib.request.urlopen(f"{base}/{path}", timeout=120) as response:
                data = response.read()
            with open(into, "wb") as out:
                out.write(data)
            return
        except OSError as e:
            problems.append(f"{base}: {e}")
    sys.exit(f"error: {path} could not be read from the PPA:\n  " + "\n  ".join(problems))


def paragraphs(text):
    """The paragraphs of a Debian control file, each a dict of its fields (continuation lines joined with newlines)."""
    for block in text.split("\n\n"):
        fields, name = {}, None
        for line in block.split("\n"):
            if not line:
                continue
            if line[0] in " \t" and name:
                fields[name] += "\n" + line.strip()
            else:
                name, _, value = line.partition(":")
                fields[name] = value.strip()
        if fields:
            yield fields


def signed_index(folder):
    """The text of the PPA's InRelease, once gpgv has found the PPA key's signature on it good."""
    listed = subprocess.run(["gpg", "--show-keys", "--with-colons", KEY], capture_output=True, text=True, check=True)
    fingerprints = [line.split(":")[9] for line in listed.stdout.splitlines() if line.startswith("fpr:")]
    if FINGERPRINT not in fingerprints:
        sys.exit(f"error: {KEY} is not the key {FINGERPRINT} (it holds {', '.join(fingerprints) or 'no key'})")
    keyring = os.path.join(folder, "key.gpg")
    subprocess.run(["gpg", "--batch", "--yes", "--dearmor", "--output", keyring, KEY], check=True)
    inrelease, text = os.path.join(folder, "InRelease"), os.path.join(folder, "Release")
    fetch(f"dists/{SUITE}/InRelease", inrelease)
    # (gpgv's exit status is no use here: Launchpad signs with the PPA's older key as well, which is not in the
    # keyring, and gpgv counts a signature it cannot check as a failure)
    status = subprocess.run(
        ["gpgv", "--status-fd", "1", "--keyring", keyring, "--output", text, inrelease],
        capture_output=True, text=True, check=False,
    )
    if not re.search(rf"^\[GNUPG:\] VALIDSIG {FINGERPRINT} ", status.stdout, re.M):
        sys.exit(f"error: the PPA's InRelease has no good signature of {FINGERPRINT}:\n{status.stdout}{status.stderr}")
    with open(text, encoding="utf-8") as signed:
        return signed.read()


def index(release, folder, path):
    """An index file of the PPA (xz), checked against the SHA-256 the signed InRelease gives for it; its text."""
    wanted = None
    section = False
    for line in release.split("\n"):
        if line.startswith("SHA256:"):
            section = True
        elif line and line[0] not in " \t":
            section = False
        elif section:
            digest, _size, name = line.split()
            if name == path:
                wanted = digest
    if wanted is None:
        sys.exit(f"error: the PPA's InRelease does not list {path}")
    into = os.path.join(folder, path.replace("/", "_"))
    fetch(f"dists/{SUITE}/{path}", into)
    with open(into, "rb") as file:
        data = file.read()
    if hashlib.sha256(data).hexdigest() != wanted:
        sys.exit(f"error: {path} is not the file the PPA's InRelease lists (SHA-256)")
    return lzma.decompress(data).decode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--release", help="the name of the release that will hold the files (default: toolchain-jammy-gcc13-<date of the PPA's index>)")
    parser.add_argument("--out", default=os.path.join(TOOLCHAIN, "jammy-gcc13.lock"))
    args = parser.parse_args()

    with tempfile.TemporaryDirectory() as folder:
        release = signed_index(folder)
        fields = next(paragraphs(release))
        date = email.utils.parsedate_to_datetime(fields["Date"].replace("UTC", "+0000"))
        rows = []  # (kind, sha256, size, path)
        sources = {}  # (source package, version) -> the binary packages built from it
        for arch, triplet in ARCHS.items():
            entries = {}
            for entry in paragraphs(index(release, folder, f"main/binary-{arch}/Packages.xz")):
                entries.setdefault(entry["Package"], []).append(entry)
            names = [name.format(triplet=triplet) for name in PACKAGES]
            names = [name for name in names if name not in ABSENT.get(arch, ())]
            for name in names:
                found = entries.get(name, [])
                if len(found) != 1:
                    sys.exit(f"error: the PPA has {len(found)} packages {name} for {arch}, not one")
                entry = found[0]
                # the runtime libraries need the base package of the GCC they were built from (gcc-16-base, ...)
                for base in re.findall(r"\bgcc-\d+-base\b", entry.get("Depends", "") + entry.get("Pre-Depends", "")):
                    if base not in names:
                        names.append(base)
                rows.append((arch, entry["SHA256"], int(entry["Size"]), entry["Filename"]))
                # Source: "gcc-13" or "gcc-13 (13.4.0-6ubuntu1)" when the versions differ; absent when it is the package itself
                source = re.fullmatch(r"(\S+)(?: \((\S+)\))?", entry.get("Source", name))
                sources.setdefault((source.group(1), source.group(2) or entry["Version"]), []).append(name)
        known = {
            (entry["Package"], entry["Version"]): entry
            for entry in paragraphs(index(release, folder, "main/source/Sources.xz"))
        }
        for key in sorted(sources):
            if key not in known:
                sys.exit(f"error: the PPA has no source package {key[0]} {key[1]} (of {', '.join(sorted(set(sources[key])))})")
            entry = known[key]
            for line in entry["Checksums-Sha256"].strip().split("\n"):
                digest, size, name = line.split()
                rows.append(("source", digest, int(size), f"{entry['Directory']}/{name}"))

    tag = args.release or f"toolchain-jammy-gcc13-{date:%Y%m%d}"
    if not re.fullmatch(r"toolchain-jammy-gcc13-[0-9A-Za-z.-]+", tag):
        sys.exit(f"error: '{tag}' is no release name like toolchain-jammy-gcc13-20260317")
    order = {"amd64": 0, "arm64": 1, "source": 2}
    rows.sort(key=lambda row: (order[row[0]], row[3]))
    versions = ", ".join(f"{name} {version}" for name, version in sorted(sources))
    lines = [
        "# GCC 13 for the Linux builds (Ubuntu 22.04), pinned: the package files that `apt-get install gcc-13 g++-13`",
        "# takes from ppa:ubuntu-toolchain-r/test on GitHub's ubuntu-22.04 runners, for amd64 and arm64, and the source",
        "# packages they were built from. native/scripts/gcc13.sh installs exactly these files, from the release named",
        "# below (the repository's own copy, made by .github/workflows/toolchain.yml), or else from Launchpad's pool.",
        "#",
        f"# Written by native/scripts/toolchain_lock.py from the PPA's index for {SUITE} of {date:%Y-%m-%d} (InRelease,",
        f"# signed by the PPA's key {FINGERPRINT}):",
        f"# {versions}.",
        "# Do not edit: run the script, then the \"Toolchain packages\" workflow (native/README.md, \"GCC 13\").",
        "#",
        "# release <name of the release that holds the files>",
        "# <amd64|arm64|source> <SHA-256> <size in bytes> <path below the PPA's .../ubuntu/>",
        f"release {tag}",
    ]
    lines += [f"{kind} {digest} {size} {path}" for kind, digest, size, path in rows]
    with open(args.out, "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")
    for kind in order:
        mine = [row for row in rows if row[0] == kind]
        print(f"{kind}: {len(mine)} files, {sum(row[2] for row in mine) / 1e6:.1f} MB")
    print(f"wrote {os.path.relpath(args.out)} (release {tag}; {versions})")


if __name__ == "__main__":
    main()
