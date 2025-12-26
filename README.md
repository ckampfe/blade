# blade

A personal CLI key-value database. Shameless port of https://github.com/charmbracelet/skate

## Use

```
$ blade help
Usage: blade <COMMAND>

Commands:
  get              Get a key. `key[@namespace]`
  set              Set a key. `key[@namespace]`. Value can be either a string, or a file read from stdin
  delete           Delete a key. `key[@namespace]`
  list             List all keys. Optionally with namespace and delimiter (default: `\t`)
  list-namespaces  List all namespaces
  dump-config      Print the current config
  help             Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

```

## Design

All key/values live in a namespace. There can be an arbitrary number of namespaces, and keys are unique per namespace.
If no namespace is provided, this namespace is `default`. All of the commands work on the `default` namespace by default.

Right now, all namespaces live in a single table in a single global SQLite database.
This may change so that each namespace gets its own SQLite database, but maybe not.

The current approach benefits from being incredibly easy to understand, manage, and backup, at the expense of having a single global writer at a time. This really shouldn't be a problem as this model of KV interaction is not designed for "webscale" write throughput.

Going to a database-per-namespace approach benefits from having `N` "physically separate" database files that do not block each other, at the expense of a proliferation of databases per namespace, making database management more annoying.
