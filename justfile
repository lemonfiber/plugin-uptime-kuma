# Task runner for lemonfiber/plugin-uptime-kuma. `just` with no argument lists these.
default:
    @just --list

# Turn on the repository's own git hooks. Once per clone.
#
# There is no package manager here, so there is no `npm ci` or `composer install`
# to hang `core.hooksPath` on the way the other repos do — it is this recipe or
# nothing, and `ci` depends on it so that running the checks once turns the hooks
# on for good. The setting is per-clone local config and no commit can carry it.
#
# Turn on this clone's git hooks. Once per clone.
hooks:
    git config core.hooksPath .githooks
    @echo "hooks on: .githooks/commit-msg, .githooks/pre-push"

# Every gate CI runs over the contents of this repository, in the order CI runs
# them, plus the hooks that answer for the commit message.
#
# Six jobs are not here, and naming them is the point — a recipe that claims to
# be CI and is a subset of it teaches people to skip it and read the run instead.
#
#   commitlint, dco, attribution, spec-check   `.githooks/commit-msg` refuses all
#                                              four of these before the push, and
#                                              `hooks` above is what turns it on
#   harness                                    `.github/interim/` here is compared
#                                              byte for byte against
#                                              plugin-template's, which needs
#                                              both trees
#   shared-files, pins, workflow-pins          need a lemonfiber/spec checkout and
#                                              the forge to compare against
#   actionlint, markdown, invite               need tools this repository does not
#                                              otherwise ask for; `npx
#                                              markdownlint-cli2 "**/*.md"` and
#                                              `actionlint` are the two commands
#   osv-scanner, gitleaks, label               forge-side, and decide nothing about
#                                              a manifest
#
# Every gate CI runs over this repository's contents — not the whole of CI.
ci: hooks manifest vocabulary proofs image schema typos links

# The stand-in refuses what it exists to refuse, then the manifest against the
# contract. The self-test runs first: a gate nobody has seen fail is a gate
# nobody knows the shape of.
#
# The self-test, then the manifest against the contract.
manifest:
    python3 .github/interim/validate.py --self-test
    python3 .github/interim/validate.py

# Every claim and contribution, against the vocabulary lemonfiber publishes now
# rather than a copy taken once. Reads the forge; no token needed.
#
# Every claim and contribution, against what lemonfiber publishes now.
vocabulary:
    python3 .github/interim/vocabulary_gate.py

# Everything the manifest declares, against the recorded responses — and the
# committed report is the one this run writes.
#
# `--report proofs.json` is not optional. CI runs `prove.py` with it and then
# `git diff --exit-code proofs.json`, so a run without the flag proves the
# assertions and leaves the report it is about to be judged on untouched.
#
# Everything the manifest declares, and the report CI diffs.
proofs:
    python3 .github/interim/prove.py --against fixtures --report proofs.json
    git diff --exit-code -- proofs.json

# The same proofs against a service that is actually running, which is the
# stronger claim and the one the recordings stand in for. No report: what CI
# compares is the run against `fixtures`.
#
# The same proofs against a service that is actually running.
live against="http://127.0.0.1:3001":
    python3 .github/interim/prove.py --against {{against}}

# The declared digest is in the registry, the tag beside it still names it, and
# whether anything signed it.
#
# The declared digest, its tag, and its signature state.
image:
    python3 .github/interim/image_gate.py

# The register pointing the other way: it fails the day lemonfiber publishes the
# schema this harness stands in for. Without a token in the environment it
# reports unproven rather than clear — `GH_TOKEN=$(gh auth token) just schema`
# is how to get an answer out of it.
#
# Refuses the day lemonfiber publishes the schema this stands in for.
schema:
    python3 .github/interim/schema_gate.py

# Spell check.
typos:
    typos

# Link check.
links:
    lychee --no-progress .
