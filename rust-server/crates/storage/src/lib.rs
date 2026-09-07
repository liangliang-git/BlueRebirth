use blueoath_domain::{AccountRepository, AccountState, ProfileId, RepositoryError};
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
    #[error("account revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
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
        run_migrations(&connection)?;
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
        self.save_account_with_revision(profile_id, account, None)
            .map(|_| ())
    }

    pub fn load_account_with_revision(
        &self,
        profile_id: &str,
    ) -> Result<Option<(Value, u64)>, StorageError> {
        let connection = self.connection()?;
        let account = connection
            .query_row(
                "SELECT account_json FROM accounts WHERE id = ?1",
                params![profile_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(account_json) = account else {
            return Ok(None);
        };
        let revision = connection
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        let revision = u64::try_from(revision)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, revision))?;
        Ok(Some((serde_json::from_str(&account_json)?, revision)))
    }

    pub fn save_account_with_revision(
        &self,
        profile_id: &str,
        account: &Value,
        expected_revision: Option<u64>,
    ) -> Result<u64, StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let account_json = serde_json::to_string(account)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let actual = transaction
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        let actual = u64::try_from(actual)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, actual))?;
        if let Some(expected) = expected_revision {
            if expected != actual {
                return Err(StorageError::RevisionConflict { expected, actual });
            }
        }
        let next_revision = actual.checked_add(1).ok_or_else(|| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                "account revision overflow",
            )))
        })?;
        let sql_revision = i64::try_from(next_revision).map_err(|_| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                "account revision exceeds SQLite integer range",
            )))
        })?;
        transaction.execute(
            "INSERT INTO accounts(id, account_json, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               account_json = excluded.account_json,
               updated_utc = excluded.updated_utc",
            params![profile_id, account_json, timestamp()],
        )?;
        transaction.execute(
            "INSERT INTO account_revisions(profile_id, revision, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id) DO UPDATE SET
               revision = excluded.revision,
               updated_utc = excluded.updated_utc",
            params![profile_id, sql_revision, timestamp()],
        )?;
        transaction.commit()?;
        Ok(next_revision)
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
        transaction.execute(
            "DELETE FROM account_revisions WHERE profile_id = ?1",
            params![profile_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, StorageError> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(connection)
    }
}

impl AccountRepository for ProfileStore {
    fn load(&self, profile_id: &ProfileId) -> Result<Option<AccountState>, RepositoryError> {
        self.load_account(profile_id.as_str())
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| RepositoryError::Storage(error.to_string()))
            })
            .transpose()
    }

    fn create(&self, account: &AccountState) -> Result<(), RepositoryError> {
        let profile = account.profile.as_ref().ok_or_else(|| {
            RepositoryError::Storage("account profile is required for creation".to_owned())
        })?;
        let value = serde_json::to_value(account)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        self.save_account(profile.id.as_str(), &value)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    fn transact<F, T>(&self, profile_id: &ProfileId, operation: F) -> Result<T, RepositoryError>
    where
        F: FnOnce(&mut AccountState) -> Result<T, blueoath_domain::DomainError>,
    {
        let mut account = self
            .load_account(profile_id.as_str())
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| RepositoryError::Storage(error.to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| AccountState {
                profile: None,
                resources: Default::default(),
            });
        let result = operation(&mut account)?;
        let value = serde_json::to_value(&account)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        self.save_account(profile_id.as_str(), &value)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(result)
    }
}

const MIGRATIONS: &[&str] = &[
    include_str!("../../../migrations/0001_schema_meta.sql"),
    include_str!("../../../migrations/0002_profiles_accounts.sql"),
    include_str!("../../../migrations/0003_account_revisions.sql"),
    include_str!("../../../migrations/0004_core_account.sql"),
    include_str!("../../../migrations/0005_core_indexes.sql"),
];

fn run_migrations(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(MIGRATIONS[0])?;
    let current: usize =
        connection.query_row("SELECT version FROM schema_meta WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })? as usize;
    if current > MIGRATIONS.len() {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = index + 1;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(migration)?;
        transaction.execute(
            "UPDATE schema_meta SET version = ?1, applied_at = ?2 WHERE id = 1",
            params![version as i64, timestamp()],
        )?;
        transaction.commit()?;
    }
    Ok(())
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
