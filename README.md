# plugin-uptime-kuma

**Uptime Kuma as a lemonfiber plugin.** Watches the things the stack depends on
and tells you which one went, rather than that something did.

A media stack fails in a way that hides its own cause. An indexer stops
answering and what you see is a search returning nothing; a tracker goes down
and what you see is a download that never starts. This watches the endpoints
directly, so the first thing you read is the one that broke.

## What it is

Three files and a directory, and none of it runs:

| Path | What |
| --- | --- |
| `plugin.toml` | The manifest. Identity, the one service, and what it needs of lemonfiber |
| `proofs.toml` | The proofs it declares, and why each one is worth asserting |
| `fixtures/` | Recorded responses the proofs run against, so nobody needs a live instance |
| `targets.toml` | The lemonfiber release this is validated and proved against |

## What the manifest declares

```toml
image  = "docker.io/louislam/uptime-kuma"
digest = "sha256:917318f9d7be5257f43ba412c766a473be336eb451d70744f3b482d0c3997c0e"
tag    = "2.5.4"
port   = 3001
bind   = "loopback"
health = { kind = "http", path = "/api/entry-page", timeout_s = 90 }
criticality = "enhancing"
takes_data  = false
forms  = ["full"]
```

**`bind = "loopback"`, not `lan`.** It holds the credentials for every
notification channel it sends on and can be pointed at any address the machine
can reach. That puts it in the tier the automation apps are in rather than the
one the library server is in.

**`takes_data = false`.** It watches endpoints. It reads no library, so the data
root is not mounted, and the generated container has no way to ask for it.

## The proofs, and why both are about the body

This service answers **HTTP 200 to every path it does not implement**, serving
its single-page app as the fallback. `/`, `/metrics`, `/api/push/<anything>` and
a path invented on the spot all return 200 with the same HTML. Verified, not
assumed:

```
kuma /api/entry-page              -> 200  {"type":"setup-database"}
kuma /api/push/abc123             -> 200  <!DOCTYPE html><html lang="en">…
kuma /api/status-page/nonexistent -> 200  <!DOCTYPE html><html lang="en">…
kuma /api/badge/1/status          -> 200  <!DOCTYPE html><html lang="en">…
```

So a probe that read a status here would pass against a build with no API at
all — and against a container that had been replaced, once Docker's own port
proxy is in front of it.

| Proof | What it establishes |
| --- | --- |
| `uptime-kuma.serves` | `/api/entry-page` answers with JSON about *this* instance, which is the only path that does |
| `uptime-kuma.status-is-not-an-answer` | An unimplemented path answers 200 with HTML — the property that makes the first proof's shape necessary |

The second is not a proof that the service works. It pins the reason the health
path is what it is, so that a future change in this behaviour goes red rather
than leaving a probe that has quietly gone blind.

## What was proved by running, and what only by validating

Proved by running, on `docker.io/louislam/uptime-kuma@sha256:917318f9…`:

- the image starts as a non-root user and answers;
- both proofs pass against the live container;
- both report **unproven** — not failed, and not passed — against the same
  published port with the process replaced by `sleep infinity`;
- the digest resolves in the registry, `2.5.4` still names it, and **no
  signature is offered for it**, which is recorded as unproven rather than
  verified.

Proved only by validating:

- that the manifest conforms. There is no published schema to conform *to*, so
  it is checked against the contract document by hand.

**Not proved at all, and not claimed:** that lemonfiber installs this. The
release that implements plugins is `0.16.0` and it is planned.

## The defect this plugin found, which it cannot work around

**Under the plugin format as it stands, this service loses its state on every
container recreate.**

lemonfiber writes a plugin's container itself, and the mount set is fixed: the
data root where `takes_data` asks for it, and the plugin's own configuration
directory at `/config`. That is deliberate and right — it is what makes "what
can this plugin reach" answerable from the format rather than from the instance.

This image keeps its database at `/app/data`. Verified:

```
$ docker exec … ls /config
no /config in image
$ docker exec … ls /app/data
docker-tls  screenshots  upload
```

There is no field by which the manifest can say so. `environment` is not in the
permitted set, and neither is a mount target — so `UPTIME_KUMA_DATA_DIR` cannot
be set and `/app/data` cannot be bound. The generated container mounts a
directory the application never reads, and every monitor, notification channel
and history entry lives in the container's writable layer until it is replaced.

The bundled stack has this exact problem and solves it in `compose/`, two
different ways. Three services mount their configuration somewhere other than
`/config` — Homepage and Seerr at `/app/config`, Caddy at a single file under
`/etc/caddy` — and two more need a mount beside it, Jellyfin for `/cache` and
Audiobookshelf for `/metadata`. Five of the twenty, and a plugin can do
neither thing.

**This is reported rather than worked around.** No privilege is asked for here,
no field is invented, and the manifest claims nothing that is not true. What it
needs is a way for a manifest to name where in its container the configuration
directory is mounted — one string, no new reach, since lemonfiber still chooses
the source and there is still exactly one of them.

Until that exists, this plugin is correct, validates, proves itself, and installs
something an operator would have to re-create after every image bump. Which is
worth knowing before installing it, and is why it is at the top of this section
rather than the bottom.

## Where this repository is not the catalogue

`lemonfiber-plugins` is the reviewed catalogue, and this is not it. This is a
plugin's **source** — the thing `F10-R9` says publishing requires and no more
than: a git repository. `F10-R7` is why its CI runs what it runs: the same
commands the catalogue's CI runs, so the first time a plugin meets them is not
in somebody else's pull request.

## Being official buys this nothing

Same schema validation, same digest pinning, same signature verification, same
proof runs as any plugin written by anybody. There is no trust bit and no
shortcut — and the defect above was not waived for one.

## Licence

The plugin data in this repository is under the Hippocratic License 3.0
(HL3-CORE) — see [LICENSE](LICENSE). Uptime Kuma itself is MIT and is not
distributed here; this repository names an image, it does not contain one.
