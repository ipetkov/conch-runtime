use runtime::env::{StringWrapper, VariableEnvironment};
use std::borrow::Borrow;
use std::vec;

lazy_static! {
    static ref IFS: String = { String::from("IFS") };
}

/// Represents the types of fields that may result from evaluating a word.
/// It is important to maintain such distinctions because evaluating parameters
/// such as `$@` and `$*` have different behaviors in different contexts.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Fields<T> {
    /// No fields, distinct from present-but-null fields.
    Zero,
    /// A single field.
    Single(T),
    /// Any number of fields resulting from evaluating the `$@` special parameter.
    At(Vec<T>),
    /// Any number of fields resulting from evaluating the `$*` special parameter.
    Star(Vec<T>),
    /// A non-zero number of fields resulting from splitting, and which do not have
    /// any special meaning.
    Split(Vec<T>),
}

impl<T: StringWrapper> Fields<T> {
    /// Indicates if a set of fields is considered null.
    ///
    /// A set of fields is null if every single string
    /// it holds is the empty string.
    pub fn is_null(&self) -> bool {
        match *self {
            Fields::Zero => true,

            Fields::Single(ref s) => s.as_str().is_empty(),

            Fields::At(ref v)   |
            Fields::Star(ref v) |
            Fields::Split(ref v) => v.iter().all(|s| s.as_str().is_empty()),
        }
    }

    /// Joins all fields using a space.
    ///
    /// Note: `Zero` is treated as a empty-but-present field for simplicity.
    pub fn join(self) -> T {
        match self {
            Fields::Zero => String::new().into(),
            Fields::Single(s) => s,
            Fields::At(v)   |
            Fields::Star(v) |
            Fields::Split(v) => v.iter()
                .map(StringWrapper::as_str)
                .filter_map(|s| if s.is_empty() { None } else { Some(s) })
                .collect::<Vec<&str>>()
                .join(" ")
                .into(),
        }
    }

    /// Joins any field unconditionally with the first character of `$IFS`.
    /// If `$IFS` is unset, fields are joined with a space, or concatenated
    /// if `$IFS` is empty.
    ///
    /// Note: `Zero` is treated as a empty-but-present field for simplicity.
    pub fn join_with_ifs<E: ?Sized>(self, env: &E) -> T
        where E: VariableEnvironment,
              E::VarName: Borrow<String>,
              E::Var: Borrow<String>,
    {
        match self {
            Fields::Zero => String::new().into(),
            Fields::Single(s) => s,
            Fields::At(v)   |
            Fields::Star(v) |
            Fields::Split(v) => {
                let sep = env.var(&IFS)
                    .map(|s| s.borrow().as_str())
                    .map_or(" ", |s| if s.is_empty() { "" } else { &s[0..1] });

                v.iter()
                    .map(StringWrapper::as_str)
                    .collect::<Vec<_>>()
                    .join(sep)
                    .into()
            },
        }
    }
}

// FIXME: with specialization can also implement From<IntoIterator<T>> but keep From<Vec<T>
impl<T> From<Vec<T>> for Fields<T> {
    fn from(mut fields: Vec<T>) -> Self {
        if fields.is_empty() {
            Fields::Zero
        } else if fields.len() == 1 {
            Fields::Single(fields.pop().unwrap())
        } else {
            Fields::Split(fields)
        }
    }
}

impl<T> From<T> for Fields<T> {
    fn from(t: T) -> Self {
        Fields::Single(t)
    }
}

impl<T> IntoIterator for Fields<T> {
    type Item = T;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let vec = match self {
            Fields::Zero => vec!(),
            Fields::Single(s) => vec!(s),
            Fields::At(v)   |
            Fields::Star(v) |
            Fields::Split(v) => v,
        };

        vec.into_iter()
    }
}

