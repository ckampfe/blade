# blade

A system-wide key-value database for use in scripts. Shameless port of https://github.com/charmbracelet/skate

[![Rust](https://github.com/ckampfe/blade/actions/workflows/rust.yml/badge.svg)](https://github.com/ckampfe/blade/actions/workflows/rust.yml)

## Examples

```
$ blade set a 1
$ blade get a
1
$ blade set b 2
$ blade list
b	2
a	1
$ blade delete a
$ blade list
b	2
```

You can also use namespaces, which are entirely separate keyspaces:

```bash
$ blade set a@ns1 hi
$ blade set a@ns2 bye
$ blade get a@ns1
hi
$ blade get a@ns2
bye
$ blade list ns1
a	hi
$ blade list ns2
a	bye
$ blade list-namespaces
default
ns1
ns2
```

## Install

```
cargo install blade --git https://github.com/ckampfe/blade
```

## API

```
$ blade help
Usage: blade [DB_LOCATION] <COMMAND>

Commands:
  get              Get a key. `key[@namespace]`
  set              Set a key. `key[@namespace]`. Value can be either a string, or a file read from stdin
  delete           Delete a key. `key[@namespace]`
  list             List all keys. Optionally with namespace and delimiter (default: `\t`)
  list-namespaces  List all namespaces
  dump-config      Print the current config
  help             Print this message or the help of the given subcommand(s)

Arguments:
  [DB_LOCATION]  Optional. Setting this environment variable overrides the db location set in the config file. If not set, uses the location set in the config file: ~/.config/blade/config.toml [env: DB_LOCATION=]

Options:
  -h, --help  Print help

```

## Configuration

A configuration file will be created at `~/.config/blade/config.toml`. On my Mac it looks like this, but the `db_location` will vary on Linux and Windows based on the XDG spec.

```
db_location = "/Users/clark/Library/Application Support/blade/blade.db"
sqlite_synchronous_mode = "normal"
sqlite_busy_timeout_ms = 5000
```

If you want system crash/power failure durability, change `sqlite_synchronous_mode` to `"full"`.

The `db_location` configuration setting can be overriden by setting the `DB_LOCATION` environment variable when calling `blade`. This is useful if you want to create a special one-off database or test something out, but the config file `db_location` is used by default because `blade` is intended to be global.

## Design

All key/values live in a namespace. There can be an arbitrary number of namespaces, and keys are unique per namespace.
If no namespace is provided, this namespace is `default`. All of the commands work on the `default` namespace by default.

Right now, all namespaces live in a single table in a single global SQLite database.
This may change so that each namespace gets its own SQLite database, but maybe not.

The current approach benefits from being incredibly easy to understand, manage, and backup, at the expense of having a single global writer at a time. This really shouldn't be a problem as this model of KV interaction is not designed for "webscale" write throughput.

Going to a database-per-namespace approach benefits from having `N` "physically separate" database files that do not block each other, at the expense of a proliferation of databases per namespace, making database management more annoying.

## Credit

This is not my idea! I stole it from https://github.com/charmbracelet/skate and wanted to try to implement my own version with some tweaks. Thanks to the Charm folks for the idea.