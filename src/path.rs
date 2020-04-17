use std::fmt;
use std::fmt::Display;

/// All pathes in the database are using this type.
/// Just a simple wrapping around Vec<String> with some handful functions
#[derive(Clone, Debug)]
pub struct Path(pub Vec<String>);

/// Allows to add any Display'able parts to the path. String, &str and others.
impl<T: Display> std::ops::Add<T> for Path {
    type Output = Path;

    fn add(mut self, rhs: T) -> Self::Output {
        self += rhs;
        self
    }
}

impl Path {
    /// Splits this path to the parent path and name.
    /// If it does not contain any parts, None is returned as name.
    pub fn pop(&self) -> (Self, Option<String>) {
        let mut res = self.clone();
        let x = res.0.pop();
        (res, x)
    }
}

/// Same as std::ops::Add, but inplace
impl<T: Display> std::ops::AddAssign<T> for Path {
    fn add_assign(&mut self, rhs: T) {
        let s = rhs.to_string();
        for i in s.split('/') {
            // TODO: Bypass .to_string() call
            self.0.push(i.to_string());
        }
    }
}

/// Converts path display, using slash as separator
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut res: String = self.0.join("/");
        if res.is_empty() {
            res = "/".to_string()
        }
        write!(f, "{}", res)
    }
}

/// Marker trait that specifies possible parent for node
/// ```
/// use lmtreedb::path::*;
///
/// // Our struct
/// struct Child(Path);
/// impl PathPart for Child {
///     fn path(self) -> Path {self.0}
/// }
///
/// // We define that only possible parent for Child can be 'Root'
/// impl ChildTrait<Root> for Child {}
///
/// // See `ExtendableDef` for more info
/// impl ExtendableDef for Child {
///     fn extend(parent: Path) -> Self { Self(parent + "child") }
/// }
///
/// // And now can write this:
/// let par = Root::default();
/// let child: Child = par.child();
/// assert_eq!(child.path().to_string(), "@root/child")
/// ```
pub trait ChildTrait<Parent: PathPart> {}

/// PathParts that implement this trait are adding new parts to path without any parameters.
/// ```
/// use lmtreedb::path::*;
///
/// struct A(Path);
/// impl PathPart for A {
///     fn path(self) -> Path {self.0}
/// }
///
/// impl ExtendableDef for A {
///     fn extend(parent: Path) -> Self {
///         // Just adds "a" part to any parent
///         Self(parent + "a")
///     }
/// }
///
/// let root = Root::default().path();
/// assert_eq!(A::extend(root).path().to_string(), "@root/a");
/// ```
pub trait ExtendableDef: PathPart {
    fn extend(parent: Path) -> Self;
}

/// This trait is implemented for all PathPart. Allows to create child without argument.
/// Child type must implement `ChildTrait<Self>` and ExtenableDef to be created using this trait.
///
/// `ChildTrait<Self>` means that child type can be created from parent type `Self`
///
/// `ExtendableDef` means that child type can be created without additional arguments (default)
pub trait NextChildDef: PathPart {
    fn child<C>(self) -> C
    where
        C: ChildTrait<Self> + ExtendableDef;
}

impl<T: PathPart> NextChildDef for T {
    fn child<C>(self) -> C
    where
        C: ChildTrait<Self> + ExtendableDef,
    {
        C::extend(self.path())
    }
}

/// Same as ExtendableDef, but also allows to specify argument.
/// ```
/// use lmtreedb::path::*;
///
/// struct A(Path);
/// impl PathPart for A {
///     fn path(self) -> Path {self.0}
/// }
///
/// impl ExtendableVal<i32> for A {
///     fn extend(parent: Path, val: i32) -> Self {
///         // Adds val to the path
///         Self(parent + val)
///     }
/// }
///
/// let root = Root::default().path();
/// assert_eq!(A::extend(root, 123).path().to_string(), "@root/123");
/// ```
pub trait ExtendableVal<T>: PathPart {
    fn extend(parent: Path, val: T) -> Self;
}

/// Same as `NextChildDef`, but with argument (see `ExtendableVal`)
pub trait NextChildVal: PathPart {
    fn child_val<C, V>(self, val: V) -> C
    where
        C: ChildTrait<Self> + ExtendableVal<V>;
}

impl<T: PathPart> NextChildVal for T {
    fn child_val<C, V>(self, val: V) -> C
    where
        C: ChildTrait<Self> + ExtendableVal<V>,
    {
        C::extend(self.path(), val)
    }
}

/// All strict path parts should implement this trait
pub trait PathPart: Sized {
    fn path(self) -> Path;
    fn into_string(self) -> String {
        self.path().to_string()
    }
}

/// The root node of database.
pub struct Root(Path);

impl Default for Root {
    fn default() -> Self {
        Self(Path(vec!["@root".to_string()]))
    }
}

impl PathPart for Root {
    fn path(self) -> Path {
        self.0
    }
}

/// Path part that just extends parent with any Display'able type. Not really strict, but useful
pub struct DynPath(Path);

impl<T: Display> ExtendableVal<T> for DynPath {
    fn extend(parent: Path, val: T) -> Self {
        Self(parent + val)
    }
}

impl PathPart for DynPath {
    fn path(self) -> Path {
        self.0
    }
}

/// Path part that wraps provided type. Like DynPath, but much more strict
pub struct Pathify<T: Display> {
    parent: Path,
    pub value: T
}

impl<T: Display> ExtendableVal<T> for Pathify<T> {
    fn extend(parent: Path, value: T) -> Self {
        Self {
            parent,
            value
        }
    }
}

impl<T: Display> PathPart for Pathify<T> {
    fn path(self) -> Path {
        self.parent + self.value
    }
}

/// Macro to make strict pathes a lot easier to use
///
/// # Create new part
/// ```path!(MyPart = "my_part")```
/// Creates new struct named MyPart that extends parent with "my_part". Looks like `.../my_part/...`
///
/// ```path!(def MyPart)```
/// Same as previous, but automatically creates text representation from identifier `.../MyPart/...`
///
/// `path!(pub MyPart = "my_part")` or `path!(pub def MyPart)`
/// Just adds pub modifier to result structs.
///
/// # Combine pathes
/// Always first argument must be expression (usually Root) and in square braces.
/// After it one or more parts can be specified.
/// If part requires an argument, it must be supplied in square braces right after the identifier of part
/// All parts are separated using `/`
///
/// # Specify links
/// This macro also allows to easly specify what parts can be after other.
///
/// See `mod tests` for examples.
#[macro_export]
macro_rules! path {
    // Combine path
    // Simple: path!([root] / T)
    ([$($root:tt)*] / $t:ty) => {
        {
            use $crate::path::NextChildDef;
            $crate::path!(@root $($root)*).child::<$t>()
        }
    };
    // child_val: path!([root] / T[val])
    ([$($root:tt)*] / $t:ty[$val:expr]) => {
        {
            use $crate::path::NextChildVal;
            $crate::path!(@root $($root)*).child_val::<$t, _>($val)
        }
    };
    // All together
    ([$($root:tt)*] / $a:tt / $($t:tt)+) => {
        $crate::path!([ $crate::path!([$($root)*] / $a) ] / $($t)+)
    };
    ([$($root:tt)*] / $a:tt[$val:tt] / $($t:tt)+) => {
        $crate::path!([ $crate::path!([$($root)*] / $a[$val]) ] / $($t)+)
    };
    (@root) => {
        $crate::path::Root::default()
    };
    (@root $root:expr) => {
        $root
    };

    // Create new
    ($vis:vis $id:ident = $name:expr) => {
        $vis struct $id($crate::path::Path);
        $crate::path!(@impl $id $name);
    };
    (def $vis:vis $name:ident) => {
        $crate::path!($vis $name = stringify!($name) );
    };

    (@impl $id:ident $name:expr) => {
        impl $crate::path::ExtendableDef for $id {
            fn extend(parent: $crate::path::Path) -> Self {
                Self(parent + $name)
            }
        }
        impl $crate::path::PathPart for $id {
            fn path(self) -> $crate::path::Path {
                self.0
            }
        }
    };

    // Link
    ($parent:tt -> $($child:tt),*) => {
        $(
            $crate::path!(@link $parent $child);
        )*
    };
    ($(parent:tt),* -> $child:tt) => {
        $(
            $crate::path!(@link $parent $child);
        )*
    };
    ($parent:tt -> $child:tt $(-> $($remaining:tt),+)+) => {
        $crate::path!($parent -> $child);
        $crate::path!($child $(-> $($remaining),+)+);
    };

    (@link $parent:ident $child:ident) => {
        impl $crate::path::ChildTrait<$parent> for $child {}
    };
    (@link * $child:ident) => {
        impl<T> $crate::path::ChildTrait<T> for $child where T: $crate::path::PathPart {}
    };
    (@link $parent:ident *) => {
        impl<T> $crate::path::ChildTrait<$parent> for T where T: $crate::path::PathPart {}
    };
}

#[cfg(test)]
mod test {
    use super::*;

    path!(A = "a");
    path!(B = "b");
    path!(def C);
    path!(Root -> A -> B);
    path!(* -> C -> DynPath -> A);

    #[test]
    fn test_root() {
        let root = Root::default();
        assert_eq!(root.path().to_string(), "@root");
    }

    #[test]
    fn test_b() {
        let root = Root::default();
        let a: A = root.child();
        let b: B = a.child();
        assert_eq!(b.into_string(), "@root/a/b");
    }

    #[test]
    fn test_c() {
        let root = Root::default();
        let a: A = root.child();
        let c: C = a.child();
        assert_eq!(c.into_string(), "@root/a/C");
    }

    #[test]
    fn test_num() {
        let root = Root::default();
        let a: A = root.child();
        let c: C = a.child();
        let n: DynPath = c.child_val(123);
        assert_eq!(n.into_string(), "@root/a/C/123");
    }

    #[test]
    fn test_combine() {
        let root = Root::default();
        let n = path!([root] / A / C / DynPath[123]);
        assert_eq!(n.into_string(), "@root/a/C/123");
    }

    #[test]
    fn test_combine_more() {
        let root = Root::default();
        let n = path!([root] / A / C / DynPath[123] / A / C / DynPath[345]);
        assert_eq!(n.into_string(), "@root/a/C/123/a/C/345");
    }

    #[test]
    fn test_optional_root() {
        let n = path!([] / A);
        assert_eq!(n.into_string(), "@root/a");
    }
}
