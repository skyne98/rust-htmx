use anyhow::Result;
use bincode::{
    config::{BigEndian, WithOtherEndian},
    DefaultOptions, Options,
};
use serde::{de::DeserializeOwned, Serialize};
use sled::Db as Sled;

pub struct Db {
    handle: Sled,
    encoder: WithOtherEndian<DefaultOptions, BigEndian>,
}
impl Db {
    pub fn new() -> Result<Self> {
        let handle = sled::open("db")?;
        let encoder = bincode::options().with_big_endian();
        Ok(Self { handle, encoder })
    }
    pub fn new_with_path(path: &str) -> Result<Self> {
        let handle = sled::open(path)?;
        let encoder = bincode::options().with_big_endian();
        Ok(Self { handle, encoder })
    }

    // CRUD
    pub fn next_id(&self) -> Result<u64> {
        let id = self.handle.generate_id()?;
        Ok(id)
    }
    pub fn insert<T: Serialize, K: AsRef<str>>(&self, key: K, value: &T) -> Result<()> {
        let key = key.as_ref();
        let value = self.encoder.serialize(value)?;
        self.handle.insert(key, value)?;
        Ok(())
    }
    pub fn get<T: DeserializeOwned, K: AsRef<str>>(&self, key: K) -> Result<Option<T>> {
        let key = key.as_ref();
        let value = self.handle.get(key)?;
        let value = match value {
            Some(value) => value,
            None => return Ok(None),
        };
        let value = self.encoder.deserialize(&value)?;
        Ok(Some(value))
    }
    pub fn remove<K: AsRef<str>>(&self, key: K) -> Result<()> {
        let key = key.as_ref();
        self.handle.remove(key)?;
        Ok(())
    }

    // Iterators
    pub fn iter<'a, T: DeserializeOwned + 'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = Result<(String, T)>> + 'a> {
        let iter = self.handle.iter().map(move |item| {
            let (key, value) = item?;
            let key = String::from_utf8(key.to_vec())?;
            let value = self.encoder.deserialize(&value)?;
            Ok((key, value))
        });
        Ok(iter)
    }
    pub fn iter_prefix<'a, T: DeserializeOwned + 'a>(
        &'a self,
        prefix: &str,
    ) -> Result<impl Iterator<Item = Result<(String, T)>> + 'a> {
        let iter = self.handle.scan_prefix(prefix).map(move |item| {
            let (key, value) = item?;
            let key = String::from_utf8(key.to_vec())?;
            let value = self.encoder.deserialize(&value)?;
            Ok((key, value))
        });
        Ok(iter)
    }
}

// Required Debug implementation for `Db`
impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").finish()
    }
}

// Tests
#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Test {
        id: u64,
        name: String,
    }

    fn setup() -> Result<(String, Db)> {
        let tick = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = format!("test_db_{}", tick);
        let db = Db::new_with_path(&path)?;
        Ok((path, db))
    }
    fn teardown((path, db): (String, Db)) -> Result<()> {
        // kill the db
        drop(db);
        // remove the directory
        std::fs::remove_dir_all(path)?;
        Ok(())
    }

    #[test]
    fn test_new() -> Result<()> {
        let db = setup()?;
        teardown(db)?;
        Ok(())
    }

    #[test]
    fn test_next_id() -> Result<()> {
        let (path, db) = setup()?;
        let id = db.next_id()?;
        assert_eq!(id, 0);
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_insert_and_get() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        let test = db.get::<Test, _>("test")?;
        assert_eq!(test.unwrap().name, "test");
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_remove() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        db.remove("test")?;
        let test = db.get::<Test, _>("test")?;
        assert!(test.is_none());
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_insert_as_update() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        let test = Test {
            id: 0,
            name: "test2".to_string(),
        };
        db.insert("test", &test)?;
        let test = db.get::<Test, _>("test")?;
        assert_eq!(test.unwrap().name, "test2");
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_iter() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        let test = Test {
            id: 1,
            name: "test2".to_string(),
        };
        db.insert("test2", &test)?;
        {
            let mut iter = db.iter::<Test>()?;
            let (key, value) = iter.next().unwrap()?;
            assert_eq!(key, "test");
            assert_eq!(value.name, "test");
            let (key, value) = iter.next().unwrap()?;
            assert_eq!(key, "test2");
            assert_eq!(value.name, "test2");
        }
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_iter_prefix() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        let test = Test {
            id: 1,
            name: "test2".to_string(),
        };
        db.insert("test2", &test)?;
        {
            let mut iter = db.iter_prefix::<Test>("test")?;
            let (key, value) = iter.next().unwrap()?;
            assert_eq!(key, "test");
            assert_eq!(value.name, "test");
        }
        teardown((path, db))?;
        Ok(())
    }

    #[test]
    fn test_iter_prefix_excludes() -> Result<()> {
        let (path, db) = setup()?;
        let test = Test {
            id: 0,
            name: "test".to_string(),
        };
        db.insert("test", &test)?;
        let test = Test {
            id: 1,
            name: "test2".to_string(),
        };
        db.insert("test2", &test)?;
        {
            let mut iter = db.iter_prefix::<Test>("test2")?;
            let (key, value) = iter.next().unwrap()?;
            assert_eq!(key, "test2");
            assert_eq!(value.name, "test2");

            let next = iter.next();
            assert!(next.is_none());
        }
        teardown((path, db))?;
        Ok(())
    }
}
