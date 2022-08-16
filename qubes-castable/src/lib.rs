//! Macros for plain-old-data (POD) types.
//!
//! These macros are used to construct types that can be safely cast two and
//! from a raw byte sequence.
#![no_std]

#[doc(hidden)]
pub extern crate core;
use core::mem::size_of;

/// If the provided expression is false, fail the build with a type error.
#[macro_export]
macro_rules! static_assert {
    ($e: expr) => {{
        let [] = [0; if $e { 0 } else { 1 }];
    }};
}

/// A trait for types that can be casted to and from a raw byte slice.
///
/// All [`Castable`] types are `Copy`, and thus do *not* implement `Drop`.
///
/// # Safety
///
/// This trait MUST NOT be implemented on any type that contains padding, or
/// that has invalid bit patterms.
///
/// This trait SHOULD NOT be implemented except by using the `castable!` macro.
/// Doing so is explicitly not supported.
///
/// Arrays of [`Castable`] types are themselves [`Castable`]:
///
/// ```rust
/// # use qubes_castable::Castable;
/// assert_eq!(Castable::as_bytes(&[0x0F0Fu16; 2]), &[0xF, 0xF, 0xF, 0xF]);
/// ```
///
/// But arrays of non-[`Castable`] types are not:
///
/// ```rust,compile_fail
/// # use qubes_castable::Castable;
/// assert_eq!(Castable::as_bytes(&[(0x0F0Fu16,); 2]), &[0xF, 0xF, 0xF, 0xF]);
/// ```
pub unsafe trait Castable:
    Copy + Clone + Eq + PartialEq + Ord + PartialOrd + core::fmt::Debug + core::hash::Hash + Sized
{
    /// The size of the type.  MUST be equal to the size as determined by
    /// [`core::mem::size_of`].
    const SIZE: usize;

    /// Casts a [`Castable`] type to a `&[u8]`, without any copies.
    ///
    /// This is safe because [`Castable`] is unsafe to implement.
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        // SAFETY: By the contract of `Castable`, `obj` has no padding bytes.
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                core::mem::size_of_val(self),
            )
        }
    }

    /// Casts a mutable reference to a [`Castable`] type to a `&mut [u8]`,
    /// without any copies.
    ///
    /// This is safe because [`Castable`] objects have no padding bytes, and any
    /// bit pattern is valid for them.
    #[inline]
    fn as_mut_bytes(&mut self) -> &mut [u8] {
        unsafe {
            let size = core::mem::size_of_val(self);
            // Obtain a mutable pointer to `obj`
            let raw_ptr = self as *mut Self;
            // End the lifetime of `obj`, to avoid aliasing mutable references
            core::mem::forget(self);
            // SAFETY: since &mut references are never aliased, there are currently
            // *no* references to `obj`.  Furthermore, *any* bit pattern for `obj`
            // is valid by the contract of `Castable`, so writing through the
            // returned slice will *not* place `obj` in an invalid state.
            core::slice::from_raw_parts_mut(raw_ptr as *mut u8, size)
        }
    }

    /// Creates a [`Castable`] type from an `&[u8]`.
    ///
    /// This is safe because [`Castable`] objects have no padding bytes, and any
    /// bit pattern is valid for them.
    ///
    /// # Panics
    ///
    /// Panics if the length of `buf` is not equal to `size_of::<Self>`.
    ///
    /// # Example
    ///
    /// Use it correctly:
    ///
    /// ```rust
    /// # use core::num::NonZeroU8;
    /// # use qubes_castable::Castable;
    /// # use core::convert::TryInto;
    /// assert_eq!(<Option<NonZeroU8>>::from_bytes(&[0]), None);
    /// assert_eq!(<Option<NonZeroU8>>::from_bytes(&[1]), Some(1u8.try_into().unwrap()));
    /// ```
    ///
    /// Pass an incorrect length and cause a panic:
    ///
    /// ```rust,should_panic
    /// # use core::num::NonZeroU8;
    /// # use qubes_castable::Castable;
    /// # use core::convert::TryInto;
    /// drop(<Option<NonZeroU8>>::from_bytes(&[]));
    /// ```
    #[inline]
    fn from_bytes(buf: &[u8]) -> Self {
        assert_eq!(
            buf.len(),
            size_of::<Self>(),
            "Size mismatch: got {} bytes but expected {}",
            buf.len(),
            size_of::<Self>()
        );
        if size_of::<Self>() == 0 {
            // For a zero-sized type, it does not matter what value to return,
            // as there is only one.  Use `zeroed` to return something.
            Self::zeroed()
        } else {
            // SAFETY: `buf` was checked to be the same size as `Self`, and
            // `Self` has a nonzero length, `buf.len()` must also be nonzero.
            // Therefore, `buf.as_ptr()` is a valid pointer that can have
            // `size_of::<Self>()` bytes read from it.  Since `Self` is
            // `Castable`, *any* bit pattern is valid for it, so this cannot
            // create a value with an invalid bit pattern.  `buf.ptr()` is *not*
            // guaranteed to be aligned, so use `read_unaligned`.
            unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const Self) }
        }
    }

    /// Creates a [`Castable`] type from an `&[u8]`.
    ///
    /// This is safe because [`Castable`] objects have no padding bytes, and any
    /// bit pattern is valid for them.
    ///
    /// # Returns
    ///
    /// On success, this returns the object read, along with the remainder of
    /// the byte slice.  If the slice is too short, returns None.
    ///
    /// # Example
    ///
    /// Use it successfully:
    ///
    /// ```rust
    /// # use core::num::NonZeroU8;
    /// # use qubes_castable::Castable;
    /// # use core::convert::TryInto;
    /// assert_eq!(<Option<NonZeroU8>>::read_from_buf(&mut &[0][..]), Some(None));
    /// assert_eq!(<Option<NonZeroU8>>::read_from_buf(&mut &[1u8][..]), Some(1u8.try_into().ok()));
    /// // excess bytes at the end are okay
    /// assert_eq!(<Option<NonZeroU8>>::read_from_buf(&mut &[1u8, 0u8][..]), Some(1u8.try_into().ok()));
    /// ```
    ///
    /// Passing too few bytes gets None:
    ///
    /// ```rust
    /// # use core::num::NonZeroU8;
    /// # use qubes_castable::Castable;
    /// # use core::convert::TryInto;
    /// assert_eq!(<Option<NonZeroU8>>::read_from_buf(&mut &[][..]), None);
    /// ```
    #[inline]
    fn read_from_buf(buf: &mut &[u8]) -> Option<Self> {
        let buf_v = *buf;
        if buf_v.len() < size_of::<Self>() {
            return None;
        }
        let res = Self::from_bytes(&buf_v[..size_of::<Self>()]);
        *buf = &buf_v[size_of::<Self>()..];
        Some(res)
    }

    /// Creates a zeroed instance of any [`Castable`] type
    ///
    /// This is safe because [`Castable`] objects have no padding bytes, and any
    /// bit pattern is valid for them.
    #[inline]
    fn zeroed() -> Self {
        // SAFETY:  Since `Self` is `Castable`, *any* bit pattern is valid for
        // it, so this cannot create a value with an invalid bit pattern.
        unsafe { core::mem::zeroed() }
    }
}

// This unsafely implements Castable for a type, checking only that it is
// FFI-safe.  It is not exported and is only used internally to this module.
macro_rules! unsafe_impl_castable {
    ($i: ty) => {
        unsafe impl Castable for $i {
            const SIZE: usize = {
                #[forbid(improper_ctypes)]
                #[forbid(improper_ctypes_definitions)]
                extern "C" fn _dummy() -> $i {
                    unreachable!()
                }
                $crate::core::mem::size_of::<$i>()
            };
        }
    };
}

unsafe_impl_castable!(());

// Unsafely implement Castable for Option<NonZero*>, but check layouts first
macro_rules! unsafe_castable_nonzero {
    ($(($i: ident, $j: ty),)*) => {$(
        unsafe_impl_castable!($j);
        unsafe impl Castable for Option<$crate::core::num::$i> {
            const SIZE: usize = {
                #[forbid(improper_ctypes)]
                #[forbid(improper_ctypes_definitions)]
                extern "C" fn _dummy() -> Option<$crate::core::num::$i> { unreachable!() }
                let _: [u8; $crate::core::mem::size_of::<Option<$crate::core::num::$i>>()] =
                    [0u8; $crate::core::mem::size_of::<$j>()];
                $crate::core::mem::size_of::<$j>()
            };
        }
    )*}
}

unsafe_castable_nonzero! {
    (NonZeroU8, u8),
    (NonZeroU16, u16),
    (NonZeroU32, u32),
    (NonZeroU64, u64),
    (NonZeroI8, i8),
    (NonZeroI16, i16),
    (NonZeroI32, i32),
    (NonZeroI64, i64),
}

// Arrays of castable types are castable
unsafe impl<T: Castable, const COUNT: usize> Castable for [T; COUNT] {
    const SIZE: usize = size_of::<[T; COUNT]>();
}

/// Create a struct that is marked as castable, meaning that it can be converted
/// to and from a byte slice without any run-time overhead.  This macro:
///
/// 1. Creates a struct with the fields and documentation provided.
/// 2. Implements the `Castable` trait for that struct, along with safety checks
///    to ensure that doing so is in fact safe.
///
/// # Examples
///
/// This will not compile, as the compiler would insert padding:
///
/// ```rust,compile_fail
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         s: u32,
///         /// Second field
///         y: u64,
///     }
/// };
/// ```
///
/// Flipping the order would make this not compile, as the compiler would
/// need to insert padding at the end:
///
/// ```rust,compile_fail
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         y: u64,
///         /// Second field
///         s: u32,
///     }
/// };
/// ```
///
/// This will also not compile, as `bool` has invalid bit patterns:
///
/// ```rust,compile_fail
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         s: u32,
///         /// Second field
///         y: bool,
///     }
/// };
/// ```
///
/// This, however, will compile:
///
/// ```rust
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         s: u32,
///         /// Second field
///         y: u32,
///     }
/// };
/// ```
///
/// Castable structs can be nested:
///
/// ```rust
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         s: u32,
///         /// Second field
///         y: u32,
///     }
/// };
///
/// castable! {
///     /// A struct
///     struct Test2 {
///         /// First field
///         s: u32,
///         /// Second field
///         y: Test,
///     }
/// };
/// ```
///
/// And the macro can define several structs at a time:
///
/// ```rust
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Test {
///         /// First field
///         s: u32,
///         /// Second field
///         y: u32,
///     }
///
///     /// A struct
///     struct Test2 {
///         /// First field
///         s: u32,
///         /// Second field
///         y: Test,
///     }
/// };
/// ```
///
/// The `NonZero*` types from `core::num` are not castable
///
/// ```rust,compile_fail
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Bad {
///         /// First field
///         s: core::num::NonZeroU32,
///     }
/// }
/// ```
///
/// But `Option<NonZero*>` is:
///
/// ```rust
/// # use qubes_castable::castable;
/// castable! {
///     /// A struct
///     struct Good {
///         /// First field
///         s: Option<core::num::NonZeroU32>,
///         /// Second field
///         t: Option<std::num::NonZeroU32>,
///     }
/// }
/// ```
#[macro_export]
macro_rules! castable {
    ($($(#[doc = $m: expr])*
    $p: vis struct $s: ident {
        $(
            $(#[doc = $n: expr])*
            $name: ident : $ty : ty
        ),*$(,)?
    })+) => {
        $(
        #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
        $(#[doc = $m])*
        #[repr(C)]
        $p struct $s {
            $(
                $(#[doc = $n])*
                pub $name : $ty,
            )*
        }
        unsafe impl $crate::Castable for $s {
            const SIZE: usize = {
                const SIZE: usize = ($(
                    ({
                        $crate::static_assert!(
                            $crate::core::mem::size_of::<$ty>() == <$ty as $crate::Castable>::SIZE
                        );
                        $crate::core::mem::size_of::<$ty>()
                    }) +
                )* 0);
                $crate::static_assert!($crate::core::mem::size_of::<$s>() == SIZE);
                SIZE
            };
        }
        impl $crate::core::default::Default for $s {
            fn default() -> Self {
                <$s as $crate::Castable>::zeroed()
            }
        }
        )+
    }
}

/// Casts a mutable reference to a slice of [`Castable`] types to a `&mut [u8]`,
/// without any copies.
///
/// This is safe because [`Castable`] objects have no padding bytes, and any bit
/// pattern is valid for them.
pub fn as_mut_bytes<T: Castable>(obj: &mut [T]) -> &mut [u8] {
    unsafe {
        // Obtain a mutable pointer to `obj` and the length
        let (raw_ptr, len) = (obj.as_mut_ptr(), obj.len());
        // End the lifetime of `obj`, to avoid aliasing mutable references
        core::mem::forget(obj);
        // SAFETY: since &mut references are never aliased, there are currently
        // *no* references to `obj`.  Furthermore, *any* bit pattern for `obj`
        // is valid by the contract of `Castable`, so writing through the
        // returned slice will *not* place `obj` in an invalid state.  Finally,
        // the number of valid bytes in a slice is exactly size_of::<T>() * len.
        core::slice::from_raw_parts_mut(raw_ptr as *mut u8, len * size_of::<T>())
    }
}

/// Casts a reference to a slice of [`Castable`] types to a `&[u8]`, without any
/// copies.
///
/// This is safe because [`Castable`] objects have no padding bytes, and any bit
/// pattern is valid for them.
pub fn as_bytes<T: Castable>(obj: &[T]) -> &[u8] {
    unsafe {
        // Obtain a pointer to `obj` and the length
        let (raw_ptr, len) = (obj.as_ptr(), obj.len());
        // SAFETY: *any* bit pattern for `obj` is valid by the contract of
        // `Castable`, so writing through the returned slice will *not* place
        // `obj` in an invalid state.  Finally, the number of valid bytes in a
        // slice is exactly size_of::<T>() * len.
        core::slice::from_raw_parts(raw_ptr as *const u8, len * size_of::<T>())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn basic() {
        castable! {
            struct Simple {
                i: u8,
            }
        }
        let mut dummy: Simple = Default::default();
        assert_eq!(dummy.i, 0);
        assert_eq!(dummy.as_bytes(), &[0]);
        let s = dummy.as_mut_bytes();
        assert_eq!(s, &[0]);
        s[0] = 60;
        assert_eq!(dummy.i, 60);
    }

    #[test]
    fn options() {
        use core::{convert::TryInto, num::NonZeroU32};
        castable! {
            struct Options {
                i: Option<NonZeroU32>
            }
        }

        let mut dummy = <Options as Default>::default();
        assert_eq!(dummy.i, None);
        assert_eq!(dummy.as_bytes(), &[0, 0, 0, 0]);
        let s = dummy.as_mut_bytes();
        assert_eq!(s, &[0, 0, 0, 0]);
        s[0] = 100;
        assert_eq!(
            dummy,
            Options {
                i: Some(u32::to_be(100u32 << 24).try_into().unwrap())
            }
        );
    }

    #[test]
    #[should_panic = "Size mismatch: got 0 bytes but expected 1"]
    fn mismatch() {
        drop(<Option<core::num::NonZeroU8>>::from_bytes(&[]))
    }
}
