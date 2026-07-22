#![doc = include_str!("../README.md")]

pub mod test;

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};
use std::rc::Rc;

/// Number of payload bytes available without a separate allocation.
///
/// This is four machine words: 32 bytes on 64-bit targets and 16 bytes on
/// 32-bit targets. A projected value also stores its projection adapters, so
/// non-zero-sized adapters count toward this capacity.
pub const INLINE_CAPACITY: usize = 4 * size_of::<usize>();

/// Maximum payload alignment supported by inline storage.
///
/// A concrete value whose alignment exceeds this value uses heap storage even
/// when its size is at most [`INLINE_CAPACITY`].
pub const INLINE_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct InlineStorage {
    words: [MaybeUninit<usize>; 4],
}

impl InlineStorage {
    const fn uninit() -> Self {
        Self {
            words: [MaybeUninit::uninit(); 4],
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.words.as_ptr().cast()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.words.as_mut_ptr().cast()
    }
}

enum Storage {
    Inline(InlineStorage),
    Heap(NonNull<u8>),
}

struct Operations<T: ?Sized> {
    project: unsafe fn(*const u8) -> *const T,
    project_mut: unsafe fn(*mut u8) -> *mut T,
    drop_value: unsafe fn(*mut u8, bool),
}

struct Projected<U, P, PM> {
    value: U,
    project: P,
    project_mut: PM,
}

/// An owning inline-or-heap smart pointer.
///
/// `Rubick<T>` exclusively owns one concrete value. If its concrete storage
/// type fits [`INLINE_CAPACITY`] and [`INLINE_ALIGNMENT`], the value is embedded
/// in the owner. Otherwise `Rubick` performs one allocation using the concrete
/// type's exact layout.
///
/// `T` may be sized, or it may be an erased target such as `dyn Trait`. Sized
/// values use [`Rubick::new`]. Erased targets use [`Rubick::new_projected`]
/// because stable Rust does not support general `CoerceUnsized` implementations
/// for custom smart pointers.
///
/// Moving an unpinned `Rubick` also moves an inline value and changes that
/// value's address. Heap values retain their allocation address across owner
/// moves, but this is an implementation detail rather than a stable-address
/// API guarantee. Standard [`std::pin::Pin`] rules apply once an owner is pinned.
///
/// The owner is conservatively `!Send` and `!Sync`: after concrete type erasure,
/// its operation table cannot express all auto traits of the hidden value.
pub struct Rubick<T: ?Sized> {
    storage: Storage,
    operations: Operations<T>,
    target: PhantomData<T>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl<T: 'static> Rubick<T> {
    /// Creates an owner for a sized value.
    ///
    /// The value is stored inline when both its size and alignment fit the
    /// configured limits. Otherwise this method performs one heap allocation.
    /// Zero-sized values are inline when their alignment fits.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_rubick::Rubick;
    ///
    /// let mut name = Rubick::new(String::from("Aimer"));
    /// name.push_str(" GUI");
    /// assert_eq!(&*name, "Aimer GUI");
    /// ```
    pub fn new(value: T) -> Self {
        Self::from_concrete(value, project_identity::<T>, project_identity_mut::<T>)
    }
}

impl<T: ?Sized + 'static> Rubick<T> {
    /// Creates an owner and explicitly projects its concrete value to `T`.
    ///
    /// `project` and `project_mut` convert shared and exclusive borrows of `U`
    /// into borrows of the same erased target. `Rubick` invokes the appropriate
    /// adapter on every borrow, using the concrete value's current address. It
    /// never stores a pointer into inline storage across owner moves.
    ///
    /// Named function items and non-capturing closures are normally zero-sized.
    /// Capturing closures and coerced function pointers are owned alongside `U`
    /// and count toward inline capacity.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_rubick::Rubick;
    ///
    /// trait Counter {
    ///     fn increment(&mut self);
    ///     fn value(&self) -> usize;
    /// }
    ///
    /// struct Count(usize);
    ///
    /// impl Counter for Count {
    ///     fn increment(&mut self) { self.0 += 1; }
    ///     fn value(&self) -> usize { self.0 }
    /// }
    ///
    /// fn as_counter(value: &Count) -> &(dyn Counter + 'static) { value }
    /// fn as_counter_mut(value: &mut Count) -> &mut (dyn Counter + 'static) { value }
    ///
    /// let mut count: Rubick<dyn Counter> =
    ///     Rubick::new_projected(Count(2), as_counter, as_counter_mut);
    /// count.increment();
    /// assert_eq!(count.value(), 3);
    /// ```
    pub fn new_projected<U, P, PM>(value: U, project: P, project_mut: PM) -> Self
    where
        U: 'static,
        P: for<'a> Fn(&'a U) -> &'a T + 'static,
        PM: for<'a> Fn(&'a mut U) -> &'a mut T + 'static,
    {
        let value = Projected {
            value,
            project,
            project_mut,
        };
        Self::from_concrete(
            value,
            project_stored::<U, P, PM, T>,
            project_stored_mut::<U, P, PM, T>,
        )
    }

    /// Returns `true` when the concrete storage is embedded in this owner.
    ///
    /// Inline does not necessarily mean stack allocated. For example, an inline
    /// `Rubick` in a `Vec` is embedded in the vector's allocation and still
    /// requires no additional allocation for its value.
    pub fn is_inline(&self) -> bool {
        matches!(self.storage, Storage::Inline(_))
    }

    /// Returns `true` when the concrete storage uses a separate heap allocation.
    ///
    /// This is always the inverse of [`Rubick::is_inline`].
    pub fn is_heap(&self) -> bool {
        matches!(self.storage, Storage::Heap(_))
    }

    fn from_concrete<U: 'static>(
        value: U,
        project: unsafe fn(*const u8) -> *const T,
        project_mut: unsafe fn(*mut u8) -> *mut T,
    ) -> Self {
        let operations = Operations {
            project,
            project_mut,
            drop_value: drop_value::<U>,
        };

        if size_of::<U>() <= INLINE_CAPACITY && align_of::<U>() <= INLINE_ALIGNMENT {
            let mut owner = Self {
                storage: Storage::Inline(InlineStorage::uninit()),
                operations,
                target: PhantomData,
                not_send_or_sync: PhantomData,
            };
            // SAFETY: The size and alignment checks above guarantee the inline
            // buffer can hold `U`. It is currently uninitialized and becomes
            // initialized exactly once by this write.
            unsafe { ptr::write(owner.data_mut().cast::<U>(), value) };
            owner
        } else {
            let pointer = NonNull::from(Box::leak(Box::new(value))).cast();
            Self {
                storage: Storage::Heap(pointer),
                operations,
                target: PhantomData,
                not_send_or_sync: PhantomData,
            }
        }
    }
}

impl<T: ?Sized> Rubick<T> {
    fn data(&self) -> *const u8 {
        match &self.storage {
            Storage::Inline(storage) => storage.as_ptr(),
            Storage::Heap(pointer) => pointer.as_ptr(),
        }
    }

    fn data_mut(&mut self) -> *mut u8 {
        match &mut self.storage {
            Storage::Inline(storage) => storage.as_mut_ptr(),
            Storage::Heap(pointer) => pointer.as_ptr(),
        }
    }
}

/// Borrows the owned value through its sized or projected target type.
impl<T: ?Sized> Deref for Rubick<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `operations.project` was created for the concrete initialized
        // value in `storage`. The returned borrow is bounded by `self`.
        unsafe { &*(self.operations.project)(self.data()) }
    }
}

/// Mutably borrows the owned value through its sized or projected target type.
impl<T: ?Sized> DerefMut for Rubick<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let project_mut = self.operations.project_mut;
        let data = self.data_mut();
        // SAFETY: Exclusive access to the owner gives exclusive access to its
        // concrete value, and the matching projection preserves that lifetime.
        unsafe { &mut *project_mut(data) }
    }
}

/// Returns a shared borrow of the owned target.
impl<T: ?Sized> AsRef<T> for Rubick<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

/// Returns an exclusive borrow of the owned target.
impl<T: ?Sized> AsMut<T> for Rubick<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

/// Drops the concrete value exactly once and frees heap storage when present.
impl<T: ?Sized> Drop for Rubick<T> {
    fn drop(&mut self) {
        let is_heap = matches!(self.storage, Storage::Heap(_));
        let drop_value = self.operations.drop_value;
        let data = self.data_mut();
        // SAFETY: `drop_value` matches the single initialized concrete value.
        // Heap mode reconstructs the original `Box<U>` and inline mode only
        // drops in place, so destruction and deallocation each happen once.
        unsafe { drop_value(data, is_heap) };
    }
}

unsafe fn project_identity<U>(pointer: *const u8) -> *const U {
    pointer.cast()
}

unsafe fn project_identity_mut<U>(pointer: *mut u8) -> *mut U {
    pointer.cast()
}

unsafe fn project_stored<U, P, PM, T: ?Sized>(pointer: *const u8) -> *const T
where
    P: for<'a> Fn(&'a U) -> &'a T,
{
    // SAFETY: This adapter is installed only when the storage contains the
    // matching `Projected<U, P, PM>` concrete type.
    let stored = unsafe { &*pointer.cast::<Projected<U, P, PM>>() };
    (stored.project)(&stored.value)
}

unsafe fn project_stored_mut<U, P, PM, T: ?Sized>(pointer: *mut u8) -> *mut T
where
    PM: for<'a> Fn(&'a mut U) -> &'a mut T,
{
    // SAFETY: This adapter is installed only when the storage contains the
    // matching `Projected<U, P, PM>` concrete type and the caller has exclusive
    // access to its owner.
    let stored = unsafe { &mut *pointer.cast::<Projected<U, P, PM>>() };
    (stored.project_mut)(&mut stored.value)
}

unsafe fn drop_value<U>(pointer: *mut u8, is_heap: bool) {
    if is_heap {
        // SAFETY: Heap mode originated from `Box<U>` and has not been freed.
        unsafe { drop(Box::from_raw(pointer.cast::<U>())) };
    } else {
        // SAFETY: Inline mode contains one initialized `U` at this address.
        unsafe { ptr::drop_in_place(pointer.cast::<U>()) };
    }
}
