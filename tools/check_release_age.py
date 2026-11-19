"""Fail when a lock file brings in a package version published less than N days ago.

The rule (no release younger than seven days) is enforced by the package managers when they
*resolve*. A lock file that is installed as-is (`--locked`, `--frozen-lockfile`,
`--require-hashes`) is never resolved, so a version written into it by hand, or by a tool run
without the setting, would go straight in. This checks the lock files themselves: every
package version that is in the working tree but not in the base revision is looked up in its
registry, and its publish time must be at least N days old.

On PyPI a version is a set of files, and a file can be uploaded long after the others, so
there each hash in the lock is dated by the upload time of the file with that hash. A hash
added to a version that was already locked is checked too.

Usage:
    python tools/check_release_age.py (--base <git-rev> | --all) [--days 7] [--exceptions FILE] LOCKFILE...

--base checks what was added since that revision. Pick a base that cannot move past a failure:
a later commit that leaves the lock alone, or a tag put on the failing commit, must not turn a
failed check green (e.g. the latest published release, not the previous commit or any tag). --all checks every entry, for lock files small enough.

Supported lock files, by name: Cargo.lock, pnpm-lock.yaml, and anything else is read as a pip
`requirements` file with hashes (name==version lines and --hash=sha256: lines, as written by
`uv pip compile --generate-hashes`).

Exceptions file: one `ecosystem name version  # reason` per line (ecosystem is crates, npm
or pypi). Only for a critical fix whose exact version the owner approved.

Run it before installing anything from the lock: a check after the install comes after the
code has already run. Standard library only. Requests carry a generic User-Agent and
nothing about the user.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

UA = {"User-Agent": "release-age-check/1.0 (CI; no contact)"}

# (ecosystem, name, version, sha256). sha256 is "" for crates and npm, whose published
# versions are single immutable files.
Entry = tuple[str, str, str, str]

PIP_REQ = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==([^\s;\\]+)")
PIP_HASH = re.compile(r"--hash=sha256:([0-9a-f]{64})")
PNPM_KEY = re.compile(r"^  '?((?:@[^/@\s']+/)?[^@\s':]+)@([^':(\s]+)'?:\s*$")


_last_crates = 0.0


def _get(url: str) -> dict:
    # crates.io asks automated clients for at most one request per second
    global _last_crates
    if url.startswith("https://crates.io/"):
        wait = 1.0 - (time.monotonic() - _last_crates)
        if wait > 0:
            time.sleep(wait)
        _last_crates = time.monotonic()
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def _norm_pypi(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def parse(path: str, text: str) -> set[Entry]:
    """Return the registry packages a lock file names."""
    name = Path(path).name
    out: set[Entry] = set()
    if name == "Cargo.lock":
        for block in text.split("[[package]]"):
            n = re.search(r'^name = "([^"]+)"', block, re.M)
            v = re.search(r'^version = "([^"]+)"', block, re.M)
            s = re.search(r'^source = "registry\+', block, re.M)
            if n and v and s:
                out.add(("crates", n.group(1), v.group(1), ""))
    elif name == "pnpm-lock.yaml":
        in_packages = False
        for line in text.splitlines():
            if re.match(r"^\S", line):
                in_packages = line.startswith("packages:")
                continue
            m = PNPM_KEY.match(line) if in_packages else None
            if m:
                out.add(("npm", m.group(1), m.group(2), ""))
    else:
        current: tuple[str, str] | None = None
        for line in text.splitlines():
            m = PIP_REQ.match(line)
            if m:
                current = (_norm_pypi(m.group(1)), m.group(2))
            elif not line.startswith((" ", "\t")):
                current = None
            if current:
                # every hash on the line: pip accepts several after one requirement
                for h in PIP_HASH.finditer(line):
                    out.add(("pypi", current[0], current[1], h.group(1)))
    return out


_pypi: dict[tuple[str, str], dict] = {}


def published(eco: str, name: str, version: str, sha256: str) -> dt.datetime:
    if eco == "crates":
        stamp = _get(f"https://crates.io/api/v1/crates/{name}/{version}")["version"]["created_at"]
    elif eco == "npm":
        stamp = _get("https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@"))["time"][version]
    else:
        if (name, version) not in _pypi:
            _pypi[(name, version)] = _get(f"https://pypi.org/pypi/{name}/{version}/json")
        files = [u for u in _pypi[(name, version)]["urls"] if u["digests"]["sha256"] == sha256]
        if not files:
            raise LookupError(f"no file on PyPI with sha256 {sha256[:12]}")
        stamp = files[0]["upload_time_iso_8601"]
    return dt.datetime.fromisoformat(stamp.replace("Z", "+00:00"))


def at_base(base: str, path: str) -> str:
    r = subprocess.run(["git", "show", f"{base}:{path}"], capture_output=True, text=True, encoding="utf-8")
    return r.stdout if r.returncode == 0 else ""  # a lock file new in this change: everything is new


def load_exceptions(path: str | None) -> set[tuple[str, str, str]]:
    if not path or not Path(path).exists():
        return set()
    out = set()
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            eco, name, version = line.split()[:3]
            out.add((eco, _norm_pypi(name) if eco == "pypi" else name, version))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--base", help="git revision to compare against")
    g.add_argument("--all", action="store_true", help="check every entry, not only the new ones")
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--exceptions")
    ap.add_argument("lockfiles", nargs="+")
    a = ap.parse_args()

    allowed = load_exceptions(a.exceptions)
    now = dt.datetime.now(dt.timezone.utc)
    limit = dt.timedelta(days=a.days)
    failed = False
    checked = 0
    for lf in a.lockfiles:
        new = parse(lf, Path(lf).read_text(encoding="utf-8"))
        if not a.all:
            new -= parse(lf, at_base(a.base, lf))
        for eco, name, version, sha in sorted(new):
            checked += 1
            label = f"{eco} {name} {version}" + (f" [{sha[:12]}]" if sha else "")
            try:
                when = published(eco, name, version, sha)
            except (urllib.error.URLError, KeyError, LookupError, ValueError) as e:
                print(f"::error file={lf}::{label}: publish time unknown ({e})")
                failed = True
                continue
            age = now - when
            tag = f"{label}  published {when:%Y-%m-%d} ({age.days} days ago)"
            if age >= limit:
                print(f"ok    {tag}")
            elif (eco, name, version) in allowed:
                print(f"ALLOW {tag}  -- listed in {a.exceptions}")
            else:
                print(f"::error file={lf}::{tag}: younger than {a.days} days")
                failed = True
    against = "(all entries)" if a.all else f"against {a.base}"
    print(f"{checked} package version(s) / file(s) checked {against}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
