use crate::catalog::BattleDropQuantities;
use rusqlite::{types::ValueRef, Connection, OpenFlags, Row};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const SCHEMA_VERSION: &str = "2";

#[derive(Clone)]
struct CachedRows(Vec<(i32, Value)>);

static CONFIG_ROWS_CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), CachedRows>>> = OnceLock::new();

pub(super) struct ServerShopCostRow {
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) amount: i64,
}

pub(super) struct ServerShopGoodRow {
    pub(super) shop_id: i32,
    pub(super) good_id: i32,
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) num: i32,
    pub(super) costs: Vec<ServerShopCostRow>,
}

pub(super) struct ServerMailRow {
    pub(super) mid: u64,
    pub(super) goods_type: i32,
    pub(super) config_id: i32,
    pub(super) num: i32,
    pub(super) subject: String,
    pub(super) content: String,
}

pub(super) fn catalog_db_path(config_dir: &Path) -> PathBuf {
    let catalog_root = if config_dir
        .file_name()
        .is_some_and(|name| name == "server-config")
    {
        config_dir.parent().unwrap_or(config_dir)
    } else {
        config_dir
    };
    if catalog_root
        .file_name()
        .is_some_and(|name| name == "catalog")
    {
        return catalog_root
            .parent()
            .unwrap_or(catalog_root)
            .join("server_config.db");
    }
    catalog_root.join("server_config.db")
}

fn open_readonly(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("open catalog database {}: {error}", path.display()))
}

fn quote_identifier(value: &str) -> Result<String, String> {
    if value.is_empty()
        || !value.chars().enumerate().all(|(index, character)| {
            (index == 0 && (character == '_' || character.is_ascii_alphabetic()))
                || (index > 0 && (character == '_' || character.is_ascii_alphanumeric()))
        })
    {
        return Err(format!("invalid catalog SQLite identifier `{value}`"));
    }
    Ok(format!("\"{value}\""))
}

fn meta_value(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT value FROM catalog_meta WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("read catalog metadata `{key}`: {error}"))
}

pub(super) fn validate_catalog_db(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "catalog database does not exist: {}",
            path.display()
        ));
    }
    let connection = open_readonly(path)?;
    let version = meta_value(&connection, "schema_version")?
        .ok_or_else(|| "catalog database has no schema_version".to_string())?;
    if version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported catalog database schema version `{version}`, expected `{SCHEMA_VERSION}`"
        ));
    }
    let config_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM catalog_configs", [], |row| row.get(0))
        .map_err(|error| format!("read catalog config count: {error}"))?;
    if config_count <= 0 {
        return Err("catalog database contains no config tables".to_string());
    }
    connection
        .query_row("SELECT COUNT(*) FROM catalog_columns", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| format!("read catalog column metadata: {error}"))?;
    Ok(())
}

fn decode_column(row: &Row<'_>, index: usize, kind: &str) -> Result<Option<Value>, String> {
    let value = row
        .get_ref(index)
        .map_err(|error| format!("read catalog column {index}: {error}"))?;
    match (kind, value) {
        (_, ValueRef::Null) => Ok(None),
        ("int", ValueRef::Integer(value)) => Ok(Some(Value::Number(Number::from(value)))),
        ("real", ValueRef::Real(value)) => Number::from_f64(value)
            .map(Value::Number)
            .map(Some)
            .ok_or_else(|| format!("invalid floating point value in catalog column {index}")),
        ("bool", ValueRef::Integer(value)) => Ok(Some(Value::Bool(value != 0))),
        ("text", ValueRef::Text(value)) => String::from_utf8(value.to_vec())
            .map(Value::String)
            .map(Some)
            .map_err(|error| format!("invalid UTF-8 in catalog text column {index}: {error}")),
        ("json", ValueRef::Text(value)) => {
            let text = String::from_utf8(value.to_vec()).map_err(|error| {
                format!("invalid UTF-8 in catalog JSON column {index}: {error}")
            })?;
            serde_json::from_str(&text)
                .map(Some)
                .map_err(|error| format!("invalid JSON in catalog column {index}: {error}"))
        }
        (expected, actual) => Err(format!(
            "catalog column {index} has kind `{expected}` but SQLite value is {actual:?}"
        )),
    }
}

fn load_config_rows_uncached(
    connection: &Connection,
    config_name: &str,
) -> Result<Vec<(i32, Value)>, String> {
    let (table_name, row_count): (String, i64) = connection
        .query_row(
            "SELECT table_name, row_count FROM catalog_configs WHERE config_name = ?1",
            [config_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| format!("read catalog config `{config_name}`: {error}"))?;
    let table = quote_identifier(&table_name)?;

    let mut column_statement = connection
        .prepare(
            "SELECT field_name, column_name, value_kind FROM catalog_columns \
             WHERE config_name = ?1 ORDER BY ordinal",
        )
        .map_err(|error| format!("prepare catalog column metadata: {error}"))?;
    let columns: Vec<(String, String, String)> = column_statement
        .query_map([config_name], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| format!("read catalog column metadata: {error}"))?
        .collect::<Result<_, _>>()
        .map_err(|error| format!("decode catalog column metadata: {error}"))?;
    let quoted_columns = columns
        .iter()
        .map(|(_, column, _)| quote_identifier(column))
        .collect::<Result<Vec<_>, _>>()?;
    let select_columns = if quoted_columns.is_empty() {
        "id".to_string()
    } else {
        format!("id, {}", quoted_columns.join(", "))
    };
    let sql = format!("SELECT {select_columns} FROM {table} ORDER BY id");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("prepare catalog table `{table_name}`: {error}"))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("query catalog table `{table_name}`: {error}"))?;
    let mut result = Vec::with_capacity(row_count.max(0) as usize);
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("iterate catalog table `{table_name}`: {error}"))?
    {
        let row_id: i32 = row
            .get(0)
            .map_err(|error| format!("read catalog row id from `{table_name}`: {error}"))?;
        let mut value = Map::new();
        for (index, (field_name, column_name, kind)) in columns.iter().enumerate() {
            if let Some(decoded) = decode_column(row, index + 1, kind)? {
                let key = if column_name == "value_id" {
                    "id"
                } else {
                    field_name
                };
                value.insert(key.to_string(), decoded);
            }
        }
        result.push((row_id, Value::Object(value)));
    }
    Ok(result)
}

pub(super) fn load_config_rows(
    path: &Path,
    config_name: &str,
) -> Result<Vec<(i32, Value)>, String> {
    let key = (path.to_path_buf(), config_name.to_string());
    let cache = CONFIG_ROWS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(rows) = guard.get(&key) {
            return Ok(rows.0.clone());
        }
    }
    let connection = open_readonly(path)?;
    let rows = load_config_rows_uncached(&connection, config_name)?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, CachedRows(rows.clone()));
    }
    Ok(rows)
}

pub(super) fn load_drop_quantities(path: &Path) -> Result<BattleDropQuantities, String> {
    let connection = open_readonly(path)?;
    let mut result = BattleDropQuantities::default();
    let mut statement = connection
        .prepare(
            "SELECT copy_id, goods_type, item_id, min_num, max_num \
             FROM battle_drop_quantities ORDER BY copy_id, goods_type, item_id",
        )
        .map_err(|error| format!("prepare battle drop quantities: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<i32>>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })
        .map_err(|error| format!("query battle drop quantities: {error}"))?;
    for row in rows {
        let (copy_id, goods_type, item_id, min_num, max_num) =
            row.map_err(|error| format!("decode battle drop quantities: {error}"))?;
        let key = format!("{goods_type}:{item_id}");
        if let Some(copy_id) = copy_id {
            result
                .copies
                .entry(copy_id)
                .or_default()
                .insert(key, [min_num, max_num]);
        } else {
            result.default_rewards.insert(key, [min_num, max_num]);
        }
    }
    Ok(result)
}

pub(super) fn load_server_shop_goods(path: &Path) -> Result<Vec<ServerShopGoodRow>, String> {
    let connection = open_readonly(path)?;
    let mut statement = connection
        .prepare(
            "SELECT good_id, shop_id, goods_type, item_id, num, source_priority \
             FROM server_shop_goods AS goods \
             WHERE source_priority = ( \
                 SELECT MAX(selected.source_priority) \
                 FROM server_shop_goods AS selected \
                 WHERE selected.good_id = goods.good_id \
             ) ORDER BY shop_id, good_id",
        )
        .map_err(|error| format!("prepare server shop goods: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })
        .map_err(|error| format!("query server shop goods: {error}"))?;
    let mut result = Vec::new();
    for row in rows {
        let (good_id, shop_id, goods_type, item_id, num, source_priority) =
            row.map_err(|error| format!("decode server shop goods: {error}"))?;
        let mut costs_statement = connection
            .prepare(
                "SELECT goods_type, item_id, amount FROM server_shop_good_costs \
                 WHERE good_id = ?1 AND source_priority = ?2 ORDER BY ordinal",
            )
            .map_err(|error| format!("prepare server shop costs: {error}"))?;
        let costs = costs_statement
            .query_map((good_id, source_priority), |cost| {
                Ok(ServerShopCostRow {
                    goods_type: cost.get(0)?,
                    item_id: cost.get(1)?,
                    amount: cost.get(2)?,
                })
            })
            .map_err(|error| format!("query server shop costs: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("decode server shop costs: {error}"))?;
        result.push(ServerShopGoodRow {
            shop_id,
            good_id,
            goods_type,
            item_id,
            num,
            costs,
        });
    }
    Ok(result)
}

pub(super) fn load_server_mail_templates(path: &Path) -> Result<Vec<ServerMailRow>, String> {
    let connection = open_readonly(path)?;
    let mut statement = connection
        .prepare(
            "SELECT mid, goods_type, config_id, num, subject, content \
             FROM server_mails ORDER BY mid",
        )
        .map_err(|error| format!("prepare server mails: {error}"))?;
    let result = statement
        .query_map([], |row| {
            Ok(ServerMailRow {
                mid: row.get::<_, i64>(0)? as u64,
                goods_type: row.get(1)?,
                config_id: row.get(2)?,
                num: row.get(3)?,
                subject: row.get(4)?,
                content: row.get(5)?,
            })
        })
        .map_err(|error| format!("query server mails: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("decode server mails: {error}"))?;
    Ok(result)
}

use rusqlite::OptionalExtension;
