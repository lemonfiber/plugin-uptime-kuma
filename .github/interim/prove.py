#!/usr/bin/env python3
"""Run this plugin's declared proofs — until `lemonfiber plugin prove` exists.

**This is CI harness, not plugin content.** A plugin is `plugin.toml` and the
recordings in `fixtures/`. Nothing under `.github/` is installed, and lemonfiber
never runs any of it (`F3-R6`).

`F3-R3` says a plugin's declared proofs run in the existing verification engine,
the same way the bundled ones do. That engine is in lemonfiber, and the verbs
that reach it are owed by `0.16.0`, which is `planned`. This stands in, and is
written to be thrown away: when `lemonfiber plugin prove .` exists, `ci.yml`
calls it and this file goes.

Two modes, and the difference is reported rather than blurred (`F10-R6`):

    --against fixtures   the recorded responses in fixtures/, which is what lets
                         this run with no live Komga anywhere (`F10-R4`)
    --against <base-url> a real instance

Three verdicts, never two (`F3-R5`, `F4-R7`): a proof that passed, one that
failed, and one that could not be run at all — which is reported as unproven and
is never counted as a pass. That distinction is the whole point of the exercise.

Exit 0 = every proof passed, 1 = one failed or could not be run.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = "plugin.toml"
ATTEMPT_TIMEOUT_S = 10

PASS, FAIL, UNPROVEN = "pass", "fail", "unproven"

TYPES = {"bool": bool, "int": int, "str": str, "list": list, "dict": dict}


def answer_from_fixture(proof: dict) -> tuple[dict, str | None]:
    """The recorded response this proof names, and why it could not be read."""
    named = proof.get("fixture")
    if not named:
        return {}, "declares no fixture, so there is nothing to run it against"
    path = ROOT / named
    if not path.is_file():
        return {}, f"names {named}, which is not in this source"
    try:
        recorded = json.loads(path.read_text(encoding="utf-8"))
    except ValueError as unreadable:
        return {}, f"{named} is not readable as JSON: {unreadable}"

    asked = proof.get("request", {})
    recorded_request = recorded.get("request", {})
    if recorded_request != asked:
        return {}, (
            f"{named} records {recorded_request.get('method')} {recorded_request.get('path')}, "
            f"and this proof asks {asked.get('method')} {asked.get('path')}"
        )
    return recorded.get("response", {}), None


def answer_from_service(proof: dict, base: str) -> tuple[dict, str | None]:
    """One request to a running instance, and what came back."""
    asked = proof.get("request", {})
    if asked.get("method", "GET") != "GET":
        return {}, f"only GET is implemented here, and this asks {asked.get('method')}"
    url = base.rstrip("/") + asked.get("path", "/")
    request = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=ATTEMPT_TIMEOUT_S) as reply:
            status, headers, body = reply.status, dict(reply.headers), reply.read()
    except urllib.error.HTTPError as answered:
        status, headers, body = answered.code, dict(answered.headers), answered.read()
    except OSError as unreachable:
        return {}, f"{url} could not be reached: {type(unreachable).__name__}: {unreachable}"
    try:
        parsed = json.loads(body) if body else None
    except ValueError:
        parsed = None
    answer = {
        "status": status,
        "headers": {"content-type": headers.get("Content-Type", "")},
        "json": parsed,
    }
    if parsed is None:
        answer["body_starts_with"] = body.decode("utf-8", "replace")[:200]
    return answer, None


def judge(proof: dict, answer: dict) -> list[str]:
    """Every way this answer is not the one the proof declared."""
    expect = proof.get("expect", {})
    faults: list[str] = []

    if "status" in expect and answer.get("status") != expect["status"]:
        faults.append(f"status {answer.get('status')!r}, and the proof declares {expect['status']!r}")

    body = answer.get("json")

    if "json" in expect:
        wanted = expect["json"]
        if not isinstance(body, dict):
            faults.append(f"the body is not a JSON object: {body!r}")
        else:
            for key, value in wanted.items():
                if body.get(key) != value:
                    faults.append(f"{key} is {body.get(key)!r}, and the proof declares {value!r}")

    for key in expect.get("json_has_keys", []):
        if not isinstance(body, dict) or key not in body:
            faults.append(f"the body carries no {key!r}")

    if "content_type" in expect:
        got = answer.get("headers", {}).get("content-type", "")
        if expect["content_type"] not in got:
            faults.append(
                f"the content type is {got!r}, and the proof declares it carries "
                f"{expect['content_type']!r}"
            )

    if "body_starts_with" in expect:
        got = answer.get("body_starts_with", "")
        if not got.startswith(expect["body_starts_with"]):
            faults.append(
                f"the body starts {got[:40]!r}, and the proof declares it starts "
                f"{expect['body_starts_with']!r}"
            )

    if expect.get("json_is_absent") and body is not None:
        faults.append(f"the body parsed as JSON, and the proof declares it does not: {body!r}")

    for key, name in expect.get("json_types", {}).items():
        if not isinstance(body, dict) or key not in body:
            continue
        wanted = TYPES.get(name)
        if wanted is None:
            faults.append(f"the proof names a type this runner does not know: {name!r}")
        elif not isinstance(body[key], wanted):
            faults.append(f"{key} is {type(body[key]).__name__}, and the proof declares {name}")

    return faults


def run(proof: dict, against: str) -> tuple[str, str]:
    if against == "fixtures":
        answer, unrunnable = answer_from_fixture(proof)
    else:
        answer, unrunnable = answer_from_service(proof, against)

    if unrunnable is not None:
        return UNPROVEN, unrunnable
    if not proof.get("expect"):
        return UNPROVEN, "declares nothing it expects, so nothing about it can be decided"

    faults = judge(proof, answer)
    if faults:
        return FAIL, "; ".join(faults)
    return PASS, f"HTTP {answer.get('status')}, and the body the proof declares"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--against",
        default="fixtures",
        help="'fixtures' for the recorded responses, or a base URL for a running instance",
    )
    args = parser.parse_args()

    path = ROOT / MANIFEST
    if not path.is_file():
        print(f"::error::{MANIFEST} is missing")
        return 1
    declared = tomllib.loads(path.read_text(encoding="utf-8")).get("proof", [])
    if not declared:
        print(f"::error::{MANIFEST} declares no proof, and a plugin whose proofs do not pass is not installed")
        return 1

    if args.against == "fixtures":
        print(
            "Run against recorded responses. These are proofs about what this plugin\n"
            "declares, not about a running service — which is a weaker claim, and is\n"
            "reported as the weaker one.\n"
        )
    else:
        print(f"Run against {args.against}.\n")

    verdicts = []
    for proof in declared:
        verdict, detail = run(proof, args.against)
        verdicts.append(verdict)
        mark = {PASS: "  ok  ", FAIL: "  FAIL", UNPROVEN: "  ????"}[verdict]
        print(f"{mark} {proof.get('id', '<unnamed>'):<28} {detail}")

    failed = verdicts.count(FAIL)
    unproven = verdicts.count(UNPROVEN)
    print(f"\n{verdicts.count(PASS)} passed, {failed} failed, {unproven} could not be run.")

    if unproven:
        print(
            "\nA proof that could not be run is unproven, and is not a proof that passed.",
            file=sys.stderr,
        )
    return 1 if failed or unproven else 0


if __name__ == "__main__":
    sys.exit(main())
