"""Fail when a lock file brings in a package version published less than N days ago.

The rule (no release younger than seven days) is enforced by the package managers when they
*resolve*. A lock file that is installed as-is (`--locked`, `--frozen-lockfile`,
`--require-hashes`) is never resolved, so a version written into it by hand, or by a tool run
without the setting, would go straight in. This checks the lock files themselves: every
package version that is in the working tree but not in the base revision is looked up in its
registry, and its publish time must be at least N days old.

Usage:
    python tools/check_release_age.py --base <git-rev> [--days 7] [--exceptions FILE] LOCKFILE...

Supported lock files, by name: Cargo.lock, pnpm-lock.yaml, *.lock / *.txt in pip
`requirements` format (name==version lines, as written by `uv pip compile`).

Exceptions file: one `ecosystem name version  # reason` per line (ecosystem is crates, npm
or pypi). Only for a critical fix whose exact version the owner approved.

Standard library only. Requests carry a generic User-Agent and nothing about the user.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

UA = {"User-Agent": "release-age-check/1.0 (CI; no contact)"}


def _get(url: str) -> dict:
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def _norm_pypi(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def parse(path: str, text: str) -> set[tuple[str, str, str]]:
    """Return {(ecosystem, name, version)} for registry packages in a lock file."""
    name = Path(path).name
    out: set[tuple[str, str, str]] = set()
    if name == "Cargo.lock":
        for block in text.split("[[package]]"):
            n = re.search(r'^name = "([^"]+)"', block, re.M)
            v = re.search(r'^version = "([^"]+)"', block, re.M)
            s = re.search(r'^source = "registry\+', block, re.M)
            if n and v and s:
                out.add(("crates", n.group(1), v.group(1)))
    elif name == "pnpm-lock.yaml":
        in_packages = False
        for line in text.splitlines():
            if re.match(r"^\S", line):
                in_packages = line.startswith("packages:")
                continue
            if not in_packages:
                continue
            m = re.match(r"^  '?((?:@[^/@\s']+/)?[^@\s':]+)@([^':(\s]+)'?:\s*$", line)
            if m:
                out.add(("npm", m.group(1), m.group(2)))
    else:
        for line in text.splitlines():
            m = re.match(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==([^\s;\\]+)", line)
            if m:
                out.add(("pypi", _norm_pypi(m.group(1)), m.group(2)))
    return out


def published(eco: str, name: str, version: str) -> dt.datetime:
    if eco == "crates":
        d = _get(f"https://crates.io/api/v1/crates/{name}/{version}")
        stamp = d["version"]["created_at"]
    elif eco == "npm":
        d = _get("https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@"))
        stamp = d["time"][version]
    else:
        d = _get(f"https://pypi.org/pypi/{name}/{version}/json")
        times = [u["upload_time_iso_8601"] for u in d["urls"]]
        if not times:
            raise LookupError("no files")
        stamp = min(times)
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
    ap.add_argument("--base", required=True, help="git revision to compare against")
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
        new = parse(lf, Path(lf).read_text(encoding="utf-8")) - parse(lf, at_base(a.base, lf))
        for eco, name, version in sorted(new):
            checked += 1
            try:
                when = published(eco, name, version)
            except (urllib.error.URLError, KeyError, LookupError, ValueError) as e:
                print(f"::error file={lf}::{eco} {name} {version}: publish time unknown ({e})")
                failed = True
                continue
            age = now - when
            tag = f"{eco} {name} {version}  published {when:%Y-%m-%d} ({age.days} days ago)"
            if age >= limit:
                print(f"ok    {tag}")
            elif (eco, name, version) in allowed:
                print(f"ALLOW {tag}  -- listed in {a.exceptions}")
            else:
                print(f"::error file={lf}::{tag}: younger than {a.days} days")
                failed = True
    print(f"{checked} new package version(s) checked against {a.base}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
