//! Macros for plain-old-data (POD) types.
//!
//! These macros are used to construct types that can be safely cast two and
//! from a raw byte sequence.
#![no_std]

/// A trait for types that can be casted to and from a raw byte slice.
///
/// # Safety
///
/// This trait MUST NOT be implemented on any type that contains padding, or
/// that has invalid bit patterms.
///
/// This trait SHOULD NOT be implemented except by using the `castable!` macro.
/// Doing so is explicitly not supported.
#[doc(hidden)]
pub extern crate core;

pub unsafe trait Castable {
    /// The size of the type.  MUST be equal to the size as determined by
    /// [`core::mem::size_of`].
    const SIZE: usize;

    /// Casts a [`Castable`] type to a `&[u8]`, without any copies.
    ///
    /// This is safe because [`Castable`] is unsafe to implement.
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
unsafe impl<T, const COUNT: usize> Castable for [T; COUNT] {
    const SIZE: usize = core::mem::size_of::<[T; COUNT]>();
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
/// Flipping the order would not make this compile, as the compiler would
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
                pub $name : $ty
            ),*
        }
        unsafe impl $crate::Castable for $s {
            const SIZE: usize = {
                const SIZE: usize = ($(
                    ({
                        let _: [u8; $crate::core::mem::size_of::<$ty>()] = [0u8; <$ty as $crate::Castable>::SIZE];
                        $crate::core::mem::size_of::<$ty>()
                    }) +
                )* 0);
                let _: [u8; $crate::core::mem::size_of::<$s>()] = [0u8; SIZE];
                SIZE
            };
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
        core::slice::from_raw_parts_mut(raw_ptr as *mut u8, len * core::mem::size_of::<T>())
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
        core::slice::from_raw_parts(raw_ptr as *const u8, len * core::mem::size_of::<T>())
    }
}
