#![feature(never_type)]
#![feature(core_intrinsics)]

#[macro_use]
extern crate slog_scope;

use std::cmp::Ordering;

use lmdb::Transaction;

use schema::*;
use wrappers::VersionWrapper;
use wrappers::{DataWrapper, DataWrapperV1};

use path::{Path, PathPart, Root};
use my_error::*;

pub mod path;
pub mod schema;
pub mod wrappers;

#[derive(Debug)]
pub struct Storage {
    db: lmdb::Database,
    env: lmdb::Environment,
}

/// Deserializes val to required type
///
/// This function does all required upgrades or downgrades to convert given version to the required.
///
/// * version argument is the version of saved data. Not the returned version.
fn load<T: Schema>(version: u64, val: rmpv::Value) -> Result<T, Error> {
    match version.cmp(&T::version()) {
        Ordering::Equal => {
            // Just load it
            let res = T::load(val).epos(pos!(quiet std::intrinsics::type_name::<T>()))?;
            Ok(res)
        }
        Ordering::Less => {
            // We found older version, but asked for newer

            // Error if T::PrevVersion >= T::version(). It looks like invalid schema.
            // Also error if there is no place to downgrade (version == 0).
            // Second check is unreachable, because version <=, but just to be sure
            if T::PrevVersion::version() >= T::version() || version == 0 {
                return Err(err!(
                    "Invalid PrevVersion: {} ({}); version: {} ({})",
                    T::PrevVersion::version(), std::intrinsics::type_name::<T::PrevVersion>(),
                    version, std::intrinsics::type_name::<T>()
                ));
            }

            // Load version-1
            let down = load::<T::PrevVersion>(version, val)
                .epos(pos!(quiet("upgrade", std::intrinsics::type_name::<T>())))?;
            // And upgrade it
            let res = T::upgrade(down).epos(pos!())?;
            Ok(res)
        }
        Ordering::Greater => {
            // We found newer version but asked for older

            // Same errors as in previous case
            if T::version() >= T::NextVersion::version() || version == std::u64::MAX {
                return Err(err!(
                    "Invalid NextVersion: {}; version: {}",
                    T::NextVersion::version(),
                    version
                ));
            }

            // Load version + 1
            let up = load::<T::NextVersion>(version, val)
                .epos(pos!(quiet("downgrade", std::intrinsics::type_name::<T>())))?;
            // And downgrade it
            let res = T::downgrade(up).epos(pos!())?;
            Ok(res)
        }
    }
}

/// Implementations of all read-only actions based on lmdb::Transaction
trait RoTransactionExt: lmdb::Transaction {
    /// Loads DataWrapper for specified path if exists.
    /// DataWrapper contains all *info* about specified object (and serialized data)
    fn info<T: DataWrapper>(&self, db: lmdb::Database, path: Path) -> Result<Option<T>, Error> {
        let key = path.to_string();

        let res = lmdb::Transaction::get(self, db, &key);
        if let Err(lmdb::Error::NotFound) = res {
            return Ok(None);
        }

        let mut data = res?;
        let parsed = rmpv::decode::read_value(&mut data).epos(pos!())?;

        let loaded = load::<VersionWrapper<T>>(1, parsed).epos(pos!(path))?;
        Ok(Some(loaded.data))
    }

    /// Deserializes and returns object from database if exists.
    fn get<T: Schema>(&self, db: lmdb::Database, path: Path) -> Result<Option<T>, Error> {
        let data: Option<DataWrapperV1> = RoTransactionExt::info(self, db, path).epos(pos!())?;

        let data = match data {
            None => return Ok(None),
            Some(val) => val,
        };

        let version = data.version;
        let data = data.data;
        let value = load(version, data).epos(pos!(T::version(), version))?;
        Ok(Some(value))
    }
}

impl<T> RoTransactionExt for T where T: lmdb::Transaction {}

/// Implementation of all write-actions based on provided put() and del()
trait RwTransactionExt {
    /// Same as put_unsafe, but also checks for path correctness
    /// and handles all stuff about children and parents
    fn put<T: Schema>(&mut self, db: lmdb::Database, path: Path, data: T) -> Result<(), Error>;

    /// Just puts data into database. No version or parents, only given data.
    fn put_unsafe<T: Schema>(
        &mut self,
        db: lmdb::Database,
        path: Path,
        data: T,
    ) -> Result<(), Error>;

    fn del(&mut self, db: lmdb::Database, path: Path) -> Result<(), Error>;

    /// Helper method that wraps data in DataWrapper first.
    fn put_unsafe_wrapped<T: Schema>(
        &mut self,
        db: lmdb::Database,
        path: Path,
        data: T,
    ) -> Result<(), Error> {
        let data = DataWrapperV1 {
            children: Default::default(),
            version: T::version(),
            data: data.save()?,
        };
        self.put_unsafe_version(db, path, data).epos(pos!())?;
        Ok(())
    }

    /// Wraps data in VersionWrapper that stores version of inner data.
    fn put_unsafe_version<T: DataWrapper>(
        &mut self,
        db: lmdb::Database,
        path: Path,
        data: T,
    ) -> Result<(), Error> {
        let data = VersionWrapper { data };
        self.put_unsafe(db, path, data).epos(pos!())?;
        Ok(())
    }
}

impl<'env> RwTransactionExt for lmdb::RwTransaction<'env> {
    fn put<T: Schema>(&mut self, db: lmdb::Database, path: Path, data: T) -> Result<(), Error> {
        // First check is this path already used
        let existing: Option<DataWrapperV1> =
            RoTransactionExt::info(self, db, path.clone()).epos(pos!())?;
        match existing {
            None => {
                // It is new key, so tell parent abount new child first.
                let (parent_path, name) = path.pop();
                let name = name.err(pos!())?;
                let parent: Option<DataWrapperV1> =
                    RoTransactionExt::info(self, db, parent_path.clone()).epos(pos!())?;
                match parent {
                    None => return Err(err!("No parent '{}' found for '{}'", parent_path, path)),
                    Some(mut par) => {
                        par.children.insert(name);
                        self.put_unsafe_version(db, parent_path, par).epos(pos!())?;
                    }
                }
                // And now we can safely put it
                self.put_unsafe_wrapped(db, path, data).epos(pos!())?;
            }
            Some(ex) => {
                // It exists. So parent already have link to this node and we can just overwrite it.
                if T::version() < ex.version {
                    warn!("overwriting newer version with older");
                }
                self.put_unsafe_version(
                    db,
                    path,
                    DataWrapperV1 {
                        children: ex.children,
                        version: T::version(),
                        data: data.save().epos(pos!())?,
                    },
                )
                .epos(pos!())?;
            }
        }

        Ok(())
    }

    fn put_unsafe<T: Schema>(
        &mut self,
        db: lmdb::Database,
        path: Path,
        data: T,
    ) -> Result<(), Error> {
        let data = data.save()?;
        let mut vec = Vec::new();
        rmpv::encode::write_value(&mut vec, &data).epos(pos!())?;
        self.put(db, &path.to_string(), &vec, lmdb::WriteFlags::NO_DUP_DATA)?;
        Ok(())
    }

    /// Removes specified node and removes it from parent.
    fn del(&mut self, db: lmdb::Database, path: Path) -> Result<(), Error> {
        // First check that there is no any children
        let info: DataWrapperV1 = RoTransactionExt::info(self, db, path.clone())
            .epos(pos!())?
            .err(pos!())?;
        if !info.children.is_empty() {
            return Err(err!("Cannot del file with children"));
        }

        // Then remove this node from it's parent.
        let (parent_path, name) = path.pop();
        let name = name.err(pos!())?;
        let mut parent: DataWrapperV1 = RoTransactionExt::info(self, db, parent_path.clone())
            .epos(pos!())?
            .err(pos!())?;
        let res = parent.children.remove(&name);
        if !res {
            // TODO: warning: Parent does not contain this child, but should
        }

        // Now put it.
        self.put_unsafe_version(db, parent_path, parent)
            .epos(pos!())?;
        self.del(db, &path.to_string(), None).epos(pos!())?;
        Ok(())
    }
}

impl Storage {
    /// Creates or loads database at the specified location.
    pub fn connect(path: &std::path::Path) -> Result<Self, Error> {
        let env = lmdb::Environment::new().open(&path).epos(pos!())?;
        let db = env.create_db(None, Default::default()).epos(pos!())?;
        let mut res = Self { db, env };
        res.init_root()?;
        Ok(res)
    }

    /// Unsafely puts the root node if it does not exists  
    fn init_root(&mut self) -> Result<(), Error> {
        let mut rw = self.env.begin_rw_txn()?;
        let existing: Option<()> =
            RoTransactionExt::get(&rw, self.db, Root::default().path()).epos(pos!())?;
        if existing.is_none() {
            RwTransactionExt::put_unsafe_wrapped(&mut rw, self.db, Root::default().path(), ())
                .epos(pos!())?;
            rw.commit()?;
        }
        Ok(())
    }

    /// Returns information about specified node if exists.
    pub fn children<T: DataWrapper>(&self, path: Path) -> Result<Option<T>, Error> {
        let ro = self.env.begin_ro_txn().epos(pos!())?;
        let res = RoTransactionExt::info(&ro, self.db, path).epos(pos!())?;
        Ok(res)
    }

    /// Returns object at the specified path and deserializes it to the requires type.
    /// Returns error if deserialization failed
    pub fn get<T: Schema>(&self, path: Path) -> Result<Option<T>, Error> {
        let ro = self.env.begin_ro_txn().epos(pos!())?;
        let res = RoTransactionExt::get(&ro, self.db, path).epos(pos!())?;
        Ok(res)
    }

    /// Removes the specified node. Should not contain any children before removing.
    pub fn del(&self, path: Path) -> Result<(), Error> {
        let mut rw = self.env.begin_rw_txn().epos(pos!())?;
        RwTransactionExt::del(&mut rw, self.db, path).epos(pos!())?;
        rw.commit().epos(pos!())?;
        Ok(())
    }

    /// Put the data at the specified path. Parent must exists before adding new entry.
    pub fn put<T: Schema>(&self, path: Path, val: T) -> Result<(), Error> {
        let mut rw = self.env.begin_rw_txn().epos(pos!())?;
        RwTransactionExt::put(&mut rw, self.db, path, val).epos(pos!())?;
        rw.commit().epos(pos!())?;
        Ok(())
    }

    /// Closes and consumes the database.
    pub fn close(self) -> Result<(), Error> {
        // Do nothing, because self.env closes database on drop
        Ok(())
    }

    pub fn flush(&self) -> Result<(), Error> {
        self.env.sync(true).epos(pos!())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use path::*;

    use super::*;
    use rmpv::Value;

    #[derive(Debug)]
    struct Test1 {
        data: i64,
    }

    def_schema!(Test1 = 1;);

    impl SchemaSerde for Test1 {
        fn load(val: rmpv::Value) -> Result<Self, Error> {
            let data = val.as_i64().err_msg(pos!(), msg!("Unable to load"))?;
            Ok(Self { data })
        }

        fn save(self) -> Result<rmpv::Value, Error> {
            Ok(rmpv::Value::Integer(rmpv::Integer::from(self.data)))
        }
    }

    impl SchemaDowngrade for Test1 {
        type NextVersion = Test2;
        fn downgrade(val: Self::NextVersion) -> Result<Self, Error> {
            Ok(Self {
                data: val.data as i64,
            })
        }
    }

    #[derive(Debug)]
    struct Test2 {
        data: f64,
    }

    def_schema!(Test2 = [2];);

    impl SchemaSerde for Test2 {
        fn load(val: Value) -> Result<Self, Error> {
            let data = val.as_f64().err_msg(pos!(), msg!("Unable to load"))?;
            Ok(Self { data })
        }

        fn save(self) -> Result<Value, Error> {
            Ok(rmpv::Value::F64(self.data))
        }
    }

    impl SchemaUpgrade for Test2 {
        type PrevVersion = Test1;

        fn upgrade(val: Self::PrevVersion) -> Result<Self, Error> {
            Ok(Self {
                data: val.data as f64,
            })
        }
    }

    #[test]
    fn create_db() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).epos(pos!()).unwrap();
        db.close().unwrap();
    }

    #[test]
    fn create_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        {
            let db = Storage::connect(path).epos(pos!()).unwrap();
            db.close().unwrap();
        }
        {
            let db = Storage::connect(path).epos(pos!()).unwrap();
            db.close().unwrap();
        }
    }

    #[test]
    fn create_twice_nodrop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).epos(pos!()).unwrap();
        let db2 = Storage::connect(path).epos(pos!()).unwrap();
        drop((db, db2));
    }

    #[test]
    fn put_get() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let data = Test1 { data: 5 };

        let db = Storage::connect(path).unwrap();
        db.put(get_path(), data).epos(pos!()).unwrap();

        let data: Test1 = db.get(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(data.data, 5);
    }

    #[test]
    fn upgrade() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let data = Test1 { data: 5 };

        let db = Storage::connect(path).unwrap();
        db.put(get_path(), data).epos(pos!()).unwrap();

        let data: Test2 = db.get(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(data.data, 5.0);
    }

    #[test]
    fn downgrade() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let data = Test2 { data: 5.3 };

        let db = Storage::connect(path).epos(pos!()).unwrap();
        db.put(get_path(), data).epos(pos!()).unwrap();

        let data: Test1 = db.get(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(data.data, 5);
    }

    #[test]
    fn overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let db = Storage::connect(path).epos(pos!()).unwrap();
        db.put(get_path(), Test1 { data: 2 }).epos(pos!()).unwrap();
        db.put(get_path(), Test2 { data: 5.3 })
            .epos(pos!())
            .unwrap();

        let data: Test2 = db.get(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(data.data, 5.3);
    }

    #[test]
    fn overwrite_downgrade() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let db = Storage::connect(path).unwrap();
        db.put(get_path(), Test2 { data: 5.3 })
            .epos(pos!())
            .unwrap();
        db.put(get_path(), Test1 { data: 2 }).epos(pos!()).unwrap();

        let data: Test2 = db.get(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(data.data, 2.0);
    }

    #[test]
    fn del() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        db.put(get_path(), Test2 { data: 5.3 })
            .epos(pos!())
            .unwrap();
        let data: Option<Test2> = db.get(get_path()).epos(pos!()).unwrap();
        assert!(data.is_some());

        db.del(get_path()).epos(pos!()).unwrap();
        let res: Option<Test1> = db.get(get_path()).epos(pos!()).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn create_child() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        db.put(get_path(), Test1 { data: 1 }).epos(pos!()).unwrap();
        db.put(get_path() + "hello", Test1 { data: 1 })
            .epos(pos!())
            .unwrap();

        let info: DataWrapperV1 = db.children(get_path()).epos(pos!()).unwrap().unwrap();
        assert!(info.children.contains("hello"));
    }

    #[test]
    fn remove_child() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        db.put(get_path(), Test1 { data: 1 }).epos(pos!()).unwrap();
        db.put(get_path() + "hello", Test1 { data: 1 })
            .epos(pos!())
            .unwrap();
        db.del(get_path() + "hello").epos(pos!()).unwrap();

        let info: DataWrapperV1 = db.children(get_path()).epos(pos!()).unwrap().unwrap();
        assert!(info.children.is_empty());
    }

    #[test]
    fn version_info() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        db.put(get_path(), Test2 { data: 1.0 })
            .epos(pos!())
            .unwrap();

        let info: DataWrapperV1 = db.children(get_path()).epos(pos!()).unwrap().unwrap();
        assert_eq!(info.version, Test2::version());
    }

    #[test]
    fn remove_parent_with_child() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        // Create parent
        db.put(get_path(), Test1 { data: 1 }).epos(pos!()).unwrap();
        // Create child
        db.put(get_path() + "a", Test1 { data: 1 })
            .epos(pos!())
            .unwrap();
        // Remove parent
        let res = db.del(get_path());
        assert!(res.is_err())
    }

    #[test]
    fn create_child_no_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let db = Storage::connect(path).unwrap();

        // Create child
        let res = db.put(get_path() + "a", Test1 { data: 1 });
        assert!(res.is_err())
    }

    fn get_path() -> Path {
        Root::default().path() + "test"
    }
}
