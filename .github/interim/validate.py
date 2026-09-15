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
SERVICE_OPTIONAL = (
    "port", "bind", "health", "media_types", "takes_data", "provides", "config_path",
)
WIRING_PERMITTED = ("hostname", "dashboard_group")
PROOF_REQUIRED = ("id", "title", "request", "expect", "why")
PROOF_OPTIONAL = ("fixture",)
# What an `expect` may constrain. Anything else is a proof this runner would
# silently not check, which is worse than one that fails.
EXPECT_PERMITTED = (
    "status", "json", "json_has_keys", "json_types", "json_is_absent",
    "content_type", "body_starts_with",
)
# An expectation that says something about the body rather than the network path.
# At least one proof must carry one (ARCH-R105).
BODY_CONSTRAINTS = frozenset(EXPECT_PERMITTED) - {"status"}

# `stack.toml`'s vocabulary minus the one value a plugin may not assign itself
# (`ARCH-R97`). Named in full so a refusal can list what was available.
CRITICALITIES = ("core", "important", "enhancing", "optional")
FORBIDDEN_CRITICALITY = "critical"

BINDS = ("loopback", "lan")
HEALTH_KINDS = ("http", "tcp", "container")
MEDIA_TYPES = ("tv", "movies", "music", "books", "comics")

# The bundled dashboard's groups, which is what a plugin's entry joins.
DASHBOARD_GROUPS = ("Watch", "Library", "Automation", "Acquisition")

# A single DNS label: what may go in front of the operator's own domain.
DNS_LABEL = re.compile(r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$")

# The forms `lemonfiber-media-stack` declares. Read from the stack rather than
# listed here would be better and is what lemonfiber will do; this file has no
# stack to read.
STACK_FORMS = (
    "search", "dl", "hunt", "tv", "movies", "music", "books",
    "auto", "library", "full", "proxy",
)

# The fields `stack.toml` has that a plugin's service may not (`ARCH-R84`), each
# refused by name rather than ignored.
FORBIDDEN_SERVICE_FIELDS = (
    "grants", "depends_on", "host_managed", "profile", "api", "last_release", "environment",
)

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


def validate_config_path(service: dict, report: Report) -> None:
    """Where the one configuration directory lands inside the container (ARCH-R100).

    The mount set is lemonfiber's and unchanged. What is checked here is that the
    target names one place, is a place, and is not the library — a path beneath
    the data root would be a second mount over somebody's media wearing a
    different name.
    """
    where = "[[service]].config_path"
    declared = service.get("config_path")
    if declared is None:
        return
    if not report.check(isinstance(declared, str) and declared, where, "must be a non-empty string"):
        return
    report.check(declared.startswith("/"), where, f"{declared!r} is not an absolute path")
    report.check(declared != "/", where, "the container root is not a configuration directory")
    report.check(".." not in declared.split("/"), where, f"{declared!r} walks out of itself")
    report.check("$" not in declared, where, f"{declared!r} interpolates; the path is data, not a template")
    report.check(
        declared != "/data" and not declared.startswith("/data/"),
        where,
        f"{declared!r} is inside the data root, which would be a second mount over the library",
    )


def validate_provides(service: dict, report: Report) -> None:
    """Capabilities claimed (F4-R1, ARCH-R102).

    A core name comes from the published vocabulary. There is not one yet — F4-R2
    owes it — so the only claim this can accept today is one namespaced with the
    plugin's own id, which is exactly what F4-R4 requires of a plugin's own.
    `vocabulary_gate.py` is what says so when that changes.
    """
    where = "[[service]].provides"
    claims = service.get("provides")
    if claims is None:
        return
    if not report.check(isinstance(claims, list), where, "must be an array of capability names"):
        return
    prefix = f"{service.get('id', '')}:"
    for claim in claims:
        if not report.check(isinstance(claim, str) and claim, where, f"{claim!r} is not a capability name"):
            continue
        if claim.startswith(prefix):
            continue
        report.fail(
            where,
            f"{claim!r} is neither namespaced with this plugin's id ({prefix}…) nor a name in the "
            "published core vocabulary — of which there is none yet, so a namespaced name is the "
            "only claim a plugin can make today",
        )


def validate_wiring(wiring: dict, service: dict, report: Report) -> None:
    """How the stack's own proxy and dashboard reach it (F3-R31, ARCH-R103, ARCH-R104)."""
    where = "[wiring]"
    for field in sorted(set(wiring) - set(WIRING_PERMITTED)):
        report.fail(
            f"{where}.{field}",
            f"{field!r} is outside the permitted set; permitted: {', '.join(WIRING_PERMITTED)}. "
            "A plugin declares which label and which group, never a stanza or an entry.",
        )

    hostname = wiring.get("hostname")
    if hostname is not None:
        report.check(
            isinstance(hostname, str) and DNS_LABEL.match(hostname) is not None,
            f"{where}.hostname",
            f"{hostname!r} is not a single DNS label; it goes in front of the operator's own "
            "domain and may not be a name, an address or a port",
        )
        report.check(
            service.get("bind") == "lan",
            f"{where}.hostname",
            f"this service binds {service.get('bind')!r}, and only a lan service is proxied — "
            "the tier decides whether it is reachable by name, not the plugin",
        )

    group = wiring.get("dashboard_group")
    if group is not None:
        report.check(
            group in DASHBOARD_GROUPS,
            f"{where}.dashboard_group",
            f"{group!r} is not one of {', '.join(DASHBOARD_GROUPS)}",
        )


def validate_proofs(proofs: list, report: Report) -> None:
    """What must hold before it is installed (F3-R1, ARCH-R105)."""
    if not report.check(isinstance(proofs, list) and proofs, "[[proof]]", "a plugin declares at least one proof"):
        return

    seen: set[str] = set()
    constrains_a_body = False

    for index, proof in enumerate(proofs):
        name = proof.get("id") or f"#{index + 1}"
        where = f"[[proof]] {name}"
        for field in PROOF_REQUIRED:
            report.check(field in proof, where, f"missing required field {field!r}")
        for field in sorted(set(proof) - set(PROOF_REQUIRED) - set(PROOF_OPTIONAL)):
            report.fail(f"{where}.{field}", f"{field!r} is outside the permitted set")

        if isinstance(proof.get("id"), str):
            report.check(proof["id"] not in seen, where, f"{proof['id']!r} is declared twice")
            seen.add(proof["id"])

        request = proof.get("request")
        if isinstance(request, dict):
            report.check("method" in request and "path" in request, f"{where}.request",
                         "names the method and the path it asks for")
            report.check(
                str(request.get("path", "")).startswith("/"),
                f"{where}.request.path",
                f"{request.get('path')!r} is not a path on the service",
            )

        expect = proof.get("expect")
        if isinstance(expect, dict):
            report.check(bool(expect), f"{where}.expect", "declares nothing, so nothing can be decided")
            for field in sorted(set(expect) - set(EXPECT_PERMITTED)):
                report.fail(
                    f"{where}.expect.{field}",
                    f"{field!r} is not something this can check; permitted: "
                    f"{', '.join(EXPECT_PERMITTED)}",
                )
            if set(expect) & BODY_CONSTRAINTS:
                constrains_a_body = True

        if isinstance(proof.get("why"), str):
            report.check(proof["why"].strip() != "", f"{where}.why",
                         "says nothing; a proof nobody can justify is one nobody will maintain")

    report.check(
        constrains_a_body,
        "[[proof]]",
        "every proof constrains only a response status. Docker publishes a port by putting a "
        "proxy in front of it, and that proxy accepts before knowing whether anything inside is "
        "listening — so none of these would fail against a container that had been emptied",
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

    validate_config_path(service, report)
    validate_provides(service, report)

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

    for field in sorted(
        set(manifest) - {"schema_version", "plugin", "service", "requires", "wiring", "proof", "secret", "override"}
    ):
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

    wiring = manifest.get("wiring")
    if wiring is not None:
        first = services[0] if isinstance(services, list) and services else {}
        if report.check(isinstance(wiring, dict), "[wiring]", "must be a table"):
            validate_wiring(wiring, first, report)

    proofs = manifest.get("proof")
    if proofs is None:
        report.fail("[[proof]]", "no proof is declared, and a plugin whose proofs do not pass is not installed")
    else:
        validate_proofs(proofs, report)

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
    ("an image named by tag alone", "digest", None, "missing required field 'digest'"),
    ("a digest that is not one", "digest", "sha256:nope", "well-formed sha256 digest"),
    ("a criticality a plugin may not assign itself", "criticality", "critical", "may not declare"),
    ("a kernel capability", "grants", ["NET_ADMIN"], "a plugin may not declare"),
    ("an environment variable", "environment", {"ND_X": "1"}, "a plugin may not declare"),
    ("a port with no tier", "bind", None, "needs the tier"),
    ("a configuration directory inside the library", "config_path", "/data/komga", "inside the data root"),
    ("a configuration directory that is the container root", "config_path", "/", "not a configuration directory"),
    ("a relative configuration directory", "config_path", "config", "not an absolute path"),
    ("a media type outside the vocabulary", "media_types", ["manga"], "is not one of"),
    ("a capability in the core namespace", "provides", ["media.serve"], "neither namespaced"),
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

    # The blocks that are not the service, each broken its own way.
    shaped = (
        ("a hostname for a loopback service", lambda m: (
            m["service"][0].__setitem__("bind", "loopback"),
            m["wiring"].__setitem__("hostname", "admin"),
        ), "only a lan service is proxied"),
        ("a hostname that is not a label", lambda m: m["wiring"].__setitem__("hostname", "http://x:80"),
         "not a single DNS label"),
        ("a dashboard group the dashboard has not got", lambda m: m["wiring"].__setitem__("dashboard_group", "Misc"),
         "is not one of"),
        ("a proxy stanza supplied by the plugin", lambda m: m["wiring"].__setitem__("reverse_proxy", "komga:25600"),
         "outside the permitted set"),
        ("a plugin declaring no proofs at all", lambda m: m.pop("proof"),
         "a plugin whose proofs do not pass is not installed"),
        ("a proof with nothing to decide", lambda m: m["proof"][0].__setitem__("expect", {}),
         "declares nothing"),
        ("a proof asserting something unknown", lambda m: m["proof"][0]["expect"].__setitem__("vibes", "good"),
         "not something this can check"),
        ("two proofs sharing an id", lambda m: m["proof"][1].__setitem__("id", m["proof"][0]["id"]),
         "declared twice"),
        ("proofs that only ever read a status", lambda m: [
            p.__setitem__("expect", {"status": 200}) for p in m["proof"]
        ], "constrains only a response status"),
    )
    for label, break_it, expected in shaped:
        broken = tomllib.loads((ROOT / MANIFEST).read_text(encoding="utf-8"))
        break_it(broken)
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
