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
- **Two more checks in `lemonfiber doctor`**, each with a fix when it fails:

| Check | What it notices |
| --- | --- |
| Uptime Kuma has been set up, rather than waiting at its first-run screen | Uptime Kuma is running but nobody has set it up, so it is watching nothing |
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
- **No accounts anywhere else.** You create Uptime Kuma's administrator account
  in Uptime Kuma itself. Any notification channel you want it to use, you set up
  in Uptime Kuma too.

## Install

Get a copy of this repository, then ask lemonfiber what installing it would do.
`--dry-run` settles everything a real install settles and writes nothing:

```sh
git clone https://github.com/lemonfiber/plugin-uptime-kuma.git
lemonfiber plugin install plugin-uptime-kuma --dry-run
lemonfiber plugin install plugin-uptime-kuma
```

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

`lemonfiber plugin installed` lists what is installed, where each plugin came
from, and how each of its services is reached.

## Set it up

1. **Open Uptime Kuma** from the dashboard, or at `http://127.0.0.1:3001` on
   this machine.
2. **Finish its first-run screen.** It creates Uptime Kuma's database and its
   administrator account. Until then, `lemonfiber doctor` reports that it is
   watching nothing.
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
lemonfiber plugin update plugin-uptime-kuma --dry-run
lemonfiber plugin update plugin-uptime-kuma
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

The plugin data in this repository is under the Hippocratic License 3.0
(HL3-CORE); see [LICENSE](LICENSE). Uptime Kuma itself is under the MIT licence
and is not distributed here: this repository names an image, it does not
contain one.

## Working on this plugin

How the manifest is built, what each proof establishes, and how to run the
checks CI runs: [docs/development.md](docs/development.md).
