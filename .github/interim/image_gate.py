#!/usr/bin/env python3
"""The declared digest is real, the tag beside it names it, and what signed it.

Two requirements about a plugin's image, and they answer differently:

`F3-R8` — an image MUST be named by an immutable digest, and one named by tag
alone MUST be refused. The published schema is what says the manifest carries
one; this checks the registry has it, and that the `tag` recorded beside it is
the tag that resolves to it today. A digest that does not correspond to its tag is
not detectable without reaching a registry, and the contract says so — but this
repository can reach one, so it does.

`F3-R25` — where a registry offers a signature it MUST be verified, and one that
does not verify MUST be refused; where none is offered the image MUST be
reported as **unproven**, and an unproven image MUST NOT be reported as
verified. So a missing signature is not a failure here. Calling it a pass would
be.

Needs the network and `docker`. Exit 0 = the digest resolves and any signature
offered verified, 1 = it does not.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]

# What the stack's own images are held to (`F2-R6`). A plugin is not bundled and
# is not held to it, but an author is better off knowing.
WANTED_ARCHES = {"amd64", "arm64"}


def services() -> list[dict]:
    """Every service this plugin installs.

    All of them rather than the first. A plugin may declare more than one, and a
    gate that asked about the first would leave the pin on the second unchecked —
    which is the half of the pair an author is least likely to have looked at.
    """
    manifest = tomllib.loads((ROOT / "plugin.toml").read_text(encoding="utf-8"))
    declared = manifest.get("service")
    return [one for one in declared if isinstance(one, dict)] if isinstance(declared, list) else []


def inspect(reference: str) -> tuple[dict | None, str]:
    asked = subprocess.run(
        ["docker", "manifest", "inspect", reference],
        capture_output=True, text=True, check=False,
    )
    if asked.returncode != 0:
        return None, asked.stderr.strip().splitlines()[-1] if asked.stderr.strip() else "not found"
    try:
        return json.loads(asked.stdout), ""
    except ValueError as unreadable:
        return None, str(unreadable)


def digest_of(reference: str) -> tuple[str | None, str]:
    """The digest a reference resolves to now, read from the registry."""
    asked = subprocess.run(
        ["docker", "buildx", "imagetools", "inspect", reference, "--format", "{{.Manifest.Digest}}"],
        capture_output=True, text=True, check=False,
    )
    if asked.returncode != 0:
        return None, asked.stderr.strip().splitlines()[-1] if asked.stderr.strip() else "not found"
    return asked.stdout.strip(), ""


def arches(index: dict) -> set[str]:
    return {
        entry["platform"]["architecture"]
        for entry in index.get("manifests", [])
        if entry.get("platform", {}).get("os") == "linux"
    }


def main() -> int:
    declared = services()
    if not declared:
        print("::error::plugin.toml declares no service, so there is no image to ask about")
        return 1
    faults = [fault for one in declared for fault in checked(one)]

    if faults:
        print(f"\n{len(faults)} problem(s).", file=sys.stderr)
        return 1
    print("\nEvery declared digest resolves, and each signature state is reported honestly.")
    return 0


def checked(declared: dict) -> list[str]:
    """One service's image, and everything wrong with how it is pinned."""
    image, digest, tag = declared["image"], declared["digest"], declared["tag"]
    by_digest = f"{image}@{digest}"
    by_tag = f"{image}:{tag}"
    faults: list[str] = []
    print(f"{declared['id']}:")

    index, why = inspect(by_digest)
    if index is None:
        print(f"::error::{by_digest} is not in the registry: {why}")
        faults.append(by_digest)
    else:
        found = arches(index)
        print(f"  ok   {by_digest}")
        print(f"       publishes {', '.join(sorted(found))}")
        missing = WANTED_ARCHES - found
        if missing:
            print(f"       note: no {', '.join(sorted(missing))} — the bundled stack requires both (F2-R6)")

    resolved, why = digest_of(by_tag)
    if resolved is None:
        print(f"  ????  {by_tag} could not be resolved: {why}")
        print("       so whether the tag still names this digest is unproven")
    elif resolved != digest:
        print(f"::error::{by_tag} now resolves to {resolved}")
        print(
            f"       and this manifest records {digest}. The digest is what runs, so nothing\n"
            "       is unsafe — but `tag` is the readable name for it and has stopped being one.",
        )
        faults.append(by_tag)
    else:
        print(f"  ok   {tag} still names this digest, so the readable name is the right one")

    # A cosign signature is published as a tag derived from the digest. Asking
    # for it needs no cosign and no keys: either the registry holds that tag or
    # it does not.
    signature = f"{image}:{digest.replace(':', '-')}.sig"
    found, _ = inspect(signature)
    if found is None:
        print("  ????  no signature offered for this image")
        print(
            "       Unproven, and reported as unproven. F3-R25 requires a signature to be\n"
            "       verified where one is offered and the image to be reported unproven where\n"
            "       none is — never reported as verified."
        )
    else:
        print(f"  ok   a signature is offered at {signature}")
        print("       ::error:: verifying it needs cosign and the publisher's identity, which this")
        print("       stand-in does not do. lemonfiber's own verification is what F3-R25 asks for.")
        faults.append(signature)

    return faults


if __name__ == "__main__":
    sys.exit(main())
