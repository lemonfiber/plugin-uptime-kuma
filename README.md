# Uptime Kuma for lemonfiber

**Know which service went down, rather than only that something stopped
working.** This plugin adds [Uptime Kuma](https://github.com/louislam/uptime-kuma)
to your lemonfiber stack: it watches the services, sites and connections your
stack depends on, and shows you which one stopped answering.

Without it, you find out an indexer, a tracker or the connection itself is down
from whatever stopped working because of it.

## What you get

- **Uptime Kuma 2.5.4**, running as a service in your stack.
- **A link on the stack's dashboard**, in the Automation group. Uptime Kuma is an
  administration tool, so like the stack's other automation services it is
  reachable from this machine only, on port 3001, and gets no household address.
- **A check in `lemonfiber doctor`**, with a fix when it fails:

| Check | What it notices |
| --- | --- |
| The path the health probe asks for still answers with this service's own state | The address lemonfiber uses to tell whether Uptime Kuma is up has stopped answering with Uptime Kuma's own state, so a passing health check no longer means it is working |

## What it needs

- **A lemonfiber with the `lemonfiber plugin` commands.** lemonfiber 0.15.0 and
  earlier do not have them. `lemonfiber version` says which version you have.
- **A stack lemonfiber has already set up on this machine.** Run
  `lemonfiber setup` first if you have not. Installing a plugin is refused on a
  machine with no stack.
- **Port 3001 free** on this machine.
- **Disk space** for the Uptime Kuma image, and for its own data in
  `config/uptime-kuma` in your stack directory.
- **Access to `docker.io`** the first time Uptime Kuma starts, to download the
  image.
- **No accounts anywhere else.** lemonfiber makes Uptime Kuma's administrator
  account for you when it installs the plugin. Any notification channel you want
  it to use, you set up in Uptime Kuma itself.

## Install

Get a copy of this repository, then ask lemonfiber what installing it would do.
`--dry-run` settles everything a real install settles and writes nothing:

```sh
git clone https://github.com/lemonfiber/plugin-uptime-kuma.git
lemonfiber plugin install ./plugin-uptime-kuma --dry-run
lemonfiber plugin install ./plugin-uptime-kuma
```

Keep the `./`: it marks a directory on this machine rather than the name of a
plugin in the catalogue.

The [lemonfiber plugin catalogue](https://github.com/lemonfiber/lemonfiber-plugins/blob/main/plugins/uptime-kuma.toml)
records the revision of this repository that a person reviewed, as `revision`.
To install exactly that, run `git -C plugin-uptime-kuma checkout <revision>`
before you install.

When you install, lemonfiber:

1. Reads `plugin.toml` and checks all of it. If anything in it does not
   conform, it names every problem at once and changes nothing.
2. Writes Uptime Kuma's container, its configuration directory and a dashboard
   entry, and records each change as it makes it.
3. Starts Uptime Kuma and asks it the questions this plugin declares, to show it
   is Uptime Kuma, that it answers, and that what it has set up survives its
   container being replaced.
4. Runs the stack's own checks before and after, and compares the two.
5. Records Uptime Kuma as installed only when all of that holds. If anything
   fails, it puts back everything it wrote, and your machine is as it was.

[Installing a plugin](https://docs.lemonfiber.app/plugins/installing-a-plugin/)
explains each step in more detail. `lemonfiber plugin installed` lists what is
installed, where each plugin came from, and how each of its services is reached.

## Set it up

1. **Open Uptime Kuma** from the dashboard, or at `http://127.0.0.1:3001` on
   this machine.
2. **Sign in as the administrator lemonfiber made.** `lemonfiber credentials`
   lists the account. `lemonfiber credentials --reveal uptime-kuma:admin-username`
   and `--reveal uptime-kuma:admin-password` print its name and password, asking
   first. Uptime Kuma's first-run screen never shows: its SQLite
   database is named when it first starts, and lemonfiber claims it before anyone
   else can reach it.
3. **Add a monitor for each thing you would want to know had gone**, and the
   notification channels you want to hear from it on.

## What it changes on your machine

Everything below is in your stack directory, or is a setting of the Uptime Kuma
container lemonfiber writes:

| What | Where |
| --- | --- |
| Uptime Kuma's container | `compose/plugins/uptime-kuma.yml`, in a Compose profile of its own, `plugin-uptime-kuma` |
| Uptime Kuma's own data: its database, uploads and notification settings | `config/uptime-kuma`, which Uptime Kuma sees as `/app/data` |
| Network | Port 3001 on `127.0.0.1`, so it is reachable from this machine only |
| Dashboard | An Uptime Kuma link in the Automation group of `config/homepage/services.yaml` |

Uptime Kuma gets no access to your data root or your library, and no site in
the stack's proxy.

A plugin cannot ask for more than this. lemonfiber writes the container itself,
and the plugin format has no field for another mount, another address or a line
of proxy configuration.

## What it sends anywhere

- **The image.** lemonfiber downloads Uptime Kuma from
  `docker.io/louislam/uptime-kuma`, pinned to one exact build
  (`sha256:917318f9d7be5257f43ba412c766a473be336eb451d70744f3b482d0c3997c0e`).
  If you have switched off downloading with `LEMONFIBER_REACH_REGISTRY`, nothing
  is downloaded and the image has to be on this machine already.
- **Nothing else from this plugin.** It names no host outside your stack,
  captures no password or key, and changes no setting of the services lemonfiber
  bundles. Its doctor checks ask only your own Uptime Kuma.
- **Uptime Kuma itself** reaches whatever you set it to watch, and sends
  notifications through whatever channels you set up in it. It keeps those
  channels' credentials in its own data, and lemonfiber reads none of them.

## Update

Get the newer version of this repository, with `git pull` or by checking out
the newer revision the catalogue lists, then update from it:

```sh
git -C plugin-uptime-kuma pull
lemonfiber plugin update ./plugin-uptime-kuma --dry-run
lemonfiber plugin update ./plugin-uptime-kuma
```

The new version is checked and proved the way an install is. Your machine is on
the old version or the new one at every moment. If the new one does not hold,
lemonfiber puts the old one back and says which version you are on. Uptime
Kuma's own data in `config/uptime-kuma` stays where it is.

## Remove

```sh
lemonfiber plugin remove uptime-kuma --dry-run
lemonfiber plugin remove uptime-kuma
```

Removing stops Uptime Kuma and takes out everything the install wrote: the
container and the dashboard link. If you have edited the dashboard link by hand
since, lemonfiber refuses rather than overwrite your edit.

Uptime Kuma's own data in `config/uptime-kuma` stays. lemonfiber does not delete
a directory holding something it did not put there, and it lists what it left.

## Getting help

- **A question:** ask on [Discord](https://discord.nightworks.io).
- **Something wrong with installing, updating, removing or the doctor checks:**
  [open an issue on this repository](https://github.com/lemonfiber/plugin-uptime-kuma/issues).
  `lemonfiber support` shows what a support bundle would hold, with passwords
  and keys replaced; `lemonfiber support --write` writes it for you to attach.
  It sends nothing anywhere by itself.
- **Something wrong in Uptime Kuma itself:**
  [Uptime Kuma's own project](https://github.com/louislam/uptime-kuma).

## Licence

The plugin data and the adapter in this repository are under the Hippocratic License 3.0
(HL3-CORE); see [LICENSE](LICENSE). Uptime Kuma itself is under the MIT licence
and is not distributed here: this repository names an image, it does not
contain one.

## Working on this plugin

The repository also holds `uptime-kuma-adapter`, the adapter that speaks
lemonfiber's `owner.claim` contract in front of Uptime Kuma, built and published
from here. `plugin.toml` runs it beside Uptime Kuma. How to work on either, and the
checks CI runs: [AGENTS.md](AGENTS.md).
