# fafnirx-fetch

The fetch [module](../../../docs/modules.md) of fafnirx, as a small service of its own: it reads what a source offers (addresses of
files, and RSS or Atom feeds whose entries link to files) and writes one file at a time for the master, over plain HTTP(S). It has no
database and no users; the master decides what is new, hashes and commits the files, and puts its own login in front of everything.
It is also the reference for [writing a fetch plugin](../../../docs/plugins.md).

```sh
FAFNIRX_PLUGIN_SECRET=<16+ characters> FAFNIRX_DATA=/data fafnirx-fetch
```

It writes only directly in `<FAFNIRX_DATA>/tmp`. All settings, the API and the legal notes: [docs/fetch.md](../../../docs/fetch.md). The
contract it implements: `crates/fetch-api` (`fetch.<kind>`).

## Layout

| File | |
|---|---|
| `module.rs` | the kit's `Fetcher` for the kind `web` (the service, the token guard, `/status` and the web part's files are `crates/module-kit`) |
| `settings.rs` | `POST /fetch/check`: the settings a user gave, checked, reduced to what is read, and labelled |
| `web.rs` | the kind `web`: listing addresses and feeds, markers from what the server says, downloading |
