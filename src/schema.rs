use std::fmt::Debug;

use crate::*;

/// Used to specify how object can be serialized and deserialized to be stored in the database
pub trait Schema: Sized + Debug {
    /// Type of the previous version of this object.
    ///
    /// Use `NoSchema` if no previous version exists (Self::version() == 0)
    type PrevVersion: Schema;

    // Same as `PrevVersion`, but for next version
    type NextVersion: Schema;

    /// What version this type handles. Must be greater than zero. Zero is reserved for `NoSchema`
    // TODO: make const
    fn version() -> u64;

    /// Deserializes object from given raw value
    fn load(val: rmpv::Value) -> Result<Self, Error>;

    /// Converts deserialized PrevVersion to current.
    ///
    /// If no PrevVersion exists, just return error
    fn upgrade(val: Self::PrevVersion) -> Result<Self, Error>;

    /// Same as `upgrade`, but converts from NextVersion to the current
    fn downgrade(val: Self::NextVersion) -> Result<Self, Error>;

    /// Serializes object back to the raw data
    fn save(self) -> Result<rmpv::Value, Error>;
}

/// Use this type for non-existing version in `Schema::PrevVersion` and `Schema::NextVersion`
pub type NoSchema = !;

impl Schema for NoSchema {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        0
    }

    fn load(_: rmpv::Value) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }

    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }

    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }
}

impl Schema for () {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        1
    }

    fn load(_: rmpv::Value) -> Result<Self, Error> {
        Ok(())
    }

    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("No prev version exists"))
    }

    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("No next version exists"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        Ok(rmpv::Value::Nil)
    }
}
