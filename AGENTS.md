# AGENTS.md — plugin-uptime-kuma

Guidance for any AI agent working in this repo.

> **Common rules for every lemonfiber repo are canonical in the spec:**
> [50-governance/ai-contributors.md](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> Read them. This file is the `plugin-uptime-kuma`-specific header only.

## What this repo is

A plugin's source: the manifest lemonfiber installs Uptime Kuma from, the proofs
it declares, and the recorded responses those proofs run against. It is not the
reviewed catalogue — that is `lemonfiber-plugins` — and it is not a fork of the
stack. Contract:
[plugin-manifest](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/plugin-manifest.md).

## Read the defect before changing anything

This image keeps its state at `/app/data`, and the container lemonfiber
generates for a plugin mounts its configuration directory at `/config` with no
field by which a manifest can say otherwise. The service therefore installs and
works and forgets everything when its container is replaced. `README.md` records
it in full.

**Do not work around it.** There is no field to set an environment variable and
no field to name a mount target, and inventing one would make this manifest
refused rather than better. The fix belongs in the contract, and until it lands
this repository's job is to state the problem accurately.

## The rules you cannot break

- **Nothing here executes.** A plugin is declarative data (`F3-R1`), and
  contributed code is never run, under any opt-in (`F3-R6`). The Python under
  `.github/interim/` is CI harness, is not part of what an operator installs,
  and is deleted when lemonfiber's own verbs replace it.
- **The image is named by digest** (`F3-R8`). Moving the pin means re-recording
  every fixture against the new image in the same change.
- **A proof asserts a body, never only a status.** It matters more here than
  almost anywhere: this service answers 200 with its single-page app for every
  path it does not implement, so a status-only proof passes against a build with
  no API at all.
- **A proof that could not be run is unproven** (`F3-R5`), never a pass.
- **No field beyond the contract's set.** A manifest carrying one is refused by
  name rather than ignored (`ARCH-R84`).
- **Nothing official about this plugin is a privilege.** If it ever needs one to
  work, that is a defect in the plugin model and belongs in a spec issue, not in
  a field here.

## Checks

```
python3 .github/interim/validate.py --self-test   # the gate refuses what it should
python3 .github/interim/validate.py              # the manifest against the contract
python3 .github/interim/prove.py                  # proofs against the recordings
python3 .github/interim/prove.py --against http://127.0.0.1:3001   # against a live one
python3 .github/interim/image_gate.py             # the digest, its tag, its signature state
python3 .github/interim/schema_gate.py            # fails the day the real schema lands
```

## Before you open a PR

- Cite a spec identifier in a commit `Spec:` trailer and the PR body.
- No AI attribution in commits.
