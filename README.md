# plugin-uptime-kuma

**Uptime Kuma as a lemonfiber plugin.** Watches the things the stack depends on
and tells you which one went, rather than that something did.

A media stack fails in a way that hides its own cause. An indexer stops answering
and what you see is a search returning nothing; a tracker goes down and what you
see is a download that never starts. This watches the endpoints directly, so the
first thing you read is the one that broke.

## What it is

One manifest and a directory of recordings. Nothing here runs:

| Path | What |
| --- | --- |
| `plugin.toml` | The whole plugin: identity, the service, what it can do, how the stack reaches it, and the proofs that must pass before it installs |
| `fixtures/` | Recorded responses the proofs run against, so nobody needs a live instance |
| `targets.toml` | The lemonfiber release this is validated and proved against |

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
automation apps are in — so it appears on the dashboard, and it gets **no proxy
hostname**. `[[wiring]]` names none, and there is no field by which it could ask:
the tier decides, not the plugin (`ARCH-R104`).

**`takes_data = false`.** It watches endpoints and reads no library, so the data
root is not mounted and the generated container has no way to ask for it.

## `config_path = "/app/data"` — the line this plugin exists to justify

This image has no `/config`. It keeps its database, its uploads and its
notification configuration at `/app/data`.

Before `config_path` existed, lemonfiber generated every plugin's container with
its configuration directory mounted at `/config`, and this plugin was **broken
two different ways depending on which user the container ran as**. Both measured,
not reasoned about:

**As the image's own user** — installs cleanly, answers its health probe, and
loses everything on recreate:

```
fresh:            {"type":"setup-database"}
after setup:      {"type":"entryPage","entryPage":null}
host /config:     ''                                    ← nothing was written here
in-container:     db-config.json kuma.db kuma.db-shm …  ← state is in the writable layer
after recreate:   {"type":"setup-database"}             ← every monitor gone
```

**As a non-root user** — does not start at all:

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
root.

This was reported as a defect in the plugin model rather than worked around, and
the model was changed — [spec#386](https://github.com/lemonfiber/spec/pull/386),
`F3-R32` and `ARCH-R100`. `/config` is a LinuxServer.io convention, not a
standard: five of the twenty bundled services keep their configuration elsewhere
and the stack's `compose/` says so for each.

## What it can do, and why it stays namespaced

`provides` is the capability model's plugin side (`F4-R1`). Both claims are
namespaced with the plugin's id (`F4-R4`), and a namespaced capability is inert
until something asks for it.

The core vocabulary is published now, and **neither of these is in it**. This
README predicted that before it existed, and the prediction was right for the
reason it gave: `F9-R3` keeps a capability nothing bundled implements out of the
core set, and nothing bundled watches endpoints. So this is the right answer
rather than a gap — an ecosystem vocabulary is exactly what namespaces are for,
and Komga's `media.serve` and these two are the two halves of the same design
working.

What changed is that it is a fact rather than a guess.
`.github/interim/published_gate.py` holds this manifest to the published set on
every run, and goes red the day either name becomes a core one.

## What it adds to what lemonfiber says

Both capabilities being inert is precisely why this section exists. A plugin may
put a row in a register lemonfiber already runs (`F3-R26`), and that is where
this plugin is useful rather than merely readable: it tells lemonfiber two things
about the stack that lemonfiber could not have known to say.

| Check | What it notices | Remedy |
| --- | --- | --- |
| `uptime-kuma:set-up` | It is running, healthy, green — and nobody ever opened it, so it is watching nothing | Open it and add a monitor for each thing you would want to know had gone |
| `uptime-kuma:api-still-answers-json` | The path the health probe asks for has stopped being an API path, so the probe has gone blind without going red | Treat the probe as unproven and pin the image back |

The first is the failure this service actually has. It does not go down; it never
starts doing the one thing it was installed for, and every surface reports it as
fine. `/api/entry-page` is the one anonymous answer that tells the two apart.

Both ask something no credential is needed for, and that is forced rather than
chosen: everything else here is socket.io, so a check needing a credential would
report `unrun` on every doctor run until recipes arrive — and a check that
quietly never runs is worse than one that fails.

No code is contributed and none can be (`F3-R6`). There is nothing for
contributed code to *be*: the row is data and the engine that reads it is
lemonfiber's, unchanged.

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

A proof that read a status here would pass against a build with no API at all —
and against a container that had been emptied, once Docker's port proxy is in
front of it.

**The right column is new, and it corrected a proof.** The unimplemented-path
proof used to ask `/api/push/<token>`, which answers the app shell only while
this service has no database; once it has one, that path is a real endpoint
answering a real 404. The proof passed against its recording and failed against a
running instance — a fixture is evidence about a moment, and that moment was a
fresh container. `/api/not-a-real-path` answers identically in both states, which
is what makes it the path worth recording.

| Proof | What it establishes |
| --- | --- |
| `uptime-kuma.serves` | `/api/entry-page` answers with JSON about *this* instance, which is the only path that does |
| `uptime-kuma.status-is-not-an-answer` | A path this service implements in **neither** state answers 200 with HTML — the property that makes the first proof's shape necessary |
| `uptime-kuma.state-outlives-its-container` | Set up, destroyed and recreated, it is still set up. The proof that `config_path` is doing its job |

The third is the one that would have caught this plugin being broken, and it is
recorded from a container that was genuinely removed and recreated rather than
restarted.

## What was proved by running, and what only by validating

Proved by running, on `docker.io/louislam/uptime-kuma@sha256:917318f9…`:

- the image starts as a non-root user against the declared `config_path` and
  answers;
- all three proofs and both contributed checks pass against the live container;
- the first two report **unproven** — not failed, not passed — against the same
  published port with the process replaced by `sleep infinity`;
- the state genuinely survives `docker rm -f` and a new container on the same
  mount, and genuinely does not survive it under the old `/config` shape;
- the unimplemented-path proof, re-run against an instance whose database had
  been created, **failed** — and that is how the path it asks about came to be
  changed to one this service implements in neither state;
- the digest resolves, `2.5.4` still names it, and **no signature is offered**,
  recorded as unproven rather than verified.

Proved only by validating:

- that the manifest conforms. It is held to lemonfiber's own generated schema,
  fetched off its default branch on every run, and then to the capability
  vocabulary and the extension points it publishes beside it.

**Not proved at all, and not claimed:** that lemonfiber installs this, generates
the mount at the declared path, or puts the dashboard entry in the Automation
group. `0.16.0` is planned. The persistence evidence above is `docker run` doing
by hand what the generated container would do — which is why the manifest now has
a field to say it, but is not a claim that an installer has done it.

## What is deliberately not here

No `[[secret]]` and no `[[override]]`. Uptime Kuma's notification credentials are
entered in its own UI and lemonfiber captures none of them; this plugin changes no
bundled setting. Both blocks exist in the format (`F3-R17`, `F3-R18`); a plugin
that holds nothing declares nothing — and in this version nothing *could* capture
one, because capture is a recipe and recipes arrive with `F8`.

No `[[recipe]]`. Seeding this with a monitor for every service in the stack is
the obvious next thing to want and it is exactly a recipe: ordered calls, a
captured token, a declared pair for each. The block exists in the format and is
checked, and a manifest declaring one asks for `recipe.run` by name — so a
lemonfiber that cannot run one refuses the manifest rather than parsing the block
and skipping it. Until that capability is offered, declaring one here would make
this plugin uninstallable in exchange for nothing.

No proxy hostname, because the tier refuses it. No dashboard widget, because a
widget needs a credential.

## Where this repository is not the catalogue

`lemonfiber-plugins` is the reviewed catalogue, and this is not it. This is a
plugin's **source** — what `F10-R9` says publishing requires and no more than: a
git repository. `F10-R7` is why its CI runs the same commands the catalogue's CI
runs.

## Being official buys this nothing

Same schema validation, same digest pinning, same signature verification, same
proof runs as any plugin written by anybody. When this plugin turned out to be
unshippable under the model as it stood, the model was fixed for everybody — not
waived for this one.

## Licence

The plugin data in this repository is under the Hippocratic License 3.0
(HL3-CORE) — see [LICENSE](LICENSE). Uptime Kuma itself is MIT and is not
distributed here; this repository names an image, it does not contain one.
