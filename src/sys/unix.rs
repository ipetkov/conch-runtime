//! Extensions and implementations specific to Unix platforms.

use std::io::{Error, ErrorKind, Result};

pub mod io;

pub(crate) trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

macro_rules! impl_is_minus_one {
    ($($t:ident)*) => ($(impl IsMinusOne for $t {
        fn is_minus_one(&self) -> bool {
            *self == -1
        }
    })*)
}

impl_is_minus_one! { i8 i16 i32 i64 isize }

pub(crate) fn cvt_r<T: IsMinusOne, F: FnMut() -> T>(mut f: F) -> Result<T> {
    loop {
        let ret = {
            let status = f();
            if status.is_minus_one() {
                Err(Error::last_os_error())
            } else {
                Ok(status)
            }
        };

        match ret {
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            other => return other,
        }
    }
}
