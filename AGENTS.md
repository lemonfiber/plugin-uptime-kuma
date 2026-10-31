# AGENTS.md — plugin-uptime-kuma

> **Start at the roadmap and board on [lemonfiber.app](https://lemonfiber.app),
> rendered from the report of where every unreleased version stands. Then the
> rules** every repository shares:
> [working in the repositories](https://github.com/lemonfiber/spec/blob/main/50-governance/working-in-the-repositories.md)
> and [the rules for agents](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> This file holds only what is true of this repository.

## What this repo is

Uptime Kuma as a first-party lemonfiber plugin: `plugin.toml` and the adapter
that speaks the `owner.claim@1` contract in front of Uptime Kuma. The adapter is
the Rust crate at the root, built on `lemonfiber-adapter-kit` from the core at the
commit `Cargo.toml` pins, into the image the `Dockerfile` builds. `release.yml`
publishes that image from every commit on `main` and from the train's tag,
through spec's reusable `adapter-release`. `plugin.toml` runs Uptime Kuma alone,
with its SQLite database named in the environment, and does not run the adapter
yet; its proofs' recordings are under `fixtures/`.

## The rules you cannot break

- **The plugin id is the service's id** (`ARCH-R266`).
- **No Python here.** The reader is spec's `first-party-plugin` workflow's.
- **No `unwrap`, `expect`, `panic` or indexing** outside tests. The lints in
  `Cargo.toml` are the core's.
- **The adapter never writes its credentials.** lemonfiber mints the
  administrator's name and password and writes them into the adapter's
  `upstream.json` as `username` and `password` (`ARCH-R315`, `ARCH-R317`). The
  password is minted as `"password"`, which Uptime Kuma's strength rule takes.
  The adapter reads them for every claim and never logs them.
- **Uptime Kuma is asked only over its Socket.IO connection**, Engine.IO 4 over a
  WebSocket, because it makes and signs in its administrator nowhere else. A
  connection is asked nothing until Uptime Kuma emits `loginRequired` or
  `autoLogin`: it adds its handlers after it accepts the connection, and an event
  emitted before then is lost.
- **A claim changes nothing Uptime Kuma already holds** (`ARCH-R328`): only an
  Uptime Kuma that says it waits to be set up is set up, and one set up by anybody
  else is answered as theirs. An account that asks the minted administrator for a
  second factor is ours, since only its holder could have turned that on.
- **The core's crates move with the train.** The `rev` in `Cargo.toml` is the
  core commit `targets.toml` names, the same for every `lemonfiber-*` crate.
- **Moving Uptime Kuma's digest** means recording every response again against
  the new image in the same change, moving `RELEASE` and `DIGEST` in
  `src/adapter.rs` with it, and committing the `proofs.json` the checks write.

## Checks

```
just ci
```

The gates that need nothing but this tree. The plugin verdict and the
recordings run in CI; `just recordings` takes the recordings again locally, with
Docker.
