use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid profile id")]
    InvalidProfileId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    pub state: Value,
}

/// SQLite persistence compatible with the C# `profiles` table.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    db_path: PathBuf,
}

impl ProfileStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        fs::create_dir_all(root.as_ref())?;
        let store = Self {
            db_path: root.as_ref().join("profiles.db"),
        };
        let connection = store.connection()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                state_json TEXT NOT NULL,
                updated_utc TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                account_json TEXT NOT NULL,
                updated_utc TEXT NOT NULL
            );",
        )?;
        Ok(store)
    }

    pub fn load(&self, profile_id: &str) -> Result<Option<StoredProfile>, StorageError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT id, name, state_json FROM profiles WHERE id = ?1",
                params![profile_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(id, name, state_json)| {
            Ok(StoredProfile {
                id,
                name,
                state: serde_json::from_str(&state_json)?,
            })
        })
        .transpose()
    }

    pub fn load_account(&self, profile_id: &str) -> Result<Option<Value>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT account_json FROM accounts WHERE id = ?1",
                params![profile_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(StorageError::from)
    }

    pub fn save(&self, profile_id: &str, name: &str, state: &Value) -> Result<(), StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let state_json = serde_json::to_string(state)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO profiles(id, name, state_json, updated_utc)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               state_json = excluded.state_json,
               updated_utc = excluded.updated_utc",
            params![profile_id, name, state_json, timestamp()],
        )?;
        Ok(())
    }

    pub fn save_account(&self, profile_id: &str, account: &Value) -> Result<(), StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let account_json = serde_json::to_string(account)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO accounts(id, account_json, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               account_json = excluded.account_json,
               updated_utc = excluded.updated_utc",
            params![profile_id, account_json, timestamp()],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id FROM profiles ORDER BY id")?;
        let profiles = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(profiles)
    }

    pub fn list_accounts(&self) -> Result<Vec<(String, Value)>, StorageError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT id, account_json FROM accounts ORDER BY id")?;
        let accounts = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        accounts
            .into_iter()
            .map(|(id, account_json)| Ok((id, serde_json::from_str::<Value>(&account_json)?)))
            .collect()
    }

    pub fn reset(&self, profile_id: &str) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;
        transaction.execute("DELETE FROM accounts WHERE id = ?1", params![profile_id])?;
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, StorageError> {
        Ok(Connection::open(&self.db_path)?)
    }
}

fn is_valid_profile_id(profile_id: &str) -> bool {
    !profile_id.is_empty()
        && profile_id.len() <= 64
        && profile_id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        })
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
