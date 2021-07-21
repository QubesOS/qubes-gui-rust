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
}

// This unsafely implements `Castable for a type, without any checks.  It is not
// exported and is only used internally to this module.
macro_rules! unsafe_impl_castable {
    ($($i: ty,)*) => {$(
        unsafe impl Castable for $i {
            const SIZE: usize = $crate::core::mem::size_of::<$i>();
        }
    )*}
}

unsafe_impl_castable!(
    // Primitive integer types
    u8,
    u16,
    u32,
    u64,
    i8,
    i16,
    i32,
    i64,
    // Unit
    (),
);

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

/// Casts a [`Castable`] type to a `&[u8]`, without any copies
///
/// This is safe because [`Castable`] is unsafe to implement.
pub fn as_bytes_single<T: Castable>(obj: &T) -> &[u8] {
    // SAFETY: By the contract of `Castable`, `obj` has no padding bytes.
    unsafe {
        core::slice::from_raw_parts(obj as *const T as *const u8, core::mem::size_of::<T>())
    }
}

/// Casts a mutable reference to a [`Castable`] type to a `&mut [u8]`, without
/// any copies.
///
/// This is safe because [`Castable`] objects have no padding bytes, and any bit
/// pattern is valid for them.
pub fn as_mut_bytes_single<T: Castable>(obj: &mut T) -> &mut [u8] {
    unsafe {
        // Obtain a mutable pointer to `obj`
        let raw_ptr = obj as *mut T;
        // End the lifetime of `obj`, to avoid aliasing mutable references
        core::mem::forget(obj);
        // SAFETY: since &mut references are never aliased, there are currently
        // *no* references to `obj`.  Furthermore, *any* bit pattern for `obj`
        // is valid by the contract of `Castable`, so writing through the
        // returned slice will *not* place `obj` in an invalid state.
        core::slice::from_raw_parts_mut(raw_ptr as *mut u8, core::mem::size_of::<T>())
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
