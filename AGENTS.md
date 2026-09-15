# AGENTS.md — plugin-uptime-kuma

Guidance for any AI agent working in this repo.

> **Common rules for every lemonfiber repo are canonical in the spec:**
> [50-governance/ai-contributors.md](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> Read them. This file is the `plugin-uptime-kuma`-specific header only.

## What this repo is

A plugin's source: the manifest lemonfiber installs Uptime Kuma from — the
service, what it can do, the proofs it declares and the checks it contributes to
the doctor — and the recorded responses those run against. It is not the
reviewed catalogue — that is `lemonfiber-plugins` — and it is not a fork of the
stack. Contract:
[plugin-manifest](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/plugin-manifest.md).

## `config_path` is load-bearing here

This image has no `/config`; it keeps its database at `/app/data`. Under the
plugin format as it originally stood there was no field to say so, and this
plugin was broken two ways — losing every monitor on recreate when run as the
image's own user, and failing to start at all when run as a non-root one.

That was reported as a defect in the model and the model was changed
(`F3-R32`, `ARCH-R100`). `config_path = "/app/data"` is the fix, and
`uptime-kuma.state-outlives-its-container` is the proof that keeps it honest.

**Do not remove or "simplify" that line**, and do not reach for an environment
variable instead — there is no field for one, deliberately (`ARCH-R101`), and a
manifest carrying one is refused.

## The rules you cannot break

- **Nothing here executes.** A plugin is declarative data (`F3-R1`), and
  contributed code is never run, under any opt-in (`F3-R6`). The Python under
  `.github/interim/` is CI harness, is not part of what an operator installs,
  and is deleted when lemonfiber's own verbs replace it.
- **The plugin is `plugin.toml` and `fixtures/`.** Proofs live in the manifest,
  not beside it: an installer reads one file, and a proof the installer never
  reads cannot be what `F3-R4` refuses an install over.
- **This service is `loopback`, so it gets no proxy hostname.** `[wiring]` names
  none and there is no field by which it could ask. The tier decides.
- **The image is named by digest** (`F3-R8`). Moving the pin means re-recording
  every fixture against the new image in the same change.
- **A proof asserts a body, never only a status.** It matters more here than
  almost anywhere: this service answers 200 with its single-page app for every
  path it does not implement, so a status-only proof passes against a build with
  no API at all.
- **A proof that could not be run is unproven** (`F3-R5`), never a pass.
- **A contributed check asks something no credential is needed for.** Everything
  else this service offers is socket.io, so a check that needed one would report
  `unrun` on every doctor run until recipes arrive — and a check that quietly
  never runs is worse than one that fails.
- **Both capabilities stay namespaced, and that is the answer.** The core
  vocabulary exists now and neither is in it, because `F9-R3` keeps a capability
  nothing bundled implements out of the core set and nothing bundled watches
  endpoints. Do not "fix" that by reaching for a core-looking name;
  `vocabulary_gate.py` holds this manifest to the published set on every run.
- **No field beyond the contract's set.** A manifest carrying one is refused by
  name rather than ignored (`ARCH-R84`).
- **Nothing official about this plugin is a privilege.** If it ever needs one to
  work, that is a defect in the plugin model and belongs in a spec issue, not in
  a field here.

## Checks

```
python3 .github/interim/validate.py --self-test   # the gate refuses what it should
python3 .github/interim/validate.py              # the manifest against the contract
python3 .github/interim/prove.py                  # everything declared, against the recordings
python3 .github/interim/prove.py --against http://127.0.0.1:3001   # against a live one
python3 .github/interim/image_gate.py             # the digest, its tag, its signature state
python3 .github/interim/schema_gate.py            # fails the day the real schema lands
python3 .github/interim/vocabulary_gate.py        # every claim, against what lemonfiber publishes now
```

## The harness is not this repository's

`.github/interim/` is written in
[`plugin-template`](https://github.com/lemonfiber/plugin-template) and copied
here byte for byte. The `harness` job fails when the two differ. Change it there
and copy it here; changing it here fails.

## Before you open a PR

- Cite a spec identifier in a commit `Spec:` trailer and the PR body.
- No AI attribution in commits.
