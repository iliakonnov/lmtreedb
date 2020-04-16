use std::fmt::Debug;
use my_error::*;

pub trait FirstVersionMarker {}

/// Part of `Schema` trait. Default implementation provided for `FirstVersionMarker` types
pub trait SchemaUpgrade: Sized {
    /// Type of the previous version of this object.
    ///
    /// Use `NoSchema` if no previous version exists (Self::version() == 0)
    type PrevVersion: Schema;

    /// Converts deserialized PrevVersion to current.
    ///
    /// If no PrevVersion exists, just return error
    fn upgrade(val: Self::PrevVersion) -> Result<Self, Error>;
}

impl<T> SchemaUpgrade for T where T: FirstVersionMarker {
    type PrevVersion = NoSchema;
    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("This is a first version, so this method should not be called"))
    }
}

pub trait LastVersionMarker {}

/// Part of `Schema` trait. Default implementation provided for `LastVersionMarker` types
pub trait SchemaDowngrade: Sized {
    // Same as `PrevVersion`, but for next version
    type NextVersion: Schema;

    /// Same as `upgrade`, but converts from NextVersion to the current
    fn downgrade(val: Self::NextVersion) -> Result<Self, Error>;
}

impl<T> SchemaDowngrade for T where T: LastVersionMarker {
    type NextVersion = NoSchema;
    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("This is a last version, so this method should not be called"))
    }
}

pub trait SchemaSerdeMarker: serde::Serialize + for<'de> serde::Deserialize<'de> {}

/// Part of `Schema` trait. Default implementation provided for `SchemaSerdeMarker` types
pub trait SchemaSerde: Sized {
    /// Deserializes object from given raw value
    fn load(val: rmpv::Value) -> Result<Self, Error>;

    /// Serializes object back to the raw data
    fn save(self) -> Result<rmpv::Value, Error>;
}

impl<'de, T> SchemaSerde for T where T: SchemaSerdeMarker {
    fn load(val: rmpv::Value) -> Result<Self, Error> {
        rmpv::ext::from_value(val).epos(pos!())
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        rmpv::ext::to_value(self).epos(pos!())
    }
}

/// Part of `Schema` trait. Default implementation provided for `FirstVersionMarker` types
pub trait SchemaVersion {
    /// What version this type handles. Must be greater than zero. Zero is reserved for `NoSchema`
    // TODO: make const
    fn version() -> u64;
}

impl<T> SchemaVersion for T where T: FirstVersionMarker {
    fn version() -> u64 {
        1
    }
}

/// This macro helps implementing schema.
/// For example `def_schema!(MyData = 1; serde, last)` means:
/// - `MyData` implements `SchemaVersion` with version = 1
/// - Version is 1, so it also implements FirstVersionMarker
/// - Also it is marked as `serde`, so `SchemaSerdeMarker` is added
/// - And `last` means that #1 is the last version, so `LastVersionMarker` is implemented too
#[macro_export]
macro_rules! def_schema {
    // Deny zero. This check can be bypassed btw
    ($t:ty = 0; $($args:tt)*) => {
        compile_error!("Version '0' is not allowed");
    };
    // Implement FirstVersionMarker
    ($t:ty = 1; $($args:tt),* $(,)?) => {
        impl $crate::schema::FirstVersionMarker for $t {}
        $(
            $crate::def_schema!(@impl [$t] $args);
        )*
    };
    // Otherwise implement SchemaVersion
    ($t:ty = $ver:expr; $($args:tt),* $(,)?) => {
        impl $crate::SchemaVersion for $t {
            fn version() -> u64 {
                $ver
            }
        }
        $(
            $crate::def_schema!(@impl [$t] $args);
        )*
    };
    (@impl [$t:ty] last) => {
        impl $crate::LastVersionMarker for $t {}
    };
    (@impl [$t:ty] serde) => {
        impl $crate::SchemaSerdeMarker for $t {}
    };
    (@impl [$t:ty] $($arg:tt)*) => {
        compile_error!(concat!(
            "Unknown arg while expanding def_schema:",
            stringify!($($arg)*)
        ));
    };
}


/// Used to specify how object can be serialized and deserialized to be stored in the database
pub trait Schema: Debug + SchemaUpgrade + SchemaDowngrade + SchemaSerde + SchemaVersion {
}

impl<T> Schema for T where T: Debug + SchemaUpgrade + SchemaDowngrade + SchemaSerde + SchemaVersion {}

/// Use this type for non-existing version in `Schema::PrevVersion` and `Schema::NextVersion`
pub type NoSchema = !;

impl SchemaVersion for NoSchema {
    fn version() -> u64 {
        0
    }
}

impl SchemaUpgrade for NoSchema {
    type PrevVersion = NoSchema;
    fn upgrade(_: Self::PrevVersion) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }
}

impl SchemaDowngrade for NoSchema {
    type NextVersion = NoSchema;
    fn downgrade(_: Self::NextVersion) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }
}

impl SchemaSerde for NoSchema {
    fn load(_: rmpv::Value) -> Result<Self, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        Err(err!("Version too low, it should not ever exists"))
    }
}

/// Implementing this trait will implement `FirstVersionMarker` and `LastVersionMarker`,
/// so if type implements `Serialize+Deserialize` it will implement `Schema` too
pub trait SingleVersionMarker
    where Self: Debug + Sized
{
}

impl<T: SingleVersionMarker> FirstVersionMarker for T {}
impl<T: SingleVersionMarker> LastVersionMarker for T {}

macro_rules! simple_type {
    { $($t:ty),* } => {
        $(impl SingleVersionMarker for $t {})*
    };
}

simple_type! {
    u8, i8,
    u16, i16,
    u32, i32,
    u64, i64,
    f32, f64,
    String, Vec<u8>,
    bool
}

impl FirstVersionMarker for () {}
impl LastVersionMarker for () {}
impl SchemaSerde for () {
    fn load(val: rmpv::Value) -> Result<Self, Error> {
        if val.is_nil() {
            Ok(())
        } else {
            Err(err!("Invalid rmpv value. Expected nil, found {:?}", val))
        }
    }

    fn save(self) -> Result<rmpv::Value, Error> {
        Ok(rmpv::Value::Nil)
    }
}
