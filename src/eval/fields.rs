use env::{StringWrapper, VariableEnvironment};
use std::borrow::Borrow;
use std::vec;

lazy_static! {
    static ref IFS: String = { String::from("IFS") };
}

const IFS_DEFAULT: &'static str = " \t\n";

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

    /// Splits a vector of fields further based on the contents of the `IFS`
    /// variable (i.e. as long as it is non-empty). Any empty fields, original
    /// or otherwise created will be discarded.
    pub fn split<E: ?Sized>(self, env: &E) -> Fields<T>
        where E: VariableEnvironment,
              E::VarName: Borrow<String>,
              E::Var: Borrow<String>,
    {
        match self {
            Fields::Zero      => Fields::Zero,
            Fields::Single(f) => split_fields_internal(vec!(f), env).into(),
            Fields::At(fs)    => Fields::At(split_fields_internal(fs, env)),
            Fields::Star(fs)  => Fields::Star(split_fields_internal(fs, env)),
            Fields::Split(fs) => Fields::Split(split_fields_internal(fs, env)),
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

/// Actual implementation of `split_fields`.
fn split_fields_internal<T, E: ?Sized>(words: Vec<T>, env: &E) -> Vec<T>
    where T: StringWrapper,
          E: VariableEnvironment,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    // If IFS is set but null, there is nothing left to split
    let ifs = env.var(&IFS).map_or(IFS_DEFAULT, |s| s.borrow().as_str());
    if ifs.is_empty() {
        return words;
    }

    let whitespace: Vec<char> = ifs.chars().filter(|c| c.is_whitespace()).collect();

    let mut fields = Vec::with_capacity(words.len());
    'word: for word in words.iter().map(StringWrapper::as_str) {
        if word.is_empty() {
            continue;
        }

        let mut iter = word.chars().enumerate().peekable();
        loop {
            let start;
            loop {
                match iter.next() {
                    // If we are still skipping leading whitespace, and we hit the
                    // end of the word there are no fields to create, even empty ones.
                    None => continue 'word,
                    Some((idx, c)) => {
                        if whitespace.contains(&c) {
                            continue;
                        } else if ifs.contains(c) {
                            // If we hit an IFS char here then we have encountered an
                            // empty field, since the last iteration of this loop either
                            // had just consumed an IFS char, or its the start of the word.
                            // In either case the result should be the same.
                            fields.push(String::new().into());
                        } else {
                            // Must have found a regular field character
                            start = idx;
                            break;
                        }
                    },
                }
            }

            let end;
            loop {
                match iter.next() {
                    None => {
                        end = None;
                        break;
                    },
                    Some((idx, c)) => if ifs.contains(c) {
                        end = Some(idx);
                        break;
                    },
                }
            }

            let field = match end {
                Some(end) => &word[start..end],
                None      => &word[start..],
            };

            fields.push(String::from(field).into());

            // Since now we've hit an IFS character, we need to also skip past
            // any adjacent IFS whitespace as well. This also conveniently
            // ignores any trailing IFS whitespace in the input as well.
            loop {
                match iter.peek() {
                    Some(&(_, c)) if whitespace.contains(&c) => {
                        iter.next();
                    },
                    Some(_) |
                    None => break,
                }
            }
        }
    }

    fields.shrink_to_fit();
    fields
}
