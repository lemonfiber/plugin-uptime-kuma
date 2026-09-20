#!/usr/bin/env python3
"""Ask the reader what this plugin's own recordings say about it.

**This is CI harness, not plugin content.** A plugin is `plugin.toml` and the
recordings in `fixtures/`. Nothing under `.github/` is installed, and lemonfiber
never runs any of it (`F3-R6`).

**Nothing here decides whether an assertion held.** `lemonfiber plugin claims`
reads the manifest, runs every bound probe, every `[[proof]]` and every
contributed check against the recording it names, and answers with no network,
no catalogue and no stack. This asks it and writes down what it said.

That is a change from what stood here, and it is the same change `validate.py`
made for the schema. A second evaluator written in Python is free to disagree
with the one an operator runs, and it did: the reader reaches a place in an
answer by JSON Pointer and the copy here looked a key up in the top level, so
the two would have reached opposite verdicts about every manifest that asserts
anything below the first level. `F10-R2` forbids a second description of the
*format*; this is the same objection one step along, about the *verdict*.

What is left here is the part that is genuinely this repository's: which reader
answered, how its answer reads on a terminal, and the record a release train
re-reads rather than re-running.

    --report FILE        write the record the release train reads

The reader is found at `$LEMONFIBER`, or as `lemonfiber` on the path. Without
one, every assertion is **unproven** — never a pass — because nothing has been
asked. `targets.toml` names the release that ships it; until that release, CI
builds it from source and `reader_gate.py` is what ends the arrangement.

Three verdicts, never two (`F3-R5`, `F4-R7`), and they are the reader's own
words rather than this file's reading of them.

Exit 0 = everything passed, 1 = one failed or could not be run.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = "plugin.toml"
TARGETS = "targets.toml"

# How long the reader is given. It reads files in a directory and asks nothing of
# anything, so a run that takes longer than this is a reader that is stuck rather
# than a reader that is busy.
READ_TIMEOUT_S = 120

PASSED, FAILED, UNPROVEN = "passed", "failed", "unproven"

# The words the reader's own report uses for a verdict, which are the words this
# writes down. Held to rather than translated: a report saying `fail` where the
# reader said `failed` is a third vocabulary for one fact.
VERDICTS = (PASSED, FAILED, UNPROVEN)


def reader() -> str | None:
    """The reader this run will ask, or nothing where there is none to ask."""
    named = os.environ.get("LEMONFIBER")
    if named:
        return named if pathlib.Path(named).is_file() else None
    return shutil.which("lemonfiber")


def asked(binary: str) -> tuple[dict | None, str | None]:
    """What the reader says about this plugin, or why it could not be asked.

    The failure paths are separated because they are different news. A reader
    that is not here is the expected state before the release that ships it; a
    reader that answered with something this cannot read is a change in the
    report's shape, which is a thing to be told rather than to guess past.
    """
    try:
        ran = subprocess.run(
            [binary, "--json", "plugin", "claims", str(ROOT)],
            capture_output=True, text=True, check=False, timeout=READ_TIMEOUT_S,
        )
    except (OSError, subprocess.SubprocessError) as unrunnable:
        return None, f"{binary} could not be run: {unrunnable}"
    # A non-zero exit is the reader's verdict, not a failure to ask: it exits
    # non-zero for a plugin it would not install, and the report on stdout is
    # exactly the one this is here to write down.
    if not ran.stdout.strip():
        why = ran.stderr.strip()[:400] or f"exit {ran.returncode}"
        return None, f"{binary} answered nothing: {why}"
    try:
        answered = json.loads(ran.stdout)
    except ValueError as unreadable:
        return None, f"{binary} answered with something that is not JSON: {unreadable}"
    if not isinstance(answered, dict):
        return None, f"{binary} answered with something that is not a report"
    return answered, None


def listed(value: object) -> list:
    """A value as the list it should be, or an empty one."""
    return value if isinstance(value, list) else []


def verdict(held: object) -> tuple[str, str]:
    """One verdict as the reader wrote it, and the sentence beside it.

    A shape this cannot read becomes `unproven` and says so. It is the reader's
    report and this file does not describe it; what it can insist on is that a
    verdict it cannot read is never counted as one that passed.
    """
    if not isinstance(held, dict):
        return UNPROVEN, "the reader's verdict was not in a shape this can read"
    outcome = held.get("outcome")
    if outcome == PASSED:
        return PASSED, "the recording answers what it declares"
    if outcome == FAILED:
        faults = [str(one) for one in listed(held.get("faults"))]
        return FAILED, "; ".join(faults) or "refuted, and the reader said no more"
    if outcome == UNPROVEN:
        return UNPROVEN, str(held.get("why") or "the reader said no more")
    return UNPROVEN, f"the reader answered {outcome!r}, which is not one of: {', '.join(VERDICTS)}"


def assertions(report: dict) -> list[dict]:
    """Every assertion the reader reached a verdict about, in one list.

    Flattened because a run reports one line each and a release train counts
    them; which block each came out of rides along as `kind`, the way it did
    when this file ran them itself.
    """
    found: list[dict] = []
    for claiming in listed(report.get("capabilities")):
        if not isinstance(claiming, dict):
            continue
        for ran in listed(claiming.get("probes")):
            if not isinstance(ran, dict):
                continue
            found.append({
                "kind": "probe",
                "id": f"{claiming.get('name')}/{ran.get('probe')}",
                "verdict": ran.get("verdict"),
            })
    for kind, held in (("proof", "proofs"), ("check", "checks")):
        for one in listed(report.get(held)):
            if not isinstance(one, dict):
                continue
            found.append({"kind": kind, "id": one.get("id"), "verdict": one.get("verdict")})
    return found


def unasked(why: str) -> list[dict]:
    """Every assertion the manifest declares, with nothing established about it.

    Read off the manifest rather than off a report there is none of, so a run
    with no reader still names what went unasked. It does not decide anything
    about them — that is the whole point of the file this is — it says which
    questions were not put.
    """
    try:
        manifest = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        # A manifest that will not read is `validate.py`'s news to break, and a
        # traceback here would be a gate nobody can tell apart from one.
        return []
    found: list[dict] = []
    for proof in listed(manifest.get("proof")):
        if isinstance(proof, dict):
            found.append({"kind": "proof", "id": proof.get("id"), "verdict": None})
    for claim in listed(manifest.get("claim")):
        if not isinstance(claim, dict):
            continue
        for probe in listed(claim.get("probe")):
            if isinstance(probe, dict):
                found.append({
                    "kind": "probe",
                    "id": f"{claim.get('capability')}/{probe.get('id')}",
                    "verdict": None,
                })
    for entry in listed(manifest.get("contribution")):
        if isinstance(entry, dict) and entry.get("at") == "doctor.check":
            found.append({"kind": "check", "id": entry.get("id"), "verdict": None})
    for one in found:
        one["why"] = why
    return found


def targeted() -> str | None:
    path = ROOT / TARGETS
    if not path.is_file():
        return None
    return tomllib.loads(path.read_text(encoding="utf-8")).get("lemonfiber")


def write_report(path: str, against: str, verdicts: list[tuple[dict, str, str]]) -> None:
    """The record a release train re-reads, rather than re-running.

    Held to the release this plugin targets rather than to whichever one is being
    cut: a report that named the version reading it would agree with every reader
    and describe none of them.
    """
    document = {
        "lemonfiber": targeted(),
        "against": against,
        "proofs": [
            {"id": one.get("id"), "kind": one.get("kind"), "outcome": outcome, "detail": detail}
            for one, outcome, detail in verdicts
        ],
    }
    pathlib.Path(path).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", metavar="FILE",
                        help="write the record a release train reads")
    args = parser.parse_args()

    if not (ROOT / MANIFEST).is_file():
        print(f"::error::{MANIFEST} is missing")
        return 1

    binary = reader()
    report, why = asked(binary) if binary else (None, "no lemonfiber on the path, and "
                                                      "$LEMONFIBER names none")
    if report is None:
        print(f"Nothing was asked: {why}\n")
        found = unasked(why or "unasked")
        verdicts = [(one, UNPROVEN, str(one["why"])) for one in found]
        against = "nothing"
    else:
        against = str(report.get("against") or "an evidence this cannot name")
        print(
            f"Read by {binary}, against {against}. These are verdicts about what this\n"
            "plugin declares, reached by the reader an operator runs — and against\n"
            "recordings rather than a running service, which is the weaker claim and is\n"
            "reported as the weaker one.\n"
        )
        for refusal in listed(report.get("refusals")):
            if isinstance(refusal, dict):
                print(f"  note  the reader refuses this manifest: "
                      f"{refusal.get('location')} — {refusal.get('message')}")
        found = assertions(report)
        verdicts = [(one, *verdict(one.get("verdict"))) for one in found]

    if not found:
        print(f"::error::{MANIFEST} declares nothing to run, and a plugin whose proofs do not "
              "pass is not installed")
        return 1

    for one, outcome, detail in verdicts:
        mark = {PASSED: "  ok  ", FAILED: "  FAIL", UNPROVEN: "  ????"}[outcome]
        print(f"{mark} {one['kind']:6} {one.get('id')!s:<34} {detail}")

    if args.report:
        write_report(args.report, against, verdicts)

    counted = [outcome for _, outcome, _ in verdicts]
    failed, unproven = counted.count(FAILED), counted.count(UNPROVEN)
    print(f"\n{counted.count(PASSED)} passed, {failed} failed, {unproven} could not be run.")

    if unproven:
        print(
            "\nSomething could not be run, and is unproven rather than passed.",
            file=sys.stderr,
        )
    return 1 if failed or unproven else 0


if __name__ == "__main__":
    sys.exit(main())
