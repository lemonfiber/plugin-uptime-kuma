#!/usr/bin/env python3
"""Fail the day the core capability vocabulary exists, so inert claims get converted.

`F4-R1` has services declare what they can do as named capabilities so that
wiring can ask for a capability rather than name a service. `F4-R2` requires the
core vocabulary to be published, versioned and owned by lemonfiber. Nothing
publishes one yet.

So every capability this plugin claims is namespaced with its own id, which is
what `F4-R4` requires of a plugin's own — and a namespaced capability is
**inert until something asks for it**. Today nothing asks. The claims in
`plugin.toml` are therefore true, validated, and wire nothing.

That is a gap, not a design, and the danger in it is the same as every stand-in:
it goes on being quietly true after the thing it was standing in for arrives.
When lemonfiber publishes the vocabulary, a claim that should have become
`media.serve` stays `komga:comics-serve` and this plugin silently fails to be a
candidate for anything.

So this register points the other way. If a published core vocabulary appears,
this **fails**, and asks for the claims to be read against it.

  * no vocabulary published — the expected state. Reported, and the run continues.
  * one published — fail, naming where it was found.

Needs the network and a token; without either it reports that it could not ask,
which is unproven rather than clear (`F4-R7`).

Exit 0 = there is still nothing to claim from, 1 = there is.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
UPSTREAM = "lemonfiber/lemonfiber"

# Where a published vocabulary would be. The web API contract is already attached
# to every release as `web-api.contract.json`, and `ARCH-R78` has the same set
# served from `GET /api/capabilities` — so a published file would arrive the same
# way and be named for it.
VOCABULARY_NAMES = (
    "capabilities.json",
    "capability-vocabulary.json",
    "core-capabilities.json",
)


def claimed() -> list[str]:
    manifest = tomllib.loads((ROOT / "plugin.toml").read_text(encoding="utf-8"))
    return list(manifest["service"][0].get("provides", []))


def releases() -> tuple[list[dict], str | None]:
    if not (os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")):
        return [], "no token in the environment, so the forge was not asked"
    asked = subprocess.run(
        ["gh", "api", f"repos/{UPSTREAM}/releases", "--paginate"],
        capture_output=True, text=True, check=False,
    )
    if asked.returncode != 0:
        return [], f"the forge did not answer: {asked.stderr.strip()[:200]}"
    try:
        return json.loads(asked.stdout), None
    except ValueError as unreadable:
        return [], f"the forge's answer was not readable: {unreadable}"


def published(found: list[dict]) -> list[tuple[str, str]]:
    return [
        (release["tag_name"], asset["name"])
        for release in found
        for asset in release.get("assets", [])
        if asset["name"] in VOCABULARY_NAMES
    ]


def main() -> int:
    claims = claimed()
    found, unreachable = releases()

    if unreachable is not None:
        print(f"Could not ask whether a vocabulary is published: {unreachable}")
        print("Unproven, not clear. This register says nothing about this run.")
        return 0

    carrying = published(found)
    if carrying:
        for tag, name in carrying:
            print(f"::error::{UPSTREAM} {tag} publishes {name}")
        print(
            "\nThe core capability vocabulary exists. Every claim this plugin makes is "
            "namespaced and therefore inert:\n\n  "
            + "\n  ".join(claims)
            + "\n\nRead them against the published set. A claim that should be a core name and "
            "is not means this plugin is not a candidate for anything that asks — which is a "
            "plugin that installs and wires nothing, and looks exactly like one that works.",
            file=sys.stderr,
        )
        return 1

    print(
        f"No lemonfiber release publishes a core capability vocabulary, so the "
        f"{len(claims)} capability claim(s) here are namespaced and inert:"
    )
    for claim in claims:
        print(f"    {claim}")
    print(
        "That is F4-R2's gap and not this plugin's choice. The expected state, and this "
        "check is what ends it."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
