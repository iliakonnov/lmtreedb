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
    const VERSION: u64;
}

impl<T> SchemaVersion for T where T: FirstVersionMarker {
    const VERSION: u64 = 1;
}

/// Reexport static_Assertions for def_schema macro
pub use static_assertions;

/// This macro helps implementing schema.
/// For example `def_schema!(MyData = 1; serde)` means:
/// - `MyData` implements `SchemaVersion` with version = 1
/// - Version is 1, so it also implements FirstVersionMarker
/// - Also it is marked as `serde`, so `SchemaSerdeMarker` is added
///
/// You can mark version as last by writing it in square brackets: `def_schema!(LastVer = [5])`
#[macro_export]
macro_rules! def_schema {
    // Deny zero. This check can be bypassed btw
    ($t:ty = 0; $($args:tt)*) => {
        compile_error!("Version '0' is not allowed");
    };
    // `= [1]`: Implement SingleVersionMarker
    ($t:ty = [1]; $($args:tt)*) => {
        impl $crate::schema::SingleVersionMarker for $t {}
        $crate::def_schema!(@expand [$t] $($args)*);
    };
    // `= 1`: Implement FirstVersionMarker
    ($t:ty = 1; $($args:tt)*) => {
        impl $crate::schema::FirstVersionMarker for $t {}
        $crate::def_schema!(@expand [$t] check_next, $($args)*);
    };
    // `= [...];`: Implement LastVersionMarker
    ($t:ty = [$ver:expr]; $($args:tt)*) => {
        impl $crate::schema::LastVersionMarker for $t {}
        impl $crate::schema::SchemaVersion for $t {
            const VERSION: u64 = $ver;
        }
        $crate::def_schema!(@expand [$t] check_prev, $($args)*);
    };
    // "Middle" version: just implement SchemaVersion and add more checks
    ($t:ty = $ver:expr; $($args:tt)*) => {
        impl $crate::schema::SchemaVersion for $t {
            const VERSION: u64 = $ver;
        }
        $crate::def_schema!(@expand [$t] check_prev, check_next, $($args)*);
    };
    // Finish implementation
    (@expand [$t:ty] $($args:tt),* $(,)?) => {
        // Implement Schema for type
        impl $crate::schema::Schema for $t {}
        $(
            $crate::def_schema!(@impl [$t] $args);
        )*
    };
    (@impl [$t:ty] serde) => {
        impl $crate::schema::SchemaSerdeMarker for $t {}
    };
    (@impl [$t:ty] check_next) => {
        // Check that Self::VERSION == NextVersion::VERSION - 1
        $crate::schema::static_assertions::const_assert_eq!(
            <$t as $crate::schema::SchemaVersion>::VERSION,
            <<$t as $crate::schema::SchemaDowngrade>::NextVersion
                 as $crate::schema::SchemaVersion>::VERSION - 1
        );
    };
    (@impl [$t:ty] check_prev) => {
        // Check that Self::VERSION == PrevVersion::VERSION + 1
        $crate::schema::static_assertions::const_assert_eq!(
            <$t as $crate::schema::SchemaVersion>::VERSION,
            <<$t as $crate::schema::SchemaUpgrade>::PrevVersion
                 as $crate::schema::SchemaVersion>::VERSION + 1
        );
    };
    (@impl [$t:ty] $($arg:tt)*) => {
        compile_error!(concat!(
            "Unknown arg while expanding def_schema: ",
            stringify!($($arg)*)
        ));
    };
}


/// Specifies how object can be serialized and deserialized to be stored in the database
///
/// You should not implement this type manually, use def_schema! macro instead.
pub trait Schema: Debug + SchemaUpgrade + SchemaDowngrade + SchemaSerde + SchemaVersion {
    fn version() -> u64 {
        Self::VERSION
    }
}

/// Use this type for non-existing version in `Schema::PrevVersion` and `Schema::NextVersion`
pub type NoSchema = !;

impl Schema for NoSchema {}

impl SchemaVersion for NoSchema {
    const VERSION: u64 = 0;
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
        $(
            def_schema!($t = [1]; serde);
        )*
    };
}

simple_type! {
    u8, i8,
    u16, i16,
    u32, i32,
    u64, i64,
    f32, f64,
    String, Vec<u8>,
    bool, ()
}
