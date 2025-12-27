use anyhow::anyhow;
use clap::{Parser, Subcommand};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

const DEFAULT_NAMESPACE: &str = "default";

#[derive(Parser)]
struct Options {
    /// Optional. Setting this environment variable overrides
    /// the db location set in the config file.
    /// If not set, uses the location set in the config file:
    /// ~/.config/blade/config.toml
    #[arg(env)]
    db_location: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Get a key. `key[@namespace]`
    Get { namespaced_key: String },
    /// Set a key. `key[@namespace]`.
    /// Value can be either a string, or a file read from stdin
    Set {
        namespaced_key: String,
        value: Option<String>,
    },
    /// Delete a key. `key[@namespace]`
    Delete { namespaced_key: String },
    /// List all keys. Optionally with namespace and delimiter (default: `\t`)
    List {
        namespace: Option<String>,
        #[arg(default_value = "\t")]
        delimiter: String,
    },
    /// List all namespaces
    ListNamespaces,
    /// Print the current config
    DumpConfig,
}

#[derive(Serialize, Deserialize)]
struct Config {
    db_location: PathBuf,
    sqlite_synchronous_mode: SqliteSynchronousMode,
    sqlite_busy_timeout_ms: i32,
}

impl Default for Config {
    fn default() -> Self {
        let mut db_location = directories::ProjectDirs::from("", "", "blade")
            .ok_or(anyhow!("could not retrieve home directory"))
            .unwrap()
            .data_local_dir()
            .to_path_buf();

        db_location.push("blade.db");

        Self {
            db_location,
            sqlite_synchronous_mode: SqliteSynchronousMode::default(),
            sqlite_busy_timeout_ms: 5_000,
        }
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SqliteSynchronousMode {
    Extra,
    Full,
    #[default]
    Normal,
    Off,
}

impl Display for SqliteSynchronousMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SqliteSynchronousMode::Extra => "extra",
            SqliteSynchronousMode::Full => "full",
            SqliteSynchronousMode::Normal => "normal",
            SqliteSynchronousMode::Off => "off",
        };

        write!(f, "{}", s)
    }
}

fn get_or_create_config_file() -> anyhow::Result<Config> {
    let mut config_path = {
        let mut config_path = directories::UserDirs::new()
            .ok_or(anyhow!("could not retrieve home directory"))?
            .home_dir()
            .to_path_buf();
        config_path.push(".config");
        config_path.push("blade");
        config_path
    };

    std::fs::create_dir_all(&config_path)?;

    config_path.push("config.toml");

    let config: Config = match std::fs::read_to_string(&config_path) {
        Ok(f) => toml::from_str(&f)?,
        Err(_) => {
            let mut f = std::fs::File::create_new(&config_path)?;

            let config = Config::default();

            let s = toml::to_string(&config)?;

            f.write_all(s.as_bytes())?;

            config
        }
    };

    Ok(config)
}

fn open_or_create_db(
    db_location: &Path,
    sqlite_synchronous_mode: SqliteSynchronousMode,
    sqlite_busy_timeout_ms: i32,
) -> anyhow::Result<rusqlite::Connection> {
    match open_db_connection(db_location, sqlite_synchronous_mode, sqlite_busy_timeout_ms) {
        Ok(c) => Ok(c),
        Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::CannotOpen,
                ..
            },
            _,
        )) => {
            let db_dir = db_location.parent().unwrap();
            std::fs::create_dir_all(db_dir)?;
            let conn =
                open_db_connection(db_location, sqlite_synchronous_mode, sqlite_busy_timeout_ms)?;
            Ok(conn)
        }
        Err(e) => Err(e)?,
    }
}

fn open_db_connection(
    path: &Path,
    sqlite_synchronous_mode: SqliteSynchronousMode,
    sqlite_busy_timeout_ms: i32,
) -> rusqlite::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.pragma_update(None, "synchronous", sqlite_synchronous_mode.to_string())?;
    conn.pragma_update(None, "busy_timeout", sqlite_busy_timeout_ms)?;
    Ok(conn)
}

fn migrate_db(conn: Connection) -> anyhow::Result<Connection> {
    conn.execute_batch(
        "
    create table if not exists entries (
        namespace text not null,
        key text not null,
        value blob not null,
        inserted_at datetime not null default(strftime('%Y-%m-%d %H:%M:%f', 'NOW')),
        updated_at datetime not null default(strftime('%Y-%m-%d %H:%M:%f', 'NOW')),
        primary key (namespace, key)
    ) without rowid;

    create trigger if not exists entries_updated_at
    after update on entries for each row
    begin
        update entries
        set updated_at = current_timestamp
        where namespace = old.namespace
        and key = old.key;
    end;
    ",
    )?;
    Ok(conn)
}

struct Key<'input> {
    namespace: &'input str,
    name: &'input str,
}

fn split_maybe_qualified_key(maybe_qualified_key: &str) -> anyhow::Result<Key<'_>> {
    let mut split = maybe_qualified_key.split("@");

    match (split.next(), split.next()) {
        (Some(name), None) => Ok(Key {
            namespace: DEFAULT_NAMESPACE,
            name,
        }),
        (Some(name), Some(namespace)) => Ok(Key { namespace, name }),
        _ => unreachable!(),
    }
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();

    let config = get_or_create_config_file()?;

    let conn = open_or_create_db(
        options.db_location.as_ref().unwrap_or(&config.db_location),
        config.sqlite_synchronous_mode,
        config.sqlite_busy_timeout_ms,
    )?;

    let conn = migrate_db(conn)?;

    match options.command {
        Command::Get { namespaced_key } => {
            let key = split_maybe_qualified_key(&namespaced_key)?;

            let mut q = conn.prepare(
                "
            select
                value
            from entries
            where namespace = ?
            and key = ?
            limit 1
            ",
            )?;

            let value: Option<Vec<u8>> = q
                .query_one([key.namespace, key.name], |row| row.get(0))
                .optional()?;

            if let Some(value) = value {
                if std::io::stdin().is_terminal() && std::str::from_utf8(&value).is_err() {
                    let mut out = std::io::stdout();
                    out.write_all(format!("binary data ({} bytes)\n", value.len()).as_bytes())?;
                } else {
                    let mut out = std::io::stdout();
                    out.write_all(&value)?;
                    out.write_all(b"\n")?;
                }
            };
        }
        Command::Set {
            namespaced_key,
            value,
        } => {
            let key = split_maybe_qualified_key(&namespaced_key)?;

            const SET_QUERY: &str = "
                    insert into entries (namespace, key, value)
                    values (?, ?, ?)
                    on conflict do update
                    set value = excluded.value
                    where namespace = excluded.namespace
                    and key = excluded.key;
                    ";

            if let Some(value) = value {
                conn.execute(
                    SET_QUERY,
                    params![key.namespace, key.name, value.as_bytes()],
                )?;
            } else {
                let mut value = vec![];

                std::io::stdin().read_to_end(&mut value)?;

                conn.execute(SET_QUERY, params![key.namespace, key.name, value])?;
            }
        }
        Command::Delete { namespaced_key } => {
            let key = split_maybe_qualified_key(&namespaced_key)?;

            conn.execute(
                "
                delete from entries
                where namespace = ?
                and key = ?
            ",
                [key.namespace, key.name],
            )?;
        }
        Command::List {
            namespace,
            delimiter,
        } => {
            let namespace = namespace.unwrap_or_else(|| DEFAULT_NAMESPACE.to_string());

            let mut q = conn.prepare(
                "
            select
                key,
                value
            from entries
            where namespace = ?
            order by inserted_at desc
            ",
            )?;

            let rows = q.query_map([namespace], |row| Ok((row.get(0)?, row.get(1)?)))?;

            let is_terminal = std::io::stdin().is_terminal();

            let mut out = std::io::stdout().lock();

            for row in rows {
                let (key, value): (String, Vec<u8>) = row?;

                if is_terminal && std::str::from_utf8(&value).is_err() {
                    out.write_all(key.as_bytes())?;
                    out.write_all(delimiter.as_bytes())?;
                    out.write_all(format!("binary data ({} bytes)\n", value.len()).as_bytes())?;
                } else {
                    out.write_all(key.as_bytes())?;
                    out.write_all(delimiter.as_bytes())?;
                    out.write_all(&value)?;
                    out.write_all(b"\n")?;
                }
            }
        }
        Command::ListNamespaces => {
            let mut q = conn.prepare(
                "
            select
                distinct namespace
            from entries
            order by namespace asc
            ",
            )?;

            let rows = q.query_map([], |row| row.get(0))?;

            let mut out = std::io::stdout().lock();

            for row in rows {
                let row: String = row?;
                writeln!(out, "{}", row)?;
            }
        }
        Command::DumpConfig => {
            let s = toml::to_string_pretty(&config)?;
            let mut out = std::io::stdout();
            writeln!(out, "{}", s)?;
        }
    }

    Ok(())
}
