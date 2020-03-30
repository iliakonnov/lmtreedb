use std::fmt::Debug;
use std::convert::{TryFrom, TryInto};

use my_error::*;

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

/// Deserialization: Value ->? Through ->? Self
/// Serialization:   Self -> Value
pub trait SimpleTypeMarker
    where Self: Into<rmpv::Value>,
          Self::Through: TryFrom<rmpv::Value>,
          Self::Through: TryInto<Self>,
          Self::Through: Debug,
          Self: Debug + Sized
{
    type Through;
}

macro_rules! simple_type {
    { $($t:ty : $u:ty),* } => {
        $(impl SimpleTypeMarker for $t {
            type Through = $u;
        })*
    };
}

simple_type! {
    u8: u64, i8: i64,
    u16: u64, i16: i64,
    u32: u64, i32: i64,
    u64: Self, i64: Self,
    f32: Self, f64: Self,
    String: Self, Vec<u8>: Self,
    bool: Self
}

impl<T> Schema for T where T: SimpleTypeMarker {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        1
    }

    fn load(val: rmpv::Value) -> Result<Self, Error> {
        // FIXME: Format these errors somehow
        let temp = T::Through::try_from(val)
            .map_err(|_| err!("Value -> Through failed"))
            ?;
        let res = temp.try_into()
            .map_err(|_| err!("Value -> Through failed"))
            ?;
        Ok(res)
    }

    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("No prev version exists"))
    }

    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("No next version exists"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        Ok(self.into())
    }
}

impl Schema for () {
    type PrevVersion = NoSchema;
    type NextVersion = NoSchema;

    fn version() -> u64 {
        1
    }

    fn load(val: rmpv::Value) -> Result<Self, Error> {
        if val.is_nil() {
            Ok(())
        } else {
            Err(err!("Invalid rmpv value. Expected nil, found {:?}", val))
        }
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
