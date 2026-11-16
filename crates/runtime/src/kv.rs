//! Persistent string key/value storage (replaces AsyncStorage).
//!
//! Pure Rust on `redb`; no host module. Values are not encrypted, so keep
//! secrets in [`crate::secure_store`]. Unlike AsyncStorage, calls are
//! synchronous: redb reads are cheap and writes are small.
//!
//! Open a separate file from other redb databases (redb locks its file),
//! e.g. `<app data dir>/kv.redb`.

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;
use std::path::Path;

const TABLE: TableDefinition<&str, &str> = TableDefinition::new("kv");

pub type KvResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct KvStore {
    db: Database,
}

impl KvStore {
    pub fn open(path: &Path) -> KvResult<Self> {
        let db = Database::create(path)?;
        // Create the table so reads on a fresh file succeed.
        let write = db.begin_write()?;
        write.open_table(TABLE)?;
        write.commit()?;
        Ok(Self { db })
    }

    pub fn get(&self, key: &str) -> KvResult<Option<String>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(TABLE)?;
        Ok(table.get(key)?.map(|value| value.value().to_string()))
    }

    pub fn set(&self, key: &str, value: &str) -> KvResult<()> {
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(TABLE)?;
            table.insert(key, value)?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn remove(&self, key: &str) -> KvResult<()> {
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(TABLE)?;
            table.remove(key)?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn keys(&self) -> KvResult<Vec<String>> {
        let read = self.db.begin_read()?;
        let table = read.open_table(TABLE)?;
        let mut keys = Vec::new();
        for item in table.iter()? {
            let (key, _) = item?;
            keys.push(key.value().to_string());
        }
        Ok(keys)
    }

    pub fn clear(&self) -> KvResult<()> {
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(TABLE)?;
            table.retain(|_, _| false)?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> KvResult<Option<T>> {
        match self.get(key)? {
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> KvResult<()> {
        self.set(key, &serde_json::to_string(value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (KvStore, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("cheetah-kv-{name}-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        (KvStore::open(&path).expect("open kv"), path)
    }

    #[test]
    fn set_get_remove_and_persist() {
        let (store, path) = temp_store("roundtrip");
        assert_eq!(store.get("prefs").unwrap(), None);
        store.set("prefs", "vegan").unwrap();
        store.set("city", "SF").unwrap();
        assert_eq!(store.get("prefs").unwrap().as_deref(), Some("vegan"));
        store.remove("city").unwrap();
        assert_eq!(store.keys().unwrap(), vec!["prefs".to_string()]);
        drop(store);

        let reopened = KvStore::open(&path).unwrap();
        assert_eq!(reopened.get("prefs").unwrap().as_deref(), Some("vegan"));
        reopened.clear().unwrap();
        assert!(reopened.keys().unwrap().is_empty());
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_values_roundtrip() {
        let (store, path) = temp_store("json");
        store.set_json("foods", &vec!["sushi", "pizza"]).unwrap();
        let foods: Vec<String> = store.get_json("foods").unwrap().unwrap();
        assert_eq!(foods, vec!["sushi", "pizza"]);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
