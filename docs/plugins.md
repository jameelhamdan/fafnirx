# Plugins: writing a fetch plugin

A **fetch plugin** teaches the master a new kind of [source](sources.md): a site, a protocol, a service with its own API. The master
keeps the database and the schedule; the plugin only answers three questions about a source: *are these settings usable?*, *what does it
offer right now?* and *write this one file here*. Anyone can write one, in any language, and run it as a container next to their master. The
[fetch module](sources.md#web-files-and-feeds) (`modules/fetch/service`) is the reference: read it next to this page.

A plugin has two parts, both optional except the first:

| Part | What it is | Where it runs |
|---|---|---|
| **Service** | a program that speaks HTTP (below) | in a container next to the master |
| **Web part** | a script that adds the source's form to the Sources page, and its translations | in the browser, served by the service |

## Connect it

A plugin is a container image you build (or pull) and run next to the master; the master never installs or starts it, and has nothing
to configure for it beyond the shared secret it already has (`FAFNIRX_PLUGIN_SECRET`). The container is given:

```sh
FAFNIRX_PLUGIN_SECRET=<the master's secret>      # joins with it, and the master calls the plugin with it
FAFNIRX_MASTER_URL=http://master:8080            # the master's web address
FAFNIRX_PLUGIN_URL=http://myplugin:8095          # where the master reaches this plugin
FAFNIRX_PLUGIN_LISTEN=0.0.0.0:8095
```

The plugin announces itself: `POST <master>/api/plugins/register` with `{"id", "address"}` and the secret as bearer token, at the start
and every 30 s (every 5 s while the master does not take it). The master asks `GET /manifest`, uses the plugin while its `id` and `contract`
match, lists it under *Settings → Plugins*, and forgets it when it has been silent for 90 s. The module kit does all of this: start the
container and it is connected.

- **Settings of a plugin** are variables of its own container (`FAFNIRX_MYPLUGIN_JOBS`); nothing of the master's environment (database,
  passwords) reaches it.
- **Data:** give the container only `data/tmp` of the master's data folder, **at the same path as the master** (`/data/tmp`), because the
  master names files by it. Keep its port off the network: it has no login, only the secret. The compose file of the
  [fetch module](sources.md#run-it) is the model; a plugin in another repository ships its own compose overlay for it.
- **Trust:** whoever has the secret can announce a plugin, and a plugin is a program with write access to `data/tmp`. Run what you trust.
- **Build:** one Dockerfile per plugin, against the `fafnirx-module-kit` (and `fafnirx-fetch-api`) crates of this repository (path or git
  dependency). `docker/Dockerfile.fetch` is the example. Push the image to any registry your servers pull from.

## The contract

HTTP with JSON. The types are in `crates/module-api` (everything modules share) and `crates/fetch-api` (this page); from Rust depend on
them by path or git, from any other language read the JSON below. Every request carries `Authorization: Bearer <token>` (the service's
`FAFNIRX_PLUGIN_SECRET`, 16+ characters) except `GET /health`; answer `401` otherwise. The master has checked the user's login and rights:
a plugin has no users.

| Request | When | Answer |
|---|---|---|
| `GET /health` | container health checks | `200`, no token needed |
| `GET /manifest` | on start, then every few seconds until it answers | the manifest |
| `POST /fetch/check` | a user adds a source | `{config, label}` or `400` with a sentence |
| `POST /fetch/list` | a source runs | the listing |
| `POST /fetch/get` | for each new file | `{size}` |
| `GET /status` | the Processing page | what is being done (optional, capability `status`) |
| `GET /ui/<file>` | the browser, through the master | the web part's files (optional) |

**Manifest**

```json
{ "id": "myplugin", "version": "1.2.0", "contract": 1, "core": "3.20.0",
  "capabilities": ["fetch.ftp", "status"],
  "ui": { "entry": "index.js", "css": "style.css", "sdk": 2 },
  "setup": ["curl was not found: install it"] }
```

`id` is what the plugin announces itself as: lower-case letters and digits, up to 24. `contract` must be the master's
(`fafnirx_module_api::CONTRACT`); another number and the module is not used. `core` is the oldest master it works with: the version of the contract
crates (`fafnirx_module_api::VERSION`) it was built against, which `Manifest::new` fills in; a master older than that does not use it
and says why in its log. A kind of source is the capability **`fetch.<kind>`**
(`fetch.ftp` for the kind `ftp`): list one only when you can read it now. A module that is up without it shows the kind as *Needs setting up*.
`setup` lists, in words for the administrator, what is missing; it is shown under *Settings*. `ui.sdk` is the web SDK version
(`fafnirx_module_api::WEB_SDK`); omit `ui` if there is no web part.

**`POST /fetch/check`** `{ "kind": "ftp", "config": {…} }` → `{ "config": {…}, "label": "ftp://host/dir" }`

`config` is what the user typed in your form. Check it, drop what you don't read, normalise it (trim, defaults) and answer with the
version you want to keep; the master stores it and sends it back with every later call. `label` is what the Sources page shows. If the
settings are unusable answer **`400` with one plain sentence** for the user as the body (`Link: a web address starting with http://`): the
master shows it as it is. The master checks two things itself and adds them to the stored settings: the confirmation that the user may
copy what the source downloads (`acknowledged`) and the limit of files per run (`max_per_run`); **ignore keys you don't know**.

**`POST /fetch/list`** `{ "kind", "config" }` → 

```json
{ "items": [ { "path": "Photos/2024/a.jpg", "marker": "etag-1", "size": 1048576, "mtime_ns": 1730000000000000000,
               "reference": { "id": "42" } } ],
  "skipped": 0, "complete": true }
```

- `path`: where the file goes in the source's folder: relative, `/`-separated, no `.`/`..`/empty parts, no control characters, each part
  at most 255 bytes. The master rejects the entry (and counts it as skipped) otherwise, and also a path that appears twice.
- `key` (optional): what the file is, whatever its path or format (a video id). The same key under a new `path` is the same
  file renamed: the master moves it and fetches nothing; with a new `marker` it is made again at the new path and the old one is
  removed. Without a key the path is the identity.
- `marker`: any string that **changes when the file the source would make changes** (an entity tag, a version, a format). The master
  fetches an entry again only when its marker or path is new. Don't put the time of the listing in it.
- `size` (0 if unknown), `mtime_ns` (Unix nanoseconds; when it was last changed, or now): shown to users.

In Rust, entries and listings are made with `FetchEntry::new(path, marker).key(..).size(..).mtime_ns(..).reference(..)` and `FetchListing::new(items).skipped(..).complete(..)`; the types are `#[non_exhaustive]`, so a field added to the contract later never breaks a plugin that is built again.
- `reference`: whatever you need to make the file later (an id, a URL); the master hands it back unread.
- `skipped`: entries you could not offer (not available, or not yet); they are tried again next run.
- `complete`: **true only if the listing is the whole truth.** When it is false, or when `items` is empty, the master deletes nothing
  for files missing from it. Say false for a feed that shows only its latest entries, for a listing cut short, for anything where "not
  listed" could mean "not read" rather than "gone".
- The master waits up to 15 minutes. Long listings should run their lookups in parallel and stop themselves in time.

**`POST /fetch/get`** `{ "kind", "config", "entry": {…as listed…}, "dest": "/data/tmp/3f9c…" }` → `{ "size": 1048576 }`

Write the file to `dest` and answer when it is **complete**. `dest` is a file directly in `<FAFNIRX_DATA>/tmp`: check that it is (resolve
the path and compare the folder) and **write nowhere else**; use a work folder next to it (`<dest>.work/`) and rename into place, and
leave nothing behind on failure. Answer `5xx` with the reason as the body on failure (the last line of what went wrong: it is shown on
the Sources page). The master hashes and commits the file, and fails the run if `size` is 0 or differs from what it received. A call may
take hours (24 h at most); a service should give up by itself when nothing arrives for a while.

Be **stateless**: no database, nothing that must survive between calls except caches you can rebuild. The master sends the settings with
every call and may call `list` and `get` of different sources at the same time; limit your own concurrency (`FAFNIRX_<ID>_JOBS`).

**A service in Rust** uses the module kit (`crates/module-kit`, `fafnirx-module-kit`), which is all of the above except what the plugin
knows: the token guard, the manifest, `/status`, the web part's files, the `dest` check, the list of what is being done, a scratch folder
and the settings every plugin reads from the environment (`FAFNIRX_PLUGIN_SECRET`, `FAFNIRX_PLUGIN_LISTEN`, `FAFNIRX_PLUGIN_UI`,
`FAFNIRX_DATA`, `FAFNIRX_<NAME>_JOBS`, `FAFNIRX_<NAME>_IDLE_SECS`). The plugin implements `Fetcher` (its kind, `check`, `parse`, `list`, `get`,
and optionally `readiness` when it needs a program) and calls `serve`. The kit also has the small tools a plugin needs: `process` (run a program
and kill it when it stands still), `settings` (read and check what a user typed), `xml` (feeds), `names` (`sanitize_name`) and `http`. With the
`testing` feature, `testing::Fixture` serves a module over a data folder, with a small website and stand-in programs to read from. `modules/fetch/service` is the example: `module.rs` is the `Fetcher`, `settings.rs` and `web.rs` are its own logic.

## The web part

The service serves a script (an ES module) and optionally a style sheet under `/ui/`; the master passes them to the browser after
checking the login, while the module is up. The default export tells the app what the plugin adds:

```ts
import { defineSourceKind, FetchQueueCard, msg, type FrontendModule } from '@/sdk'
import { MyForm, NO_FORM, toConfig, problem, type Form } from './form'

export default {
  sources: [
    defineSourceKind<Form>({
      id: 'ftp',                              // the kind: the service lists the capability fetch.ftp
      label: msg('FTP'),                      // in the list of services
      intro: msg('The master copies this folder on a schedule.'),
      initial: NO_FORM,                       // what the form starts with
      form: MyForm,                           // ({ value, onChange }) => JSX
      body: toConfig,                         // form value -> the `config` sent to /fetch/check
      problem: (value, t) => (value.url ? null : t('Give an address.')), // why it cannot be added yet, or null
    }),
  ],
  queue: FetchQueueCard,                        // optional: your part of the Downloads card on the Processing page, given GET /status
} satisfies FrontendModule
```

- **Translations** are JSON files like the app's own: `web/translations/<lang>.json` (`es`, `zh`, `ar`, `ru`, `ja`), English text → translation.
  The build serves them beside the script as `strings.<lang>.json` and the manifest lists the languages (`ui.strings`). The app fetches the
  file of the language in use when it is needed and merges it with its own strings; it never lets a plugin replace one of its strings, and a
  text without a translation shows in English.
- **Shared pieces:** the SDK has what every kind of source needs, so the plugin writes only what is its own: `FetchQueueCard` (your part of the
  one Downloads card, under a badge with the module's name; `children` add lines to it), and for the form `FileLimitsFields` (include and exclude patterns, sizes, files per run), `PerRunField`, `TextField`
  and `AcknowledgeField` (the confirmation to copy), with their body and checks (`limitsBody`, `limitsProblem`, `perRunProblem`,
  `acknowledgedProblem`, `splitLines`, `isWebAddress`). Their texts are the app's and are already translated.
- **Imports:** a plugin imports `react`, `@/sdk` and `@/components/ui/*` (buttons, inputs, cards…) and nothing else of the app; at run
  time those are the app's own copies, so the plugin shares its React, language and look. Anything else (an icon set) is bundled into
  the script. Build it as a library with those four imports left out: `frontend/vite.module.config.ts` is the recipe (it replaces them
  with reads from `window.__fafnirx`), and the style sheet imports `frontend/src/theme.css` so utility classes exist
  (`frontend/src/modules/video/style.css` shows how).
- A script built for another `WEB_SDK` number is not loaded. The Sources page offers the kind while the module is up and its manifest lists
  `fetch.<id>`; the form's value is yours, the master only ever sees what `body` returns.

## Test it

1. Unit-test `check`, the listing and `get` against a small server of your own, as `modules/fetch/service` does.
2. Start the service by hand and call it with `curl` and the bearer token; check `401` without the token and `400` for bad settings.
3. Start a throwaway master with your container connected (above): *Settings → Plugins* shows it *up*, the
   Sources page offers your kind, adding a source calls `/fetch/check`, the first run lists and fetches, and the files appear under
   Files. `.github/scripts/smoke-image.sh <master image> connect <your image> <id>` does the first half of this (for the ids it knows).
4. Change a file at the source and run again: it must come as a new version. Remove one: it must be deleted only when `complete` is true.
5. Kill the service in the middle of a `get`: no half file may be left in `data/tmp`.

Before you share a plugin, say in its README what it downloads from where, that the user must have the right to copy it, and under which
licence it and the tools in its image are distributed.
