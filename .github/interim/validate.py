#!/usr/bin/env python3
"""Validate `plugin.toml` against the manifest contract — until a schema exists.

**This is CI harness, not plugin content.** A plugin is `plugin.toml`,
`proofs.toml` and `fixtures/`; nothing under `.github/` is part of what an
operator installs, and nothing here is ever run by lemonfiber (`F3-R6`).

`F3-R2` requires a manifest to be validated against a **published schema**, and
`ARCH-R92` requires that schema to be *generated from the types lemonfiber
deserialises* and published with every release. No release publishes one yet:
the plugin machinery is `0.16.0`, which is `planned`. So there is nothing to
validate against, and the honest options are to report this plugin unvalidated
or to check it against the contract by hand.

This does the second and says so. It is deliberately **not** a schema and must
never be published as one: `F10-R2` forbids a second, hand-maintained
description of the manifest format precisely because it can disagree with the
parser, and a plugin that validates in an author's editor and is refused on an
operator's machine is the failure that rule exists to prevent. When the schema
is published, `ci.yml` switches to it and this file is deleted — and until then
`schema_gate.py` is what fails the day it appears.

Every violation is reported in one pass, each naming its location (`ARCH-R94`,
`F1-R9`, `F10-R10`). Exit 0 = conforms, 1 = does not.
"""

from __future__ import annotations

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = "plugin.toml"

SUPPORTED_SCHEMA_VERSIONS = {1}

PLUGIN_REQUIRED = ("id", "name", "version", "description", "without_it", "upstream", "license", "forms")
SERVICE_REQUIRED = ("id", "name", "image", "digest", "tag", "criticality")
SERVICE_OPTIONAL = ("port", "bind", "health", "media_types", "takes_data")

# `stack.toml`'s vocabulary minus the one value a plugin may not assign itself
# (`ARCH-R97`). Named in full so a refusal can list what was available.
CRITICALITIES = ("core", "important", "enhancing", "optional")
FORBIDDEN_CRITICALITY = "critical"

BINDS = ("loopback", "lan")
HEALTH_KINDS = ("http", "tcp", "container")
MEDIA_TYPES = ("tv", "movies", "music", "books")

# The forms `lemonfiber-media-stack` declares. Read from the stack rather than
# listed here would be better and is what lemonfiber will do; this file has no
# stack to read.
STACK_FORMS = (
    "search", "dl", "hunt", "tv", "movies", "music", "books",
    "auto", "library", "full", "proxy",
)

# The fields `stack.toml` has that a plugin's service may not (`ARCH-R84`), each
# refused by name rather than ignored.
FORBIDDEN_SERVICE_FIELDS = ("grants", "depends_on", "host_managed", "profile", "api", "last_release")

DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:[-+].*)?$")


class Report:
    """Every violation, named with its location, reported in one pass."""

    def __init__(self) -> None:
        self.faults: list[str] = []

    def fail(self, where: str, what: str) -> None:
        self.faults.append(f"{where}: {what}")

    def check(self, ok: bool, where: str, what: str) -> bool:
        if not ok:
            self.fail(where, what)
        return ok


def validate_plugin(table: dict, report: Report) -> None:
    where = "[plugin]"
    for field in PLUGIN_REQUIRED:
        report.check(field in table, where, f"missing required field {field!r}")

    for field in ("id", "name", "version", "description", "without_it", "upstream", "license"):
        if field in table:
            report.check(
                isinstance(table[field], str) and table[field].strip(),
                f"{where}.{field}",
                "must be a non-empty string",
            )

    if isinstance(table.get("id"), str):
        report.check(
            table["id"] == table["id"].lower() and bool(re.fullmatch(r"[a-z0-9][a-z0-9-]*", table["id"])),
            f"{where}.id",
            f"{table['id']!r} must be lowercase, and the name it is installed and journalled under",
        )

    if isinstance(table.get("version"), str):
        report.check(SEMVER.match(table["version"]) is not None, f"{where}.version",
                     f"{table['version']!r} is not semver")

    forms = table.get("forms")
    if forms is not None and report.check(
        isinstance(forms, list) and forms,
        f"{where}.forms",
        "must be a non-empty array of forms the stack declares",
    ):
        for form in forms:
            report.check(
                form in STACK_FORMS,
                f"{where}.forms",
                f"{form!r} names no form the stack declares; available: {', '.join(STACK_FORMS)}",
            )


def validate_health(health: dict, report: Report) -> None:
    where = "[[service]].health"
    kind = health.get("kind")
    if not report.check(kind in HEALTH_KINDS, where,
                        f"kind {kind!r} is not one of {', '.join(HEALTH_KINDS)}"):
        return
    if kind == "http":
        report.check("path" in health, where, "an http health declares the path it asks for")
    if "timeout_s" in health:
        report.check(
            isinstance(health["timeout_s"], int) and health["timeout_s"] > 0,
            f"{where}.timeout_s",
            "must be a positive integer",
        )


def validate_service(service: dict, report: Report) -> None:
    where = "[[service]]"
    for field in SERVICE_REQUIRED:
        report.check(field in service, where, f"missing required field {field!r}")

    permitted = set(SERVICE_REQUIRED) | set(SERVICE_OPTIONAL)
    for field in sorted(set(service) - permitted):
        if field in FORBIDDEN_SERVICE_FIELDS:
            report.fail(
                f"{where}.{field}",
                f"{field!r} is a stack.toml field a plugin may not declare; "
                "lemonfiber fixes what a plugin's service may reach of the machine",
            )
        else:
            report.fail(
                f"{where}.{field}",
                f"{field!r} is outside the permitted set; permitted: {', '.join(sorted(permitted))}",
            )

    digest = service.get("digest")
    if digest is not None:
        report.check(
            isinstance(digest, str) and DIGEST.match(digest) is not None,
            f"{where}.digest",
            f"{digest!r} is not a well-formed sha256 digest",
        )

    image = service.get("image")
    if isinstance(image, str):
        report.check(
            "@" not in image and ":" not in image.rsplit("/", 1)[-1],
            f"{where}.image",
            f"{image!r} carries a tag or digest; the registry path is named on its own",
        )

    criticality = service.get("criticality")
    if criticality == FORBIDDEN_CRITICALITY:
        report.fail(
            f"{where}.criticality",
            f"a plugin may not declare {FORBIDDEN_CRITICALITY!r}; available: {', '.join(CRITICALITIES)}",
        )
    elif criticality is not None:
        report.check(
            criticality in CRITICALITIES,
            f"{where}.criticality",
            f"{criticality!r} is not one of {', '.join(CRITICALITIES)}",
        )

    if "port" in service:
        port = service["port"]
        report.check(isinstance(port, int) and 1 <= port <= 65535, f"{where}.port", f"invalid port {port!r}")
        report.check("bind" in service, f"{where}.bind",
                     "a declared port needs the tier it is published on")
    if "bind" in service:
        report.check(service["bind"] in BINDS, f"{where}.bind",
                     f"{service['bind']!r} is not one of {', '.join(BINDS)}")

    if "takes_data" in service:
        report.check(isinstance(service["takes_data"], bool), f"{where}.takes_data", "must be a boolean")

    for media_type in service.get("media_types", []):
        report.check(
            media_type in MEDIA_TYPES,
            f"{where}.media_types",
            f"{media_type!r} is not one of {', '.join(MEDIA_TYPES)}",
        )

    health = service.get("health")
    if health is not None:
        validate_health(health, report)


def validate(manifest: dict, report: Report) -> None:
    version = manifest.get("schema_version")
    report.check(
        version in SUPPORTED_SCHEMA_VERSIONS,
        MANIFEST,
        f"schema_version {version!r} is not one of {sorted(SUPPORTED_SCHEMA_VERSIONS)}",
    )

    for field in sorted(set(manifest) - {"schema_version", "plugin", "service", "requires"}):
        report.fail(f"{MANIFEST}.{field}", f"{field!r} is not a top-level table this contract defines")

    plugin = manifest.get("plugin")
    if report.check(isinstance(plugin, dict), MANIFEST, "no [plugin] table"):
        validate_plugin(plugin, report)

    services = manifest.get("service")
    if report.check(isinstance(services, list) and services, MANIFEST, "no [[service]] entry"):
        report.check(len(services) == 1, MANIFEST,
                     f"{len(services)} services declared; this schema version permits exactly one")
        for service in services:
            validate_service(service, report)

    requires = manifest.get("requires")
    if requires is not None:
        report.check(isinstance(requires, dict), "[requires]", "must be a table")
        capabilities = requires.get("capabilities", [])
        report.check(
            isinstance(capabilities, list) and all(isinstance(c, str) and c for c in capabilities),
            "[requires].capabilities",
            "must be an array of capability names",
        )
        report.check(
            "min_lemonfiber_version" not in requires and "version" not in requires,
            "[requires]",
            "a manifest carries no minimum lemonfiber version; an unmet requirement "
            "is refused by naming the capability",
        )


# Each case is a manifest broken one way, and the words its refusal must carry.
# A gate nobody has seen fail is a gate nobody knows the shape of.
BROKEN = (
    ("an image named by tag alone", 'digest', None, "missing required field 'digest'"),
    ("a digest that is not one", 'digest', "sha256:nope", "well-formed sha256 digest"),
    ("a criticality a plugin may not assign itself", 'criticality', "critical", "may not declare"),
    ("a kernel capability", 'grants', ["NET_ADMIN"], "a plugin may not declare"),
    ("a port with no tier", 'bind', None, "needs the tier"),
)


def self_test() -> int:
    """Every rule above refuses the shape it exists to refuse."""
    sound = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))

    report = Report()
    validate(sound, report)
    if report.faults:
        print("::error::self-test: the manifest in this repository was refused")
        for fault in report.faults:
            print(f"  {fault}")
        return 1
    print("  ok   the sound manifest is accepted")

    for label, field, value, expected in BROKEN:
        broken = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))
        if value is None:
            broken["service"][0].pop(field, None)
        else:
            broken["service"][0][field] = value
        report = Report()
        validate(broken, report)
        said = " ".join(report.faults)
        if expected not in said:
            print(f"::error::self-test: {label} was not refused by name — said: {said or '(nothing)'}")
            return 1
        print(f"  ok   {label} refused")

    # One pass, every violation: three faults at once, and all three named.
    broken = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))
    broken["service"][0].pop("digest")
    broken["service"][0]["criticality"] = "critical"
    broken["plugin"]["forms"] = ["nonesuch"]
    report = Report()
    validate(broken, report)
    if len(report.faults) < 3:
        print(f"::error::self-test: three violations reported as {len(report.faults)}; "
              "every one is named in one pass")
        return 1
    print("  ok   three violations reported in one pass, not the first of them")

    print("\nself-test passed.")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()

    path = ROOT / MANIFEST
    if not path.is_file():
        print(f"::error::{MANIFEST} is missing from the root of this plugin's source")
        return 1

    report = Report()
    try:
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as broken:
        print(f"::error file={MANIFEST}::not readable as TOML: {broken}")
        return 1

    validate(manifest, report)

    print(
        "Checked against 20-architecture/contracts/plugin-manifest.md, not against a\n"
        "published schema — none exists yet (ARCH-R92 is owed by 0.16.0, which is\n"
        "planned). This is a stand-in and the weaker answer of the two.\n"
    )

    if report.faults:
        for fault in report.faults:
            print(f"::error file={MANIFEST}::{fault}")
        print(f"\n{len(report.faults)} violation(s).", file=sys.stderr)
        return 1

    print(f"{MANIFEST} conforms to the contract, on every rule this stand-in can check.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
