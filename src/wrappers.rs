use std::collections::HashSet;
use std::iter::FromIterator;

use crate::*;

use super::load;
use super::{NoSchema, Schema};

/// Stores only DataWrapper data and version
///
/// Internally serializes to two records: DataWrapper's data and DataWrapper's version.
/// Can be desserialized to any DataWrapper version, using upgrades and downgrades if required
#[derive(Debug)]
pub struct VersionWrapper<T: DataWrapper> {
    pub data: T,
}

impl<T: DataWrapper> Schema for VersionWrapper<T> {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        1
    }

    fn load(val: rmpv::Value) -> Result<Self, Error> {
        // Load internal array of two records
        let arr = val.as_array().err(pos!(val))?;
        if arr.len() != 2 {
            return Err(err!("Invalid format"));
        }

        // Split internal data
        let version = arr[0].as_u64().err(pos!())?;
        let data = arr[1].clone();

        // And deserialize saved version (`version`) to the required (`Self::T`)
        let data = load(version, data).epos(pos!())?;
        Ok(Self { data })
    }

    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        // It is required, because no one knows what version of VersionWrapper is stored in the database
        Err(err!("VersionWrapper must have only one version"))
    }

    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("VersionWrapper must have only one version"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        let mut arr = Vec::with_capacity(2);
        arr.push(rmpv::Value::from(T::version()));
        arr.push(self.data.save()?);
        Ok(rmpv::Value::Array(arr))
    }
}

/// All versions of DataWrapper must implement this marker trait to be used in VersionWrapper
pub trait DataWrapper: Schema {}

/// Used by the "filesystem". Stores version of stored data, data and all children.
#[derive(Clone, Debug)]
pub struct DataWrapperV1 {
    pub children: HashSet<String>,
    pub version: u64,
    pub data: rmpv::Value,
}

impl DataWrapper for DataWrapperV1 {}

impl Schema for DataWrapperV1 {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        1
    }

    fn load(val: rmpv::Value) -> Result<Self, Error> {
        let arr = val.as_array().err(pos!(val))?;
        if arr.len() != 3 {
            return Err(err!("Invalid format"));
        }
        let children = arr[0].as_array().err(pos!())?;
        let children: Option<Vec<String>> = children
            .iter()
            .map(|x| x.as_str().map(|y| y.to_string()))
            .collect();
        let children = children.err(pos!())?;

        let version = arr[1].as_u64().err(pos!())?;
        let data = arr[2].clone();
        Ok(Self {
            children: HashSet::from_iter(children),
            version,
            data,
        })
    }

    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("Cannot upgrade, no previous version exists"))
    }

    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("Cannot downgrade, no next version exists"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        let mut arr = Vec::with_capacity(3);
        let children: Vec<rmpv::Value> = self.children.into_iter().map(rmpv::Value::from).collect();
        arr.push(rmpv::Value::from(children));
        arr.push(rmpv::Value::from(self.version));
        arr.push(self.data);
        Ok(rmpv::Value::from(arr))
    }
}
