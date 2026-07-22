#![doc = include_str!("../README.md")]

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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::mem::{align_of, size_of};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    use super::{INLINE_ALIGNMENT, INLINE_CAPACITY, Rubick};

    trait Value {
        fn value(&self) -> usize;
        fn set_value(&mut self, value: usize);
    }

    #[derive(Debug)]
    struct ExactBoundary([u8; INLINE_CAPACITY]);

    impl Value for ExactBoundary {
        fn value(&self) -> usize {
            usize::from(self.0[0])
        }

        fn set_value(&mut self, value: usize) {
            self.0[0] = value as u8;
        }
    }

    struct Oversized([u8; INLINE_CAPACITY + 1]);

    impl Value for Oversized {
        fn value(&self) -> usize {
            usize::from(self.0[0])
        }

        fn set_value(&mut self, value: usize) {
            self.0[0] = value as u8;
        }
    }

    #[repr(align(32))]
    struct OverAligned(u8);

    #[repr(C, align(16))]
    struct MaximallyAligned([u8; INLINE_CAPACITY]);

    #[repr(align(32))]
    struct OverAlignedZeroSized;

    struct Envelope {
        prefix: usize,
        value: usize,
        suffix: usize,
    }

    fn project_envelope_value(value: &Envelope) -> &usize {
        assert_eq!(value.prefix, 0xA1);
        assert_eq!(value.suffix, 0xB2);
        &value.value
    }

    fn project_envelope_value_mut(value: &mut Envelope) -> &mut usize {
        assert_eq!(value.prefix, 0xA1);
        assert_eq!(value.suffix, 0xB2);
        &mut value.value
    }

    fn project_value<U: Value + 'static>(value: &U) -> &(dyn Value + 'static) {
        value
    }

    fn project_value_mut<U: Value + 'static>(value: &mut U) -> &mut (dyn Value + 'static) {
        value
    }

    #[test]
    fn fixed_layout_matches_representative_aimer_values() {
        assert_eq!(INLINE_CAPACITY, 4 * size_of::<usize>());
        assert_eq!(INLINE_ALIGNMENT, 16);
        assert!(INLINE_CAPACITY / size_of::<usize>() < 8);
    }

    #[test]
    fn sized_values_dereference_and_mutate() {
        let mut value = Rubick::new(String::from("Aimer"));
        value.push_str(" GUI");

        assert_eq!(&*value, "Aimer GUI");
        assert_eq!(value.as_ref(), "Aimer GUI");
    }

    #[test]
    fn storage_selection_checks_size_alignment_and_zero_sized_values() {
        assert!(Rubick::new(()).is_inline());
        assert!(Rubick::new(ExactBoundary([0; INLINE_CAPACITY])).is_inline());
        assert!(Rubick::new(Oversized([0; INLINE_CAPACITY + 1])).is_heap());
        let over_aligned = Rubick::new(OverAligned(7));
        assert!(over_aligned.is_heap());
        assert_eq!(over_aligned.0, 7);
        assert!(align_of::<OverAligned>() > INLINE_ALIGNMENT);
    }

    #[test]
    fn storage_selection_handles_both_alignment_boundaries() {
        let maximally_aligned = Rubick::new(MaximallyAligned([7; INLINE_CAPACITY]));
        let over_aligned_zero_sized = Rubick::new(OverAlignedZeroSized);

        assert_eq!(align_of::<MaximallyAligned>(), INLINE_ALIGNMENT);
        assert_eq!(maximally_aligned.0[INLINE_CAPACITY - 1], 7);
        assert!(maximally_aligned.is_inline());

        assert_eq!(size_of::<OverAlignedZeroSized>(), 0);
        assert!(align_of::<OverAlignedZeroSized>() > INLINE_ALIGNMENT);
        assert!(over_aligned_zero_sized.is_heap());
    }

    #[test]
    fn projected_trait_values_dispatch_inline_and_on_heap() {
        let mut inline: Rubick<dyn Value> = Rubick::new_projected(
            ExactBoundary([3; INLINE_CAPACITY]),
            project_value,
            project_value_mut,
        );
        let mut heap: Rubick<dyn Value> = Rubick::new_projected(
            Oversized([5; INLINE_CAPACITY + 1]),
            project_value,
            project_value_mut,
        );

        inline.set_value(11);
        heap.set_value(13);

        assert!(inline.is_inline());
        assert!(heap.is_heap());
        assert_eq!(inline.value(), 11);
        assert_eq!(heap.value(), 13);
    }

    #[test]
    fn moves_swaps_and_collection_growth_rebuild_projection() {
        let first: Rubick<dyn Value> = Rubick::new_projected(
            ExactBoundary([1; INLINE_CAPACITY]),
            project_value,
            project_value_mut,
        );
        let second: Rubick<dyn Value> = Rubick::new_projected(
            Oversized([2; INLINE_CAPACITY + 1]),
            project_value,
            project_value_mut,
        );
        let mut values = Vec::with_capacity(1);
        values.extend([first, second]);
        values.swap(0, 1);

        assert_eq!(values[0].value(), 2);
        assert_eq!(values[1].value(), 1);
    }

    #[test]
    fn repeated_relocation_preserves_a_projection_to_an_interior_field() {
        let owner: Rubick<usize> = Rubick::new_projected(
            Envelope {
                prefix: 0xA1,
                value: 7,
                suffix: 0xB2,
            },
            project_envelope_value,
            project_envelope_value_mut,
        );
        let mut owners = Vec::with_capacity(1);
        owners.push(owner);

        for expected in 8..=256 {
            owners.push(Rubick::new(expected));
            let last = owners.len() - 1;
            owners.swap(0, last);
            owners.swap(0, last);
            *owners[0] += 1;
            assert_eq!(*owners[0], expected);
        }
    }

    #[test]
    fn captured_projection_adapters_keep_state_across_owner_moves() {
        let shared_calls = Rc::new(Cell::new(0));
        let mutable_calls = Rc::new(Cell::new(0));
        let mut owner: Rubick<usize> = Rubick::new_projected(
            10_usize,
            {
                let shared_calls = Rc::clone(&shared_calls);
                move |value: &usize| {
                    shared_calls.set(shared_calls.get() + 1);
                    value
                }
            },
            {
                let mutable_calls = Rc::clone(&mutable_calls);
                move |value: &mut usize| {
                    mutable_calls.set(mutable_calls.get() + 1);
                    value
                }
            },
        );

        owner = std::hint::black_box(owner);
        assert_eq!(*owner, 10);
        *owner = 12;
        assert_eq!(*owner, 12);
        assert_eq!(shared_calls.get(), 2);
        assert_eq!(mutable_calls.get(), 1);
    }

    #[test]
    fn panicking_projection_does_not_corrupt_the_owned_value() {
        let should_panic = Rc::new(Cell::new(true));
        let mut owner: Rubick<usize> = Rubick::new_projected(
            41_usize,
            {
                let should_panic = Rc::clone(&should_panic);
                move |value: &usize| {
                    assert!(!should_panic.replace(false), "project once with a panic");
                    value
                }
            },
            |value: &mut usize| value,
        );

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = *owner;
        }));
        assert!(result.is_err());

        *owner += 1;
        assert_eq!(*owner, 42);
    }

    struct DropValue<const N: usize> {
        drops: Rc<Cell<usize>>,
        bytes: [u8; N],
    }

    impl<const N: usize> Drop for DropValue<N> {
        fn drop(&mut self) {
            self.drops
                .set(self.drops.get() + 1);
        }
    }

    trait DropTarget {}

    impl<const N: usize> DropTarget for DropValue<N> {}

    fn project_drop_target<U: DropTarget + 'static>(value: &U) -> &(dyn DropTarget + 'static) {
        value
    }

    fn project_drop_target_mut<U: DropTarget + 'static>(
        value: &mut U,
    ) -> &mut (dyn DropTarget + 'static) {
        value
    }

    #[test]
    fn values_drop_exactly_once_in_both_modes() {
        let inline_drops = Rc::new(Cell::new(0));
        let heap_drops = Rc::new(Cell::new(0));
        {
            let _inline = Rubick::new(DropValue::<0> {
                drops: Rc::clone(&inline_drops),
                bytes: [],
            });
            let _heap = Rubick::new(DropValue::<INLINE_CAPACITY> {
                drops: Rc::clone(&heap_drops),
                bytes: [0; INLINE_CAPACITY],
            });
        }

        assert_eq!(inline_drops.get(), 1);
        assert_eq!(heap_drops.get(), 1);
    }

    #[test]
    fn replacing_an_owner_drops_each_value_once() {
        let first_drops = Rc::new(Cell::new(0));
        let second_drops = Rc::new(Cell::new(0));
        let mut owner = Rubick::new(DropValue::<0> {
            drops: Rc::clone(&first_drops),
            bytes: [],
        });

        let old = std::mem::replace(
            &mut owner,
            Rubick::new(DropValue::<0> {
                drops: Rc::clone(&second_drops),
                bytes: [],
            }),
        );
        drop(old);
        assert_eq!(first_drops.get(), 1);
        assert_eq!(second_drops.get(), 0);

        drop(owner);
        assert_eq!(second_drops.get(), 1);
    }

    #[test]
    fn values_drop_during_panic_unwinding() {
        let inline_drops = Rc::new(Cell::new(0));
        let heap_drops = Rc::new(Cell::new(0));
        let result = catch_unwind(AssertUnwindSafe({
            let inline_drops = Rc::clone(&inline_drops);
            let heap_drops = Rc::clone(&heap_drops);
            move || {
                let _inline = Rubick::new(DropValue::<0> {
                    drops: inline_drops,
                    bytes: [],
                });
                let _heap = Rubick::new(DropValue::<INLINE_CAPACITY> {
                    drops: heap_drops,
                    bytes: [0; INLINE_CAPACITY],
                });
                panic!("exercise unwind drop");
            }
        }));

        assert!(result.is_err());
        assert_eq!(inline_drops.get(), 1);
        assert_eq!(heap_drops.get(), 1);
    }

    #[test]
    fn nested_and_mixed_mode_owners_drop_every_value_once() {
        const OWNER_COUNT: usize = 128;

        let drops = Rc::new(Cell::new(0));
        let mut owners = Vec::with_capacity(1);
        for index in 0..OWNER_COUNT {
            let owner: Rubick<dyn DropTarget> = if index % 2 == 0 {
                Rubick::new_projected(
                    DropValue::<0> {
                        drops: Rc::clone(&drops),
                        bytes: [],
                    },
                    project_drop_target,
                    project_drop_target_mut,
                )
            } else {
                Rubick::new_projected(
                    DropValue::<INLINE_CAPACITY> {
                        drops: Rc::clone(&drops),
                        bytes: [0; INLINE_CAPACITY],
                    },
                    project_drop_target,
                    project_drop_target_mut,
                )
            };
            if index % 2 == 0 {
                assert!(owner.is_inline());
            } else {
                assert!(owner.is_heap());
            }
            owners.push(owner);
        }
        owners.reverse();
        owners.rotate_left(37);
        drop(owners);
        assert_eq!(drops.get(), OWNER_COUNT);

        let nested_drops = Rc::new(Cell::new(0));
        let inner = Rubick::new(DropValue::<0> {
            drops: Rc::clone(&nested_drops),
            bytes: [],
        });
        let outer = Rubick::new(inner);
        assert!(outer.is_heap());
        drop(outer);
        assert_eq!(nested_drops.get(), 1);
    }

    #[test]
    fn owner_layout_and_unpin_contract_are_fixed() {
        fn assert_unpin<T: Unpin>() {}

        assert_unpin::<Rubick<u32>>();
        assert!(size_of::<Rubick<u32>>() >= INLINE_CAPACITY);
        assert!(align_of::<Rubick<u32>>() >= INLINE_ALIGNMENT);
    }

    #[test]
    fn owners_are_conservatively_not_send_or_sync() {
        trait AmbiguousIfSend<A> {
            fn check() {}
        }

        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

        trait AmbiguousIfSync<A> {
            fn check() {}
        }

        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

        <Rubick<u32> as AmbiguousIfSend<_>>::check();
        <Rubick<u32> as AmbiguousIfSync<_>>::check();
    }

    #[test]
    fn test_fixture_uses_heap_payload_bytes() {
        let value = DropValue::<3> {
            drops: Rc::new(Cell::new(0)),
            bytes: [1, 2, 3],
        };
        assert_eq!(value.bytes.len(), 3);
    }
}
