#![doc = include_str!("../README.md")]

mod erase;
mod operations;
mod pool;
mod storage;
pub mod test;

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::rc::Rc;

pub use crate::erase::ErasedFrom;

use crate::operations::{AdaptedTable, ErasedTable, IdentityTable, Operations, Projected};
use crate::storage::InlineStorage;

/// Number of machine words an owner reserves for its payload by default.
///
/// Four words is a compromise for values whose concrete type is not known to
/// the owner's author. A type alias may pick a different capacity, which is
/// how the framework tunes widgets and elements independently.
pub const DEFAULT_WORDS: usize = 4;

/// Payload bytes available without a separate allocation, at the default
/// capacity.
///
/// This is [`DEFAULT_WORDS`] machine words: 32 bytes on 64-bit targets and 16
/// bytes on 32-bit targets. An owner with an explicit capacity reports its own
/// limit through [`Rubick::INLINE_CAPACITY`].
pub const INLINE_CAPACITY: usize = DEFAULT_WORDS * size_of::<*mut u8>();

/// Maximum payload alignment supported by inline storage.
///
/// Inline storage is an array of machine words, so it guarantees exactly word
/// alignment. A concrete value with a stricter alignment — a `repr(align)`
/// type or a SIMD vector — uses heap storage even when it would otherwise fit.
/// Framework values are word aligned, and forcing a larger alignment on every
/// owner would waste padding in each of them.
pub const INLINE_ALIGNMENT: usize = align_of::<*mut u8>();

/// An owning inline-or-heap smart pointer.
///
/// `Rubick<T, WORDS>` exclusively owns one concrete value. If the concrete
/// storage type fits `WORDS` machine words and [`INLINE_ALIGNMENT`], the value
/// is embedded in the owner. Otherwise `Rubick` takes one block from a
/// thread-local pool sized for the concrete layout.
///
/// # Representation
///
/// An owner is `WORDS + 1` machine words: the payload buffer plus one pointer
/// to a `'static` operation table describing the concrete type. There is no
/// runtime storage discriminant, because inline versus heap is a compile-time
/// function of the concrete layout and therefore a property of that table. In
/// heap mode the first payload word holds the allocation pointer and the
/// remaining words are unused.
///
/// ```
/// use aimer_rubick::Rubick;
///
/// assert_eq!(size_of::<Rubick<u32>>(), 5 * size_of::<usize>());
/// assert_eq!(align_of::<Rubick<u32>>(), align_of::<usize>());
/// ```
///
/// # Choosing a capacity
///
/// `WORDS` must be at least one so that heap mode can store its pointer; a
/// smaller value fails to compile. Use a small capacity for targets whose
/// concrete types are known to be large, and a larger capacity when avoiding
/// the allocation matters more than the owner's own size:
///
/// ```
/// use aimer_rubick::Rubick;
///
/// // A thin owner: two words, never inline for anything but a word.
/// type Thin = Rubick<dyn std::fmt::Debug, 1>;
/// // A roomy owner: eight words of payload.
/// type Roomy = Rubick<dyn std::fmt::Debug, 8>;
///
/// assert_eq!(size_of::<Thin>(), 2 * size_of::<usize>());
/// assert_eq!(size_of::<Roomy>(), 9 * size_of::<usize>());
/// ```
///
/// # Targets
///
/// `T` may be sized, or it may be an erased target such as `dyn Trait`.
/// Sized values use [`Rubick::new`]. Erased targets use [`Rubick::erase`],
/// which needs an [`ErasedFrom`] implementation because stable Rust does not
/// support `CoerceUnsized` for custom smart pointers, or
/// [`Rubick::new_projected`] when the borrow is not a plain unsizing.
///
/// Moving an unpinned `Rubick` also moves an inline value and changes that
/// value's address. Heap values retain their allocation address across owner
/// moves, but this is an implementation detail rather than a stable-address
/// API guarantee. Standard [`std::pin::Pin`] rules apply once an owner is
/// pinned.
///
/// The owner is conservatively `!Send` and `!Sync`: after concrete type
/// erasure, its operation table cannot express all auto traits of the hidden
/// value, and the payload pool is thread local.
pub struct Rubick<T: ?Sized + 'static, const WORDS: usize = DEFAULT_WORDS> {
    storage: InlineStorage<WORDS>,
    operations: &'static Operations<T>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl<T: 'static> Rubick<T, DEFAULT_WORDS> {
    /// Creates an owner for a sized value at the default capacity.
    ///
    /// The value is stored inline when both its size and alignment fit
    /// [`INLINE_CAPACITY`]. Otherwise this method takes one pooled heap block.
    /// Zero-sized values are always inline when their alignment fits.
    ///
    /// This constructor deliberately fixes the capacity so that `Rubick::new`
    /// needs no turbofish: Rust never falls back to a const parameter's
    /// default during inference. Use [`Rubick::erase`], which accepts any
    /// capacity, when a sized value belongs in an owner of a different size.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_rubick::Rubick;
    ///
    /// let mut name = Rubick::new(String::from("Aimer"));
    /// name.push_str(" GUI");
    ///
    /// assert_eq!(&*name, "Aimer GUI");
    /// ```
    #[inline]
    pub fn new(value: T) -> Self {
        Self::from_parts(value, IdentityTable::<T, DEFAULT_WORDS>::REF)
    }
}

impl<T: ?Sized + 'static, const WORDS: usize> Rubick<T, WORDS> {
    /// Payload bytes this owner can hold without a separate allocation.
    pub const INLINE_CAPACITY: usize = InlineStorage::<WORDS>::CAPACITY;

    /// Creates an owner by erasing `value` to the target `T`.
    ///
    /// Borrowing the result costs one pointer rebuild and no adapter call,
    /// because [`ErasedFrom`] supplies the target metadata as a constant. This
    /// is the constructor to use for `dyn Trait` targets.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_rubick::{ErasedFrom, Rubick};
    ///
    /// trait Counter {
    ///     fn increment(&mut self);
    ///     fn value(&self) -> usize;
    /// }
    ///
    /// // SAFETY: The template is `null::<C>()` coerced to the target.
    /// unsafe impl<C: Counter + 'static> ErasedFrom<C> for dyn Counter {
    ///     const TEMPLATE: *const Self = std::ptr::null::<C>() as *const dyn Counter;
    /// }
    ///
    /// struct Count(usize);
    ///
    /// impl Counter for Count {
    ///     fn increment(&mut self) {
    ///         self.0 += 1;
    ///     }
    ///
    ///     fn value(&self) -> usize {
    ///         self.0
    ///     }
    /// }
    ///
    /// let mut count: Rubick<dyn Counter> = Rubick::erase(Count(2));
    /// count.increment();
    ///
    /// assert_eq!(count.value(), 3);
    /// assert!(count.is_inline());
    /// ```
    #[inline]
    pub fn erase<U: 'static>(value: U) -> Self
    where
        T: ErasedFrom<U>,
    {
        Self::from_parts(value, ErasedTable::<U, T, WORDS>::REF)
    }

    /// Creates an owner and explicitly projects its concrete value to `T`.
    ///
    /// `project` and `project_mut` convert shared and exclusive borrows of `U`
    /// into borrows of the same erased target. `Rubick` invokes the appropriate
    /// adapter on every borrow, using the concrete value's current address. It
    /// never stores a pointer into inline storage across owner moves.
    ///
    /// Prefer [`Rubick::erase`] whenever the borrow is a plain unsizing
    /// coercion: it stores no adapters alongside the value and calls nothing
    /// when borrowing. Reach for this constructor when the projection is a
    /// genuine transformation, such as exposing an interior field or choosing
    /// a target that depends on the value.
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
    /// struct Envelope {
    ///     stamp: u32,
    ///     message: String,
    /// }
    ///
    /// let letter: Rubick<String> = Rubick::new_projected(
    ///     Envelope {
    ///         stamp: 7,
    ///         message: String::from("hello"),
    ///     },
    ///     |envelope: &Envelope| &envelope.message,
    ///     |envelope: &mut Envelope| &mut envelope.message,
    /// );
    ///
    /// assert_eq!(&*letter, "hello");
    /// ```
    #[inline]
    pub fn new_projected<U, P, PM>(value: U, project: P, project_mut: PM) -> Self
    where
        U: 'static,
        P: for<'a> Fn(&'a U) -> &'a T + 'static,
        PM: for<'a> Fn(&'a mut U) -> &'a mut T + 'static,
    {
        Self::from_parts(
            Projected {
                value,
                project,
                project_mut,
            },
            AdaptedTable::<U, P, PM, T, WORDS>::REF,
        )
    }

    /// Destroys the current payload and stores `value` in its place.
    ///
    /// Reconciliation regenerates a tree whose nodes usually keep their
    /// concrete types, so the replacement layout normally matches the one
    /// already allocated. In that case this method reuses the existing heap
    /// block and performs no allocator work at all; otherwise it releases the
    /// old block and takes a fitting one.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_rubick::{ErasedFrom, Rubick};
    ///
    /// trait Label {
    ///     fn text(&self) -> &str;
    /// }
    ///
    /// // SAFETY: The template is `null::<L>()` coerced to the target.
    /// unsafe impl<L: Label + 'static> ErasedFrom<L> for dyn Label {
    ///     const TEMPLATE: *const Self = std::ptr::null::<L>() as *const dyn Label;
    /// }
    ///
    /// struct Title(&'static str);
    ///
    /// impl Label for Title {
    ///     fn text(&self) -> &str {
    ///         self.0
    ///     }
    /// }
    ///
    /// let mut label: Rubick<dyn Label> = Rubick::erase(Title("draft"));
    /// label.replace(Title("final"));
    ///
    /// assert_eq!(label.text(), "final");
    /// ```
    #[inline]
    pub fn replace<U: 'static>(&mut self, value: U)
    where
        T: ErasedFrom<U>,
    {
        self.replace_parts(value, ErasedTable::<U, T, WORDS>::REF);
    }

    /// Returns `true` when the concrete storage is embedded in this owner.
    ///
    /// Inline does not necessarily mean stack allocated. For example, an inline
    /// `Rubick` in a `Vec` is embedded in the vector's allocation and still
    /// requires no additional allocation for its value.
    #[inline]
    pub fn is_inline(&self) -> bool {
        self.operations.inline
    }

    /// Returns `true` when the concrete storage uses a separate heap block.
    ///
    /// This is always the inverse of [`Rubick::is_inline`].
    #[inline]
    pub fn is_heap(&self) -> bool {
        !self.operations.inline
    }

    /// Returns `true` when borrowing this owner needs no adapter call.
    ///
    /// Owners built with [`Rubick::new`] or [`Rubick::erase`] are direct.
    /// Owners built with [`Rubick::new_projected`] are not.
    #[inline]
    pub fn is_direct(&self) -> bool {
        self.operations.projection.is_direct()
    }

    #[inline]
    fn from_parts<U: 'static>(value: U, operations: &'static Operations<T>) -> Self {
        const {
            assert!(
                WORDS >= 1,
                "a Rubick needs at least one word to hold its heap pointer"
            );
        }

        let mut owner = Self {
            storage: InlineStorage::uninit(),
            operations,
            not_send_or_sync: PhantomData,
        };
        let destination = if operations.inline {
            owner.storage.as_mut_ptr()
        } else {
            let block = pool::allocate(operations.heap_class, operations.layout);
            owner.storage.set_pointer(block);
            block.as_ptr()
        };
        // SAFETY: `operations` was built for `U`, so it selected a destination
        // with `U`'s size and alignment. The destination is uninitialized and
        // becomes initialized exactly once by this write.
        unsafe { destination.cast::<U>().write(value) };
        owner
    }

    fn replace_parts<U: 'static>(&mut self, value: U, operations: &'static Operations<T>) {
        let previous = self.operations;
        let data = self.data_mut();
        // A vacant table keeps the owner droppable while no value is stored,
        // so a panicking destructor cannot lead to a second drop.
        self.operations = Operations::VACANT_REF;
        // SAFETY: `previous` describes the value currently stored at `data`.
        unsafe { (previous.drop_in_place)(data) };

        let reusable = !previous.inline
            && pool::reuses(
                previous.heap_class,
                operations.heap_class,
                previous.layout.size() == operations.layout.size()
                    && previous.layout.align() == operations.layout.align(),
            );
        let destination = if operations.inline {
            if !previous.inline {
                // SAFETY: The block came from the pool with `previous`'s class
                // and layout, and its value has just been destroyed.
                unsafe {
                    pool::deallocate(
                        NonNull::new_unchecked(data),
                        previous.heap_class,
                        previous.layout,
                    )
                };
            }
            self.storage.as_mut_ptr()
        } else if reusable {
            data
        } else {
            if !previous.inline {
                // SAFETY: As above; the old block is no longer referenced.
                unsafe {
                    pool::deallocate(
                        NonNull::new_unchecked(data),
                        previous.heap_class,
                        previous.layout,
                    )
                };
            }
            let block = pool::allocate(operations.heap_class, operations.layout);
            self.storage.set_pointer(block);
            block.as_ptr()
        };
        // SAFETY: The destination has `U`'s size and alignment and currently
        // holds no value.
        unsafe { destination.cast::<U>().write(value) };
        self.operations = operations;
    }

    #[inline]
    fn data(&self) -> *const u8 {
        if self.operations.inline {
            self.storage.as_ptr()
        } else {
            // SAFETY: Heap mode always initializes the pointer word.
            unsafe { self.storage.pointer() }
        }
    }

    #[inline]
    fn data_mut(&mut self) -> *mut u8 {
        if self.operations.inline {
            self.storage.as_mut_ptr()
        } else {
            // SAFETY: Heap mode always initializes the pointer word.
            unsafe { self.storage.pointer() }
        }
    }
}

/// Borrows the owned value through its sized or projected target type.
impl<T: ?Sized + 'static, const WORDS: usize> Deref for Rubick<T, WORDS> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        let data = self.data();
        // SAFETY: The projection was installed for the concrete initialized
        // value in this storage, and the returned borrow is bounded by `self`.
        unsafe { &*self.operations.projection.shared(data) }
    }
}

/// Mutably borrows the owned value through its sized or projected target type.
impl<T: ?Sized + 'static, const WORDS: usize> DerefMut for Rubick<T, WORDS> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        let operations = self.operations;
        let data = self.data_mut();
        // SAFETY: Exclusive access to the owner gives exclusive access to its
        // concrete value, and the matching projection preserves that lifetime.
        unsafe { &mut *operations.projection.exclusive(data) }
    }
}

/// Returns a shared borrow of the owned target.
impl<T: ?Sized + 'static, const WORDS: usize> AsRef<T> for Rubick<T, WORDS> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

/// Returns an exclusive borrow of the owned target.
impl<T: ?Sized + 'static, const WORDS: usize> AsMut<T> for Rubick<T, WORDS> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

/// Drops the concrete value exactly once and returns heap storage to the pool.
impl<T: ?Sized + 'static, const WORDS: usize> Drop for Rubick<T, WORDS> {
    #[inline]
    fn drop(&mut self) {
        let operations = self.operations;
        let data = self.data_mut();
        // SAFETY: The table matches the single initialized concrete value, and
        // an owner is dropped once, so the value is destroyed once.
        unsafe { (operations.drop_in_place)(data) };
        if !operations.inline {
            // SAFETY: The block came from the pool with this exact class and
            // layout, and its value has just been destroyed.
            unsafe {
                pool::deallocate(
                    NonNull::new_unchecked(data),
                    operations.heap_class,
                    operations.layout,
                )
            };
        }
    }
}
