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
    /// Value can be either a string, or a file read from stdin,
    /// like `blade set key < file.txt`
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
    /// Delete namespace and all its keys
    DeleteNamespace { namespace: String },
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
    conn.pragma_update(None, "foreign_keys", true)?;

    #[cfg(target_os = "macos")]
    conn.pragma_update(None, "fullfsync", true)?;

    Ok(conn)
}

fn migrate_db(conn: Connection) -> anyhow::Result<Connection> {
    conn.execute_batch(
        "
        begin;

        create table if not exists namespaces (
            id integer primary key,
            namespace text not null,
            inserted_at datetime not null default(strftime('%Y-%m-%d %H:%M:%f', 'NOW'))
        );

        create unique index if not exists idx_namespaces_namespace on namespaces (namespace);

        create table if not exists entries (
            namespace_id integer not null,
            key text not null,
            value blob not null,
            inserted_at datetime not null default(strftime('%Y-%m-%d %H:%M:%f', 'NOW')),
            primary key (namespace_id, key),

            foreign key (namespace_id) references namespaces(id) on delete cascade
        ) without rowid;

        create index if not exists idx_entries_namespace_id on entries (namespace_id);

        create trigger if not exists delete_empty_namespace
        after delete on entries
        for each row
        when not exists (select 1 from entries where namespace_id = old.namespace_id)
        begin
            delete from namespaces where id = old.namespace_id and namespace <> 'default';
        end;

        commit;
    ",
    )?;
    Ok(conn)
}

struct Key<'input> {
    namespace: &'input str,
    name: &'input str,
}

fn split_maybe_qualified_key(maybe_qualified_key: &str) -> anyhow::Result<Key<'_>> {
    if maybe_qualified_key.trim().is_empty() {
        return Err(anyhow!("key cannot be empty"));
    }

    let mut split = maybe_qualified_key.split("@");

    match (split.next(), split.next()) {
        (Some(name), None) => Ok(Key {
            namespace: DEFAULT_NAMESPACE,
            name,
        }),
        (Some(name), Some(namespace)) => {
            if name.trim().is_empty() {
                Err(anyhow!("key cannot be empty"))
            } else if namespace.trim().is_empty() {
                Err(anyhow!("namespace cannot be empty"))
            } else {
                Ok(Key { namespace, name })
            }
        }
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

    let mut conn = migrate_db(conn)?;

    match options.command {
        Command::Get { namespaced_key } => {
            let key = split_maybe_qualified_key(&namespaced_key)?;

            let mut q = conn.prepare(
                "
            select
                value
            from entries
            inner join namespaces
                on namespaces.id = entries.namespace_id
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
            insert into entries (namespace_id, key, value)
            values (?, ?, ?)
            on conflict do update
            set
                value = excluded.value,
                inserted_at = strftime('%Y-%m-%d %H:%M:%f', 'NOW')
            where namespace_id = excluded.namespace_id
            and key = excluded.key;
            ";

            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            let namespace_id: i64 = {
                let mut namespace_q = tx.prepare(
                    "insert into namespaces (namespace) values (?)
                    on conflict do update
                        set namespace = excluded.namespace
                    returning id",
                )?;

                namespace_q.query_one([key.namespace], |row| row.get(0))?
            };

            if let Some(value) = value {
                tx.execute(SET_QUERY, params![namespace_id, key.name, value.as_bytes()])?;
            } else {
                let mut value = vec![];

                std::io::stdin().read_to_end(&mut value)?;

                tx.execute(SET_QUERY, params![namespace_id, key.name, value])?;
            }

            tx.commit()?;
        }
        Command::Delete { namespaced_key } => {
            let key = split_maybe_qualified_key(&namespaced_key)?;

            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            let namespace_id: Option<i64> = tx
                .query_one(
                    "select id from namespaces where namespace = ?",
                    [key.namespace],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(namespace_id) = namespace_id {
                tx.execute(
                    "
                    delete from entries
                    where namespace_id = ?
                    and key = ?
                    ",
                    params![namespace_id, key.name],
                )?;
            }

            tx.commit()?;
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
                inner join namespaces
                    on namespaces.id = entries.namespace_id
                where namespace = ?
                order by entries.inserted_at desc
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
                namespace
            from namespaces
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
        Command::DeleteNamespace { namespace } => {
            conn.execute("delete from namespaces where namespace = ?", [namespace])?;
        }
        Command::DumpConfig => {
            let s = toml::to_string_pretty(&config)?;
            let mut out = std::io::stdout();
            writeln!(out, "{}", s)?;
        }
    }

    Ok(())
}
