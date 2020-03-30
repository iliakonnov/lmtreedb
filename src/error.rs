use inheritance::{inheritable, Inheritance};
use std::fmt::{Debug, Display, Formatter};

/// Position of occurred error. Best used with the `pos!` macro
pub struct ErrorPosition {
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
    pub context: Option<String>,
}

impl ErrorPosition {
    fn unknown() -> Self {
        Self {
            file: "<unknown>",
            line: 0,
            column: 0,
            context: None,
        }
    }
}

impl Display for ErrorPosition {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        match &self.context {
            None => write!(f, "{}:{}", self.file, self.line)?,
            Some(ctx) => write!(f, "{}:{} ({})", self.file, self.line, ctx)?,
        };
        Ok(())
    }
}

/// Type for all critical errors that should be bubbled up.
pub struct Error {
    message: Option<String>,
    position: ErrorPosition,

    /// Reference to the error, which caused this error to happen.
    previous: Option<Box<Error>>,
}

impl Error {
    /// Creates new using only position
    pub fn pos(pos: ErrorPosition) -> Self {
        Self {
            message: None,
            position: pos,
            previous: None,
        }
    }

    /// Creates new error with both message and position
    pub fn msg(pos: ErrorPosition, msg: String) -> Self {
        Self {
            message: Some(msg),
            position: pos,
            previous: None,
        }
    }

    /// Makes this error to be cause of the next error and returns next back.
    pub fn chain(self, mut next: Error) -> Error {
        next.previous = Some(Box::new(self));
        next
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        let this = match &self.message {
            None => format!("[<{}>]", self.position),
            Some(msg) => format!("[<{}> {}]", self.position, msg),
        };
        if let Some(val) = &self.previous {
            write!(f, "{}\n   -> {}", *val, this)?;
        } else {
            write!(f, "\n{}", this)?;
        }
        Ok(())
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self)?;
        Ok(())
    }
}

// Unable to implement std::error::Error directly for Error because of conflict of
// `impl From<T: std::error::Error> for Error` in this file
// and `impl From<T> for T` in stdlib
// FIXME: Maybe #![feature(specialization)] can help some day
#[derive(Inheritance)]
pub struct ErrWrapper(#[inherits(Display, Debug)] Error);

impl std::error::Error for ErrWrapper {}

impl AsRef<ErrWrapper> for Error {
    fn as_ref(&self) -> &ErrWrapper {
        &ErrWrapper(*self)
    }
}

impl AsRef<Error> for ErrWrapper {
    fn as_ref(&self) -> &Error {
        &self.0
    }
}

// Skip impl Into<Error> for ErrWrapper because of `impl From<T> for Error`

impl Into<ErrWrapper> for Error {
    fn into(self) -> ErrWrapper {
        ErrWrapper(self)
    }
}

/// Just an extension trait to plain Result which provides useful things.
pub trait ChainableResult {
    type Result;

    /// Sets error message and position if it is an Result::Error variant.
    fn emsg<F, G>(self, pos: F, msg: G) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
        G: FnOnce() -> String;

    /// Sets only position if it is an Result::Error variant.
    ///
    /// Very often used as `let unwrapped = res.epos(pos!())?` to bubble up unexpected error.
    fn epos<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition;
}

// Implement `ChainableResult` for all results, where Result::Error can be converted into `Error` type
impl<R, E: Into<Error>> ChainableResult for Result<R, E> {
    type Result = R;

    fn emsg<F, G>(self, pos: F, msg: G) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
        G: FnOnce() -> String,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => Err(err.into().chain(Error::msg(pos(), msg()))),
        }
    }

    fn epos<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => Err(err.into().chain(Error::pos(pos()))),
        }
    }
}

/// Extension trait for plain Result to allow conversation of all Display'able errors into my own Error
pub trait CastableResult {
    type Result;
    fn cast<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition;
}

impl<R, E: Display> CastableResult for Result<R, E> {
    type Result = R;

    fn cast<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
    {
        match self {
            Ok(val) => Ok(val),
            Err(err) => Err(Error::msg(pos(), err.to_string())),
        }
    }
}

/// macro to build ErrorPosition struct easily. Necer build it manually, use this macro!
///
/// You can provide any variables as arguments, so the will be added to the stacktrace (works like `dbg!` macro).
///
/// Also `quiet` prefix can be provided to skip variable name if it too big and useless.
#[macro_export]
macro_rules! pos {
    () => {
        || $crate::error::ErrorPosition {
            file: file!(),
            line: line!(),
            column: column!(),
            context: None,
        }
    };

    // Based on `dbg!` source code
    (quiet $val:expr) => {
        || $crate::error::ErrorPosition {
            file: file!(),
            line: line!(),
            column: column!(),
            context: Some(format!("={:?}", &$val))
        }
    };
    ($val:expr) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        || $crate::error::ErrorPosition {
            file: file!(),
            line: line!(),
            column: column!(),
            context: Some(format!("{} = {:?}", stringify!($val), &$val))
        }
    };
    // Trailing comma
    (quiet $val:expr,) => { pos!(quiet $val) };
    ($val:expr,) => { pos!($val) };

    // Multiple
    ($($val:expr),+ $(,)?) => {
        || $crate::error::ErrorPosition {
            file: file!(),
            line: line!(),
            column: column!(),
            context: Some(format!(
                concat!(
                    $(
                        "{} = {:?}; ",

                        // Void to make this loop over `$val` possible
                        pos!(@void $val)
                    ),*
                )
                $(
                    , stringify!($val), &$val
                )*
            ))
        }
    };

    (@void $t:tt) => {""}
}

/// Like `try!` macro, but with some enhancements and not deprecated
#[macro_export]
macro_rules! etry {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(err) => {
                let e: $crate::error::Error = err.into();
                return Err(e.chain(Error::pos(pos!()())));
            }
        }
    };
}

/// Creates error with specified text and automatically adds current position to it.
///
/// Text is provided by the closure
#[macro_export]
macro_rules! err {
    ($($t:tt)*) => {
        $crate::error::Error::msg(pos!()(), format!($($t)*))
    };
}

/// Creates closure, which just returns given format string. Best used with `err!`
#[macro_export]
macro_rules! msg {
    ($($t:tt)*) => {
        || format!($($t)*)
    };
}

/// This extension trait adds `err` method to allow some types to be converted into Result.
///
/// Very useful to safely unwrap `Option<T>`
pub trait IntoErr {
    type Result;
    fn err<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition;
    fn err_msg<F, G>(self, pos: F, msg: G) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
        G: FnOnce() -> String;
}

impl<T> IntoErr for Option<T> {
    type Result = T;

    fn err<F>(self, pos: F) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
    {
        match self {
            Some(val) => Ok(val),
            None => Err(Error::msg(pos(), "Unwrap of None".to_string())),
        }
    }

    fn err_msg<F, G>(self, pos: F, msg: G) -> Result<Self::Result, Error>
    where
        F: FnOnce() -> ErrorPosition,
        G: FnOnce() -> String,
    {
        match self {
            Some(val) => Ok(val),
            None => Err(Error::msg(pos(), format!("Unwrap of None: {}", msg()))),
        }
    }
}

impl<T: std::error::Error> From<T> for Error {
    fn from(err: T) -> Self {
        Error::msg(ErrorPosition::unknown(), err.to_string())
    }
}
