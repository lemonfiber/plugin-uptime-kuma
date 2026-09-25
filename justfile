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
#   harness                                    `.github/reader/` here is compared
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
ci: hooks reader manifest proofs image typos links

# The release `targets.toml` names, fetched once into `.lemonfiber/` and checked
# against its published digest, and what it says of this plugin. A capability it
# does not offer a plugin is reported rather than failed on.
#
# The release this plugin names, and what it says of it.
reader:
    python3 .github/reader/reader.py reader

# Every refusal that release makes of the manifest, each with its location.
#
# The manifest, as the release reads it.
manifest:
    python3 .github/reader/reader.py manifest

# Everything the manifest declares, against the recorded responses — and the
# committed report is the one this run writes. `reader.py proofs` writes
# `proofs.json` every time, and CI then runs `git diff --exit-code proofs.json`.
#
# Everything the manifest declares, and the report CI diffs.
proofs:
    python3 .github/reader/reader.py proofs
    git diff --exit-code -- proofs.json

# Every pinned digest is in its registry and its tag still names it, then what
# vouches for each image. Needs `docker`.
#
# The pinned digests, their tags, and what vouches for them.
image:
    python3 .github/reader/reader.py image

# Spell check.
typos:
    typos

# Link check.
links:
    lychee --no-progress .
