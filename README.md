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

A single, global SQLite database. This may change so that each namespace gets its own SQLite database, but maybe not.
