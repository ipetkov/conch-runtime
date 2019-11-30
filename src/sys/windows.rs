//! Extensions and implementations specific to Windows platforms.

use std::io::{Error, Result};

pub mod io;

pub(crate) trait IsZero {
    fn is_zero(&self) -> bool;
}

macro_rules! impl_is_zero {
    ($($t:ident)*) => ($(impl IsZero for $t {
        fn is_zero(&self) -> bool {
            *self == 0
        }
    })*)
}

impl_is_zero! { i8 i16 i32 i64 isize u8 u16 u32 u64 usize }

impl<T> IsZero for *mut T {
    fn is_zero(&self) -> bool {
        self.is_null()
    }
}

pub(crate) fn cvt<I: IsZero>(i: I) -> Result<I> {
    if i.is_zero() {
        Err(Error::last_os_error())
    } else {
        Ok(i)
    }
}
