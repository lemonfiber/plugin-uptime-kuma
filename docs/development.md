# Working on plugin-uptime-kuma

For the people who maintain this plugin. What the plugin gives the people who
run it is in the [README](../README.md).

## What is here

One manifest and a directory of recordings. Nothing here runs:

| Path | What |
| --- | --- |
| `plugin.toml` | The whole plugin: identity, the service, what it can do, how the stack reaches it, and the proofs that must pass before it installs |
| `fixtures/` | Recorded responses the proofs run against, so nobody needs a live instance |
| `targets.toml` | The lemonfiber release this is validated and proved against |
| `proofs.json` | The proof report CI writes and compares against the committed one |
| `.github/interim/` | The CI harness, copied byte for byte from `plugin-template`. Not part of what an operator installs |

The contract is
[plugin-manifest](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/plugin-manifest.md).
Rules for working in this repository, for people and agents alike, are in
[AGENTS.md](../AGENTS.md).

## What the manifest declares

```toml
[[service]]
image       = "docker.io/louislam/uptime-kuma"
digest      = "sha256:917318f9d7be5257f43ba412c766a473be336eb451d70744f3b482d0c3997c0e"
tag         = "2.5.4"
port        = 3001
bind        = "loopback"
health      = { kind = "http", path = "/api/entry-page", timeout_s = 90 }
criticality = "enhancing"
takes_data  = false
config_path = "/app/data"
provides    = ["uptime-kuma:endpoint-monitor", "uptime-kuma:status-page"]

[[wiring]]
dashboard_group = "Automation"
```

**`loopback`, not `lan`.** It holds the credentials for every channel it notifies
on and can be pointed at any address the machine can reach. That is the tier the
automation apps are in, so it appears on the dashboard and gets **no proxy
hostname**. `[[wiring]]` names none, and there is no field by which it could ask:
the tier decides, not the plugin (`ARCH-R104`).

**`takes_data = false`.** It watches endpoints and reads no library, so the data
root is not mounted and the generated container has no way to ask for it.

## `config_path = "/app/data"`

This image has no `/config`. It keeps its database, its uploads and its
notification configuration at `/app/data`, so that is where the one
configuration directory is mounted (`F3-R32`, `ARCH-R100`,
[spec#386](https://github.com/lemonfiber/spec/pull/386)).

Mounted at `/config` instead, this image is broken two different ways depending
on which user the container runs as. Both were measured.

**As the image's own user**, it installs cleanly, answers its health probe, and
loses everything on recreate:

```
fresh:            {"type":"setup-database"}
after setup:      {"type":"entryPage","entryPage":null}
host /config:     ''                                    ← nothing was written here
in-container:     db-config.json kuma.db kuma.db-shm …  ← state is in the writable layer
after recreate:   {"type":"setup-database"}             ← every monitor gone
```

**As a non-root user**, it does not start at all:

```
state: exited  exit=0
  errno: -13, code: 'EACCES', syscall: 'mkdir', path: 'data/upload/'
```

With the target declared, the same sequence keeps its state:

```
after setup:      {"type":"entryPage","entryPage":null}
container removed entirely with `docker rm -f`
host mount keeps: db-config.json docker-tls kuma.db kuma.db-shm kuma.db-wal screenshots upload
after recreate:   {"type":"entryPage","entryPage":null}   ← still set up
```

One directory, one mount, the source still lemonfiber's. Only the target is named
here, and it is checked: one absolute path, not the root, not inside the data
root. `/config` is a LinuxServer.io convention, not a standard.

## What it can do

`provides` is the capability model's plugin side (`F4-R1`). Both claims are
namespaced with the plugin's id (`F4-R4`), and a namespaced capability is inert
until something asks for it.

Neither name is in lemonfiber's published core vocabulary, and that is the right
answer rather than a gap: `F9-R3` keeps a capability nothing bundled implements
out of the core set, and nothing bundled watches endpoints.
`.github/interim/published_gate.py` holds this manifest to the published set on
every run, and goes red the day either name becomes a core one.

## What it adds to the doctor

Both capabilities being inert is why this section matters. A plugin may put a
row in a register lemonfiber already runs (`F3-R26`), and this plugin adds two
things about the stack that lemonfiber could not have known to say.

| Check | What it notices | Remedy |
| --- | --- | --- |
| `uptime-kuma:set-up` | It is running, healthy, green, and nobody ever opened it, so it is watching nothing | Open it and add a monitor for each thing you would want to know had gone |
| `uptime-kuma:api-still-answers-json` | The path the health probe asks for has stopped being an API path, so the probe has gone blind without going red | Treat the probe as unproven and pin the image back |

The first is the failure this service actually has. It does not go down; it
never starts doing the one thing it was installed for, and every surface reports
it as fine. `/api/entry-page` is the one anonymous answer that tells the two
apart.

Both ask something no credential is needed for, and that is forced rather than
chosen: everything else here is socket.io, so a check needing a credential would
report `unrun` on every doctor run, and a check that quietly never runs is worse
than one that fails.

No code is contributed and none can be (`F3-R6`): the row is data, and the engine
that reads it is lemonfiber's.

## The proofs, and why all three are about the body

This service answers **HTTP 200 to every path it does not implement**, serving
its single-page app as the fallback. Measured:

```
                                 before its database exists   afterwards
/api/entry-page              ->  200 {"type":"setup-database"}  200 {"type":"entryPage"}
/api/not-a-real-path         ->  200 <!DOCTYPE html>…           200 <!DOCTYPE html>…
/api/push/abc123             ->  200 <!DOCTYPE html>…           404 {"ok":false,…}
/api/status-page/nonexistent ->  200 <!DOCTYPE html>…           404 {"status":"fail",…}
/api/badge/1/status          ->  200 <!DOCTYPE html>…           200 image/svg+xml
```

A proof that read a status here would pass against a build with no API at all,
and against a container that had been emptied, once Docker's port proxy is in
front of it.

The unimplemented-path proof asks `/api/not-a-real-path` because it answers
identically in both states. `/api/push/<token>` answers the app shell only while
the service has no database; once it has one, that path is a real endpoint
answering a real 404, so a proof over it describes a fresh container rather than
this image.

| Proof | What it establishes |
| --- | --- |
| `uptime-kuma.serves` | `/api/entry-page` answers with JSON about *this* instance, which is the only path that does |
| `uptime-kuma.status-is-not-an-answer` | A path this service implements in **neither** state answers 200 with HTML, the property that makes the first proof's shape necessary |
| `uptime-kuma.state-outlives-its-container` | Set up, destroyed and recreated, it is still set up. The proof that `config_path` is doing its job |

The third is recorded from a container that was genuinely removed and recreated
rather than restarted.

## The evidence

Run against the live image, `docker.io/louislam/uptime-kuma@sha256:917318f9…`:

- the image starts as a non-root user against the declared `config_path` and
  answers;
- all three proofs and both contributed checks pass against the live container;
- the first two report **unproven**, neither failed nor passed, against the same
  published port with the process replaced by `sleep infinity`;
- the state survives `docker rm -f` and a new container on the same mount, and
  does not survive it with the configuration directory at `/config`;
- the digest resolves, `2.5.4` names it, and **no signature is offered**,
  recorded as unproven rather than verified.

Checked by validating only: that the manifest conforms. CI holds it to
lemonfiber's own generated schema, fetched off its default branch on every run,
and then to the capability vocabulary and the extension points published beside
it.

Not claimed here: a run of `lemonfiber plugin install` over this manifest. The
persistence evidence above is `docker run` doing by hand what the generated
container does. The `lemonfiber plugin` commands are on lemonfiber's `main`
branch and in no release, and `targets.toml` names `0.16.0`, which is not
released.

## What is deliberately not here

No `[[secret]]` and no `[[override]]`. Uptime Kuma's notification credentials are
entered in its own UI and lemonfiber captures none of them; this plugin changes
no bundled setting. Both blocks exist in the format (`F3-R17`, `F3-R18`); a
plugin that holds nothing declares nothing.

No `[[recipe]]`. Seeding this with a monitor for every service in the stack would
be a recipe: ordered calls, a captured token, a declared pair for each. A
manifest declaring one asks for `recipe.run` by name, so a lemonfiber that cannot
run one refuses the manifest rather than parsing the block and skipping it.
Declaring one here would make this plugin uninstallable wherever `recipe.run` is
not offered.

No proxy hostname, because the tier refuses it. No dashboard widget, because a
widget needs a credential.

## This repository is not the catalogue

[`lemonfiber-plugins`](https://github.com/lemonfiber/lemonfiber-plugins) is the
reviewed catalogue. This is a plugin's **source**, which is all `F10-R9` says
publishing requires: a git repository. `F10-R7` is why its CI runs the same
commands the catalogue's CI runs.

Being official buys this plugin nothing: the same schema validation, digest
pinning, signature check and proof runs apply to any plugin written by anybody.

## Running the checks

```sh
just ci        # every gate CI runs over this repository, in CI's order
just live      # the same proofs against a running instance on 127.0.0.1:3001
```

`just` lists the recipes `ci` is made of. The CI jobs it does not run are named
in the `justfile` beside the recipe, with what covers each.

`prove.py` is given `--against fixtures --report proofs.json`, which is what CI
runs it with. Without `--report` the assertions are proved and `proofs.json` is
left untouched, so a run that reports everything passing is still refused by
`git diff --exit-code proofs.json`.

Moving the image pin means re-recording every fixture against the new image in
the same change.

## Before you open a pull request

`just ci` turns this clone's git hooks on as its first step, and
`.githooks/commit-msg` then refuses a commit CI would refuse: a non-conventional
subject, a missing sign-off, a missing `Spec:` citation, or a trailer crediting
an assistant. The rules are in
[50-governance/contributing.md](https://github.com/lemonfiber/spec/blob/main/50-governance/contributing.md).
