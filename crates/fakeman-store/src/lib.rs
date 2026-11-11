//! SQLite persistence for fakeman: schema, migrations and the endpoint repository.
//!
//! The store keeps its own plain records and does not depend on `fakeman-core`.

mod records;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use rusqlite_migration::{M, Migrations};

pub use records::{
    BodyFormat, EndpointId, EndpointRecord, EndpointSummary, FieldRecord, HeaderRecord,
    UnknownBodyFormat,
};

const MIGRATION_STEPS: &[M<'static>] = &[M::up(include_str!("../migrations/0001_initial.sql"))];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(MIGRATION_STEPS);

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    #[error("invalid generator params: {0}")]
    Params(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no OS data directory is available")]
    NoDataDir,
    #[error("endpoint {0} does not exist")]
    NotFound(EndpointId),
    #[error("{0} is out of range for the database")]
    OutOfRange(&'static str),
    #[error("reorder must list every endpoint exactly once")]
    InvalidOrder,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The database location in the OS data directory:
/// `%APPDATA%\\fakeman\\fakeman.db` on Windows and
/// `~/Library/Application Support/fakeman/fakeman.db` on macOS.
pub fn default_db_path() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new().ok_or(Error::NoDataDir)?;
    Ok(dirs.data_dir().join("fakeman").join("fakeman.db"))
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens the database at [`default_db_path`], creating its directory.
    pub fn open_default() -> Result<Self> {
        let path = default_db_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::open(path)
    }

    /// Opens or creates the database at `path` and migrates it to the latest
    /// schema. A database created by this call gets the sample endpoint.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let (mut store, first_launch) = Self::init(conn)?;
        if first_launch {
            store.seed()?;
        }
        Ok(store)
    }

    /// Opens an empty, unseeded in-memory database.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::init(Connection::open_in_memory()?)?.0)
    }

    /// Returns the migrated store and whether the database was brand new.
    fn init(mut conn: Connection) -> Result<(Self, bool)> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok((Store { conn }, version == 0))
    }

    fn seed(&mut self) -> Result<()> {
        if self
            .conn
            .query_row("SELECT count(*) FROM endpoints", [], |r| r.get::<_, i64>(0))?
            > 0
        {
            return Ok(());
        }
        let mut endpoint = self.create("Create user")?;
        endpoint.url = "https://httpbin.org/post".into();
        endpoint.fields = [
            ("firstname", "person.firstName"),
            ("lastname", "person.lastName"),
            ("email", "internet.email"),
        ]
        .into_iter()
        .map(|(name, generator_key)| FieldRecord {
            name: name.into(),
            generator_key: generator_key.into(),
            params: serde_json::json!({}),
            enabled: true,
        })
        .collect();
        self.save(&endpoint)
    }

    /// All endpoints in sidebar order.
    pub fn list_endpoints(&self) -> Result<Vec<EndpointSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, method, url, sort_order FROM endpoints ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(EndpointSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                method: r.get(2)?,
                url: r.get(3)?,
                sort_order: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_endpoint(&self, id: EndpointId) -> Result<EndpointRecord> {
        load(&self.conn, id)
    }

    /// Creates an endpoint with default settings at the end of the list.
    pub fn create(&mut self, name: &str) -> Result<EndpointRecord> {
        self.conn.execute(
            &format!(
                "INSERT INTO endpoints (name, sort_order, created_at, updated_at)
                 VALUES (?1, (SELECT coalesce(max(sort_order) + 1, 0) FROM endpoints), {NOW}, {NOW})"
            ),
            [name],
        )?;
        self.get_endpoint(self.conn.last_insert_rowid())
    }

    /// Writes the endpoint row and replaces its fields and headers in one
    /// transaction. `sort_order` and `created_at` are left unchanged.
    pub fn save(&mut self, endpoint: &EndpointRecord) -> Result<()> {
        let tx = self.conn.transaction()?;
        let run_count = i64::from(endpoint.run_count);
        let delay_ms =
            i64::try_from(endpoint.delay_ms).map_err(|_| Error::OutOfRange("delay_ms"))?;
        let updated = tx.execute(
            &format!(
                "UPDATE endpoints SET name = ?2, method = ?3, url = ?4, body_format = ?5,
                 run_count = ?6, delay_ms = ?7, updated_at = {NOW} WHERE id = ?1"
            ),
            params![
                endpoint.id,
                endpoint.name,
                endpoint.method,
                endpoint.url,
                endpoint.body_format,
                run_count,
                delay_ms
            ],
        )?;
        if updated == 0 {
            return Err(Error::NotFound(endpoint.id));
        }
        tx.execute("DELETE FROM fields WHERE endpoint_id = ?1", [endpoint.id])?;
        tx.execute("DELETE FROM headers WHERE endpoint_id = ?1", [endpoint.id])?;
        insert_children(&tx, endpoint.id, &endpoint.fields, &endpoint.headers)?;
        Ok(tx.commit()?)
    }

    pub fn rename(&mut self, id: EndpointId, name: &str) -> Result<()> {
        let updated = self.conn.execute(
            &format!("UPDATE endpoints SET name = ?2, updated_at = {NOW} WHERE id = ?1"),
            params![id, name],
        )?;
        if updated == 0 {
            return Err(Error::NotFound(id));
        }
        Ok(())
    }

    /// Deep-copies an endpoint as "<name> copy", placed right after the original.
    pub fn duplicate(&mut self, id: EndpointId) -> Result<EndpointRecord> {
        let tx = self.conn.transaction()?;
        let source = load(&tx, id)?;
        tx.execute(
            "UPDATE endpoints SET sort_order = sort_order + 1 WHERE sort_order > ?1",
            [source.sort_order],
        )?;
        tx.execute(
            &format!(
                "INSERT INTO endpoints
                   (name, method, url, body_format, run_count, delay_ms, sort_order, created_at, updated_at)
                 SELECT name || ' copy', method, url, body_format, run_count, delay_ms, sort_order + 1, {NOW}, {NOW}
                 FROM endpoints WHERE id = ?1"
            ),
            [id],
        )?;
        let copy_id = tx.last_insert_rowid();
        insert_children(&tx, copy_id, &source.fields, &source.headers)?;
        tx.commit()?;
        self.get_endpoint(copy_id)
    }

    /// Deletes an endpoint; its fields and headers go with it.
    pub fn delete(&mut self, id: EndpointId) -> Result<()> {
        if self
            .conn
            .execute("DELETE FROM endpoints WHERE id = ?1", [id])?
            == 0
        {
            return Err(Error::NotFound(id));
        }
        Ok(())
    }

    /// Sets the sidebar order. `ids` must list every endpoint exactly once.
    pub fn reorder(&mut self, ids: &[EndpointId]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut existing: Vec<EndpointId> = tx
            .prepare("SELECT id FROM endpoints ORDER BY id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut requested = ids.to_vec();
        requested.sort_unstable();
        existing.sort_unstable();
        if requested != existing {
            return Err(Error::InvalidOrder);
        }
        {
            let mut stmt = tx.prepare("UPDATE endpoints SET sort_order = ?2 WHERE id = ?1")?;
            for (position, id) in (0_i64..).zip(ids) {
                stmt.execute(params![id, position])?;
            }
        }
        Ok(tx.commit()?)
    }
}

fn load(conn: &Connection, id: EndpointId) -> Result<EndpointRecord> {
    let row = conn
        .query_row(
            "SELECT id, name, method, url, body_format, run_count, delay_ms, sort_order, created_at, updated_at
             FROM endpoints WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    EndpointRecord {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        method: r.get(2)?,
                        url: r.get(3)?,
                        body_format: r.get(4)?,
                        run_count: r.get(5)?,
                        delay_ms: 0,
                        sort_order: r.get(7)?,
                        created_at: r.get(8)?,
                        updated_at: r.get(9)?,
                        fields: Vec::new(),
                        headers: Vec::new(),
                    },
                    r.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?;
    let (mut endpoint, delay_ms) = row.ok_or(Error::NotFound(id))?;
    endpoint.delay_ms = u64::try_from(delay_ms).map_err(|_| Error::OutOfRange("delay_ms"))?;

    let mut stmt = conn.prepare(
        "SELECT name, generator_key, params_json, enabled FROM fields
         WHERE endpoint_id = ?1 ORDER BY position, id",
    )?;
    let fields = stmt.query_map([id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?, r.get(3)?))
    })?;
    for field in fields {
        let (name, generator_key, params, enabled) = field?;
        endpoint.fields.push(FieldRecord {
            name,
            generator_key,
            params: serde_json::from_str(&params)?,
            enabled,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT key, value, enabled FROM headers WHERE endpoint_id = ?1 ORDER BY position, id",
    )?;
    endpoint.headers = stmt
        .query_map([id], |r| {
            Ok(HeaderRecord {
                key: r.get(0)?,
                value: r.get(1)?,
                enabled: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(endpoint)
}

fn insert_children(
    tx: &Transaction<'_>,
    id: EndpointId,
    fields: &[FieldRecord],
    headers: &[HeaderRecord],
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO fields (endpoint_id, position, name, generator_key, params_json, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (position, field) in (0_i64..).zip(fields) {
        stmt.execute(params![
            id,
            position,
            field.name,
            field.generator_key,
            serde_json::to_string(&field.params)?,
            field.enabled
        ])?;
    }
    let mut stmt = tx.prepare(
        "INSERT INTO headers (endpoint_id, position, key, value, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (position, header) in (0_i64..).zip(headers) {
        stmt.execute(params![
            id,
            position,
            header.key,
            header.value,
            header.enabled
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_valid() {
        super::MIGRATIONS.validate().unwrap();
    }
}
