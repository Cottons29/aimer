//! Single-threaded shared ownership with read-only field projection.
//!
//! [`Shared`] owns one heap allocation containing strong and weak-reference
//! counters. Sized values live in that allocation; dynamically sized values
//! can be retained through an owned `Box` or borrowed from a static reference.
//! Inside a [`UiAllocator::scope`], the shared allocation comes from the active
//! application UI heap. Otherwise, it uses the same thread-local size-class
//! pool as heap-mode [`Rubick`] values.

use std::alloc::{Layout, handle_alloc_error};
use std::borrow::Borrow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::str::FromStr;
use allocator_api2::alloc::Allocator;

use crate::erase::ErasedFrom;
use crate::pool;
use crate::{Rubick, UiAllocator};

struct SharedAllocation<T: ?Sized> {
    strong: Cell<usize>,
    // Includes one implicit weak reference while the value is alive.
    weak: Cell<usize>,
    origin: AllocationOrigin,
    layout: Layout,
    value: SharedValueStorage<T>,
}

enum SharedValueStorage<T: ?Sized> {
    Inline(NonNull<T>),
    Boxed(ManuallyDrop<Box<T>>),
    // Points to an immutable static value, which this allocation never drops.
    Static(NonNull<T>),
}

enum AllocationOrigin {
    ThreadLocal(u8),
    Ui(UiAllocator),
}

impl AllocationOrigin {
    fn ui_allocator(&self) -> Option<UiAllocator> {
        match self {
            Self::ThreadLocal(_) => None,
            Self::Ui(allocator) => Some(allocator.clone()),
        }
    }
}

/// Releases the implicit weak reference even when dropping the value unwinds.
struct ReleaseImplicitWeak<T: ?Sized> {
    allocation: NonNull<SharedAllocation<T>>,
}

impl<T: ?Sized> Drop for ReleaseImplicitWeak<T> {
    fn drop(&mut self) {
        let weak = unsafe { self.allocation.as_ref() }.weak.get();
        debug_assert!(weak > 0, "the implicit weak reference must be alive");

        if weak == 1 {
            // SAFETY: the implicit weak reference is the final reference to the
            // allocation, and the owned value has already been dropped or moved
            // out. A static value has no owned payload to destroy.
            unsafe { deallocate_allocation(self.allocation) };
        } else {
            unsafe { self.allocation.as_ref() }.weak.set(weak - 1);
        }
    }
}

/// Deallocates an allocation after its owned value has been destroyed.
unsafe fn deallocate_allocation<T: ?Sized>(allocation: NonNull<SharedAllocation<T>>) {
    let layout = unsafe { allocation.as_ref() }.layout;
    // SAFETY: the caller guarantees that no strong or weak reference can access
    // the allocation again and that any owned value has already been destroyed.
    let origin = unsafe { ptr::addr_of!((*allocation.as_ptr()).origin).read() };
    match origin {
        AllocationOrigin::ThreadLocal(class) => {
            // SAFETY: this allocation came from the thread-local Rubick pool
            // with this exact class and layout.
            unsafe { pool::deallocate(allocation.cast(), class, layout) };
        }
        AllocationOrigin::Ui(allocator) => {
            // SAFETY: this allocation came from this UI heap with the exact
            // layout, and the stored handle kept that heap alive.
            unsafe { allocator.deallocate(allocation.cast(), layout) };
        }
    }
}

fn allocate_shared_block<T: ?Sized>(
    layout: Layout,
    allocator: Option<UiAllocator>,
) -> (NonNull<SharedAllocation<T>>, AllocationOrigin) {
    match allocator {
        Some(allocator) => {
            let allocation = match allocator.allocate(layout) {
                Ok(allocation) => allocation.cast(),
                Err(_) => handle_alloc_error(layout),
            };
            (allocation, AllocationOrigin::Ui(allocator))
        }
        None => {
            let class = pool::class_of(layout);
            (
                pool::allocate(class, layout).cast(),
                AllocationOrigin::ThreadLocal(class),
            )
        }
    }
}

#[inline]
fn increment_count(count: usize) -> usize {
    if count == usize::MAX {
        // Wrapping would eventually free an allocation with live handles.
        std::process::abort();
    }
    count + 1
}

/// A single-threaded, reference-counted pointer.
///
/// `Shared<T>` stores sized `T` values inline with its strong-reference count.
/// Dynamically sized values can be retained with [`from_box`](Self::from_box),
/// and statically lived values can be borrowed with
/// [`from_static`](Self::from_static). Cloning a `Shared` increments the count
/// without cloning the value. Owned values are destroyed when the final strong
/// handle is dropped; static values are never destroyed by `Shared`. A [`Weak`]
/// handle keeps only the shared allocation alive.
///
/// The value is read-only while shared. Mutable access is available through
/// [`get_mut`](Self::get_mut) when there is exactly one strong owner and no
/// [`Weak`] handles, or through [`make_mut`](Self::make_mut) using copy-on-write.
/// The `Shared` allocation uses the active UI heap when one is scoped; heap
/// allocations owned internally by `T` continue to use `T`'s own allocator.
///
/// `Shared` is deliberately neither [`Send`] nor [`Sync`]. Cycles made entirely
/// from strong handles still leak; use [`Weak`] for non-owning back references.
///
/// # Examples
///
/// ```
/// use aimer_rubick::shared::Shared;
///
/// let first = Shared::new(String::from("pineapple"));
/// let second = first.clone();
///
/// assert_eq!(&*second, "pineapple");
/// assert_eq!(Shared::strong_count(&first), 2);
/// ```
pub struct Shared<T: ?Sized> {
    allocation: NonNull<SharedAllocation<T>>,
    // Makes the single-threaded contract explicit in the auto traits while
    // preserving covariance over `T`.
    not_thread_safe: PhantomData<*const Cell<()>>,
}




/// A non-owning handle to a [`Shared`] allocation.
///
/// A `Weak` handle does not keep the value alive. Use [`upgrade`](Self::upgrade)
/// to attempt to create a temporary strong [`Shared`] handle.
///
/// # Examples
///
/// ```
/// use aimer_rubick::shared::Shared;
///
/// let value = Shared::new(String::from("pineapple"));
/// let weak = Shared::downgrade(&value);
/// drop(value);
///
/// assert!(weak.upgrade().is_none());
/// ```
pub struct Weak<T: ?Sized> {
    allocation: NonNull<SharedAllocation<T>>,
    not_thread_safe: PhantomData<*const Cell<()>>,
}

impl<T> Shared<T> {
    /// Allocates `value` with an initial strong-reference count of one.
    ///
    /// Inside a [`UiAllocator::scope`], the active application UI heap owns
    /// this allocation. Otherwise it uses the same thread-local size-class
    /// pool as heap-mode [`Rubick`] values.
    ///
    /// This controls the allocation for `Shared` itself. Heap-owning fields of
    /// `T`, such as a `String`, keep using their own allocator.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self::new_with_allocator(value, UiAllocator::current())
    }

    /// Allocates `value` from the supplied application UI heap.
    ///
    /// The allocation retains `allocator` until its final strong and weak
    /// handles are dropped, so the returned handle may outlive both the
    /// allocator scope and the [`UiMemory`](crate::UiMemory) value.
    #[must_use]
    pub fn new_in(value: T, allocator: &UiAllocator) -> Self {
        Self::new_with_allocator(value, Some(allocator.clone()))
    }

    fn new_with_allocator(value: T, allocator: Option<UiAllocator>) -> Self {
        let header_layout = Layout::new::<SharedAllocation<T>>();
        let (layout, value_offset) = header_layout
            .extend(Layout::new::<T>())
            .unwrap_or_else(|_| handle_alloc_error(Layout::new::<T>()));
        let layout = layout.pad_to_align();
        let (allocation, origin) = allocate_shared_block(layout, allocator);
        // SAFETY: the allocation includes the extended layout, and `value_offset`
        // aligns the trailing value for `T`.
        let value_ptr = unsafe {
            allocation
                .cast::<u8>()
                .as_ptr()
                .add(value_offset)
                .cast::<T>()
        };

        // SAFETY: `value_ptr` points to aligned, uninitialized storage for one
        // `T` inside the allocation.
        unsafe {
            value_ptr.write(value);
            allocation.as_ptr().write(SharedAllocation {
                strong: Cell::new(1),
                weak: Cell::new(1),
                origin,
                layout,
                value: SharedValueStorage::Inline(NonNull::new_unchecked(value_ptr)),
            });
        }

        Self {
            allocation,
            not_thread_safe: PhantomData,
        }
    }

    /// Returns mutable access, cloning into a new allocation when another
    /// strong or weak handle exists.
    #[inline]
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        if Self::strong_count(this) != 1
            || Self::weak_count(this) != 0
            || matches!(&this.inner().value, SharedValueStorage::Static(_))
        {
            let value = (**this).clone();
            let allocator = UiAllocator::current().or_else(|| this.ui_allocator());
            *this = Self::new_with_allocator(value, allocator);
        }

        Self::get_mut(this).expect("a newly allocated Shared has one owner")
    }

    /// Moves an owned value out when `this` is its only owner.
    ///
    /// If another strong handle exists, ownership of `this` is returned in
    /// `Err`. Existing weak handles remain valid but cannot be upgraded. A
    /// static-backed value is also returned in `Err`, because `Shared` cannot
    /// move a value out of its static storage.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        if Self::strong_count(&this) != 1
            || matches!(&this.inner().value, SharedValueStorage::Static(_))
        {
            return Err(this);
        }

        let this = ManuallyDrop::new(this);
        let allocation = this.allocation;
        unsafe { allocation.as_ref() }.strong.set(0);
        let _release_weak = ReleaseImplicitWeak { allocation };

        // SAFETY: the strong count is zero and `this` cannot run `Drop`, so the
        // value is uniquely owned and cannot be observed or dropped again.
        let value = unsafe {
            match &mut (*allocation.as_ptr()).value {
                SharedValueStorage::Inline(value) => value.as_ptr().read(),
                SharedValueStorage::Boxed(value) => *ManuallyDrop::take(value),
                SharedValueStorage::Static(_) => unreachable!("static values cannot be unwrapped"),
            }
        };
        Ok(value)
    }

    /// Returns the value, cloning it only when other owners exist.
    pub fn unwrap_or_clone(this: Self) -> T
    where
        T: Clone,
    {
        Self::try_unwrap(this).unwrap_or_else(|shared| (*shared).clone())
    }

}

impl<T: ?Sized> Shared<T> {
    /// Retains a statically lived value without copying or dropping it.
    ///
    /// A shared control block is still allocated so strong and weak handles
    /// keep their usual count semantics. `get_mut` returns `None` for this
    /// value; [`make_mut`](Self::make_mut) clones it into owned storage, and
    /// [`try_unwrap`](Self::try_unwrap) returns the handle unchanged.
    ///
    /// Inside a [`UiAllocator::scope`], the control block uses the active UI
    /// heap. Otherwise, it uses the thread-local Rubick pool.
    #[must_use]
    pub fn from_static(value: &'static T) -> Self
    where
        T: 'static,
    {
        Self::from_static_with_allocator(value, UiAllocator::current())
    }

    /// Retains a static value with its control block allocated from `allocator`.
    #[must_use]
    pub fn from_static_in(value: &'static T, allocator: &UiAllocator) -> Self
    where
        T: 'static,
    {
        Self::from_static_with_allocator(value, Some(allocator.clone()))
    }

    fn from_static_with_allocator(
        value: &'static T,
        allocator: Option<UiAllocator>,
    ) -> Self
    where
        T: 'static,
    {
        let layout = Layout::new::<SharedAllocation<T>>();
        let (allocation, origin) = allocate_shared_block(layout, allocator);

        // SAFETY: `allocation` is suitably sized and aligned for the control
        // block. The stored pointer comes from `&'static T` and is never used
        // for mutation or dropped as an owned value.
        unsafe {
            allocation.as_ptr().write(SharedAllocation {
                strong: Cell::new(1),
                weak: Cell::new(1),
                origin,
                layout,
                value: SharedValueStorage::Static(NonNull::from(value)),
            });
        }

        Self {
            allocation,
            not_thread_safe: PhantomData,
        }
    }

    /// Retains a value already owned by a `Box`, including dynamically sized
    /// values such as `str` and slices.
    ///
    /// The shared control block uses the active UI heap when one is scoped. The
    /// boxed value keeps its original allocation and allocator.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_rubick::Shared;
    ///
    /// let first: Shared<str> =
    ///     Shared::from_box(String::from("pineapple").into_boxed_str());
    /// let second = first.clone();
    ///
    /// assert_eq!(first, second);
    /// ```
    #[must_use]
    pub fn from_box(value: Box<T>) -> Self {
        Self::from_box_with_allocator(value, UiAllocator::current())
    }

    /// Retains a value already owned by a `Box`, allocating the shared
    /// control block from `allocator`.
    #[must_use]
    pub fn from_box_in(value: Box<T>, allocator: &UiAllocator) -> Self {
        Self::from_box_with_allocator(value, Some(allocator.clone()))
    }

    fn from_box_with_allocator(value: Box<T>, allocator: Option<UiAllocator>) -> Self {
        let layout = Layout::new::<SharedAllocation<T>>();
        let (allocation, origin) = allocate_shared_block(layout, allocator);

        // SAFETY: `allocation` is suitably sized and aligned uninitialized
        // storage returned for exactly `layout` by the recorded allocator.
        unsafe {
            allocation.as_ptr().write(SharedAllocation {
                strong: Cell::new(1),
                weak: Cell::new(1),
                origin,
                layout,
                value: SharedValueStorage::Boxed(ManuallyDrop::new(value)),
            });
        }

        Self {
            allocation,
            not_thread_safe: PhantomData,
        }
    }

    /// Returns the number of strong handles to this allocation.
    #[must_use]
    #[inline]
    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.get()
    }

    /// Returns the number of explicit weak handles to this allocation.
    #[must_use]
    #[inline]
    pub fn weak_count(this: &Self) -> usize {
        this.inner().weak.get() - 1
    }

    /// Creates a non-owning handle to this allocation.
    #[must_use]
    #[inline]
    pub fn downgrade(this: &Self) -> Weak<T> {
        let weak = increment_count(this.inner().weak.get());
        this.inner().weak.set(weak);

        Weak {
            allocation: this.allocation,
            not_thread_safe: PhantomData,
        }
    }

    /// Returns `true` when both handles point to the same allocation.
    #[must_use]
    #[inline]
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.allocation == other.allocation
    }

    /// Returns mutable access when `this` is the only strong owner, no weak
    /// handles can later be upgraded, and the value is owned by `Shared`.
    #[inline]
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if Self::strong_count(this) == 1
            && Self::weak_count(this) == 0
            && !matches!(&this.inner().value, SharedValueStorage::Static(_))
        {
            // SAFETY: `&mut Self` prevents use through this handle, one strong
            // owner proves that no other `Shared` or projection exists, and no
            // weak handle can be upgraded concurrently on this thread.
            let value = unsafe { this.value_mut_ptr() };
            Some(unsafe { &mut *value })
        } else {
            None
        }
    }

    /// Creates a read-only handle to a borrowed part of `T`.
    ///
    /// The returned [`ShareRef`] owns a strong reference to the complete value,
    /// so it can outlive this root handle. Its selector is evaluated on every
    /// access; no raw interior field pointer is stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_rubick::Shared;
    ///
    /// struct State { title: String }
    ///
    /// let state = Shared::new(State { title: "Aimer".into() });
    /// let title = state.project(|state: &State| &state.title);
    /// drop(state);
    ///
    /// assert_eq!(&*title, "Aimer");
    /// ```
    #[inline]
    pub fn project<Field, Select>(&self, select: Select) -> ShareRef<Field>
    where
        T: 'static,
        Field: ?Sized + 'static,
        Select: for<'a> Fn(&'a T) -> &'a Field + 'static,
    {
        ShareRef::<Field>::from_projection(self.clone(), select)
    }

    #[inline]
    fn inner(&self) -> &SharedAllocation<T> {
        // SAFETY: every live `Shared` owns one strong reference, so its
        // allocation remains initialized for the duration of this borrow.
        unsafe { self.allocation.as_ref() }
    }

    #[inline]
    fn ui_allocator(&self) -> Option<UiAllocator> {
        self.inner().origin.ui_allocator()
    }

    #[inline]
    fn value_ptr(&self) -> *const T {
        match &self.inner().value {
            SharedValueStorage::Inline(value) => value.as_ptr(),
            SharedValueStorage::Boxed(value) => &***value,
            SharedValueStorage::Static(value) => value.as_ptr(),
        }
    }

    /// Gets a mutable pointer when the caller has proved unique ownership.
    #[inline]
    unsafe fn value_mut_ptr(&mut self) -> *mut T {
        // SAFETY: the caller guarantees this is the only strong owner, that no
        // weak handle can upgrade while this mutable access is in progress, and
        // that the stored value is not borrowed from static storage.
        let allocation = unsafe { self.allocation.as_mut() };
        match &mut allocation.value {
            SharedValueStorage::Inline(value) => value.as_ptr(),
            SharedValueStorage::Boxed(value) => &mut ***value,
            SharedValueStorage::Static(_) => unreachable!("static values cannot be mutably borrowed"),
        }
    }
}

impl<T: ?Sized> Weak<T> {
    /// Returns the number of live strong handles for this allocation.
    #[must_use]
    #[inline]
    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.get()
    }

    /// Returns the number of explicit weak handles for this allocation.
    #[must_use]
    #[inline]
    pub fn weak_count(this: &Self) -> usize {
        let allocation = this.inner();
        let implicit = if allocation.strong.get() == 0 { 0 } else { 1 };
        allocation.weak.get() - implicit
    }

    /// Returns `true` when both handles point to the same allocation.
    #[must_use]
    #[inline]
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.allocation == other.allocation
    }

    /// Attempts to create a strong handle while the value is still alive.
    #[must_use]
    #[inline]
    pub fn upgrade(&self) -> Option<Shared<T>> {
        let strong = self.inner().strong.get();
        if strong == 0 {
            return None;
        }

        self.inner().strong.set(increment_count(strong));
        Some(Shared {
            allocation: self.allocation,
            not_thread_safe: PhantomData,
        })
    }

    #[inline]
    fn inner(&self) -> &SharedAllocation<T> {
        // SAFETY: a live `Weak` keeps the allocation alive even after the value
        // has been dropped.
        unsafe { self.allocation.as_ref() }
    }
}

impl<T: ?Sized> Clone for Weak<T> {
    #[inline]
    fn clone(&self) -> Self {
        let weak = increment_count(self.inner().weak.get());
        self.inner().weak.set(weak);

        Self {
            allocation: self.allocation,
            not_thread_safe: PhantomData,
        }
    }
}

impl<T: ?Sized> Drop for Weak<T> {
    fn drop(&mut self) {
        let weak = self.inner().weak.get();
        debug_assert!(weak > 0, "a live Weak must own a weak reference");

        if weak == 1 {
            debug_assert_eq!(
                self.inner().strong.get(),
                0,
                "the implicit weak reference must remain while strong handles exist"
            );
            // SAFETY: this is the final weak reference and the value has already
            // been dropped.
            unsafe { deallocate_allocation(self.allocation) };
        } else {
            self.inner().weak.set(weak - 1);
        }
    }
}

impl<T: ?Sized> Clone for Shared<T> {
    #[inline]
    fn clone(&self) -> Self {
        let strong = increment_count(Self::strong_count(self));
        self.inner().strong.set(strong);

        Self {
            allocation: self.allocation,
            not_thread_safe: PhantomData,
        }
    }
}



impl<T: ?Sized> Drop for Shared<T> {
    fn drop(&mut self) {
        let count = Self::strong_count(self);
        debug_assert!(count > 0, "a live Shared must own a strong reference");

        if count > 1 {
            self.inner().strong.set(count - 1);
            return;
        }

        self.inner().strong.set(0);
        let _release_weak = ReleaseImplicitWeak {
            allocation: self.allocation,
        };

        // SAFETY: this is the final strong handle. Setting the strong count to
        // zero prevents a weak handle from being upgraded during `T::drop`.
        // The guard releases the implicit weak reference after the value is
        // dropped, including if `T::drop` unwinds.
        unsafe {
            match &mut (*self.allocation.as_ptr()).value {
                SharedValueStorage::Inline(value) => ptr::drop_in_place(value.as_ptr()),
                SharedValueStorage::Boxed(value) => ManuallyDrop::drop(value),
                SharedValueStorage::Static(_) => {}
            }
        }
    }
}

impl<T: ?Sized> Deref for Shared<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: the allocation is live, and shared ownership only permits an
        // immutable reference here.
        unsafe { &*self.value_ptr() }
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: ?Sized> From<Box<T>> for Shared<T> {
    fn from(value: Box<T>) -> Self {
        Self::from_box(value)
    }
}

impl<T: FromStr> FromStr for Shared<T> {
    type Err = T::Err;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        T::from_str(value).map(Self::new)
    }
}

impl FromStr for Shared<str> {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_box(value.to_owned().into_boxed_str()))
    }
}

impl<T: ?Sized> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ?Sized> Borrow<T> for Shared<T> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, formatter)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, formatter)
    }
}



impl<T: ?Sized> fmt::Pointer for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&self.value_ptr(), formatter)
    }
}

impl<T, U> PartialEq<Shared<U>> for Shared<T>
where
    T: ?Sized + PartialEq<U>,
    U: ?Sized,
{
    fn eq(&self, other: &Shared<U>) -> bool {
        **self == **other
    }
}

impl<T: ?Sized + Eq> Eq for Shared<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for Shared<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: ?Sized + Ord> Ord for Shared<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: ?Sized + Hash> Hash for Shared<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}


/// An owning, read-only projection into a [`Shared`] value.
///
/// An internal projected reference stores a strong owner plus a selector. It
/// never stores a raw pointer into the selected field.
struct SharedRef<Owner: ?Sized, Field: ?Sized, Select> {
    owner: Shared<Owner>,
    select: Select,
    field: PhantomData<*const Field>,
}

impl<Owner: ?Sized, Field, Select> SharedRef<Owner, Field, Select>
where
    Field: ?Sized,
    Select: for<'a> Fn(&'a Owner) -> &'a Field,
{
    /// Borrows the selected field.
    #[must_use]
    #[inline]
    pub fn get(&self) -> &Field {
        (self.select)(&self.owner)
    }

}

trait SharedValue<T: ?Sized> {
    fn get(&self) -> &T;
}

impl<Owner: ?Sized, Field, Select> SharedValue<Field> for SharedRef<Owner, Field, Select>
where
    Owner: 'static,
    Field: ?Sized + 'static,
    Select: for<'a> Fn(&'a Owner) -> &'a Field + 'static,
{
    #[inline]
    fn get(&self) -> &Field {
        SharedRef::get(self)
    }
}

// SAFETY: The template carries the vtable for the concrete `SharedRef`, whose
// `SharedValue<Field>` implementation returns a borrow tied to that owner.
unsafe impl<Owner: ?Sized, Field, Select> ErasedFrom<SharedRef<Owner, Field, Select>>
    for dyn SharedValue<Field>
where
    Owner: 'static,
    Field: ?Sized + 'static,
    Select: for<'a> Fn(&'a Owner) -> &'a Field + 'static,
{
    const TEMPLATE: *const Self =
        std::ptr::null::<SharedRef<Owner, Field, Select>>() as *const dyn SharedValue<Field>;
}

struct ProjectedShareRef<T: ?Sized + 'static, U: ?Sized + 'static, Select> {
    source: ShareRef<T>,
    select: Select,
    target: PhantomData<*const U>,
}

impl<T, U, Select> SharedValue<U> for ProjectedShareRef<T, U, Select>
where
    T: ?Sized + 'static,
    U: ?Sized + 'static,
    Select: for<'a> Fn(&'a T) -> &'a U,
{
    #[inline]
    fn get(&self) -> &U {
        (self.select)(self.source.get())
    }
}

// SAFETY: The template carries the vtable for `ProjectedShareRef`, whose
// `SharedValue<U>` implementation returns a borrow tied to the source handle.
unsafe impl<T, U, Select> ErasedFrom<ProjectedShareRef<T, U, Select>> for dyn SharedValue<U>
where
    T: ?Sized + 'static,
    U: ?Sized + 'static,
    Select: for<'a> Fn(&'a T) -> &'a U + 'static,
{
    const TEMPLATE: *const Self =
        std::ptr::null::<ProjectedShareRef<T, U, Select>>() as *const dyn SharedValue<U>;
}

enum ShareRefStorage<T: ?Sized + 'static> {
    Static(&'static T),
    Shared(Shared<Rubick<dyn SharedValue<T>, 1>>),
}

/// A type-erased, owning read-only reference to a shared value.
///
/// `ShareRef<T>` is the stable interface for APIs that need to retain a
/// projected value without knowing the owner or selector types used to obtain
/// it. Cloning the handle clones its ownership reference; it never clones the
/// selected value. The returned borrow from [`ShareRef::get`] is tied to the
/// handle. Static-backed handles retain a static reference directly.
///
/// [`Shared::project`] creates a projected handle directly. Use
/// [`ShareRef::from_rc`] to retain a value held by an [`Rc`], or
/// [`ShareRef::from_static`] for a value with a static lifetime.
pub struct ShareRef<T: ?Sized + 'static> {
    value: ShareRefStorage<T>,
}

impl<T: ?Sized + 'static> ShareRef<T> {
    #[inline]
    fn from_projection<Owner: ?Sized, Select>(owner: Shared<Owner>, select: Select) -> Self
    where
        Owner: 'static,
        Select: for<'a> Fn(&'a Owner) -> &'a T + 'static,
    {
        let allocator = UiAllocator::current().or_else(|| owner.ui_allocator());
        let reference = SharedRef {
            owner,
            select,
            field: PhantomData,
        };
        match allocator {
            Some(allocator) => allocator.scope(|| {
                let value: Rubick<dyn SharedValue<T>, 1> = Rubick::erase(reference);
                Self {
                    value: ShareRefStorage::Shared(Shared::new_in(value, &allocator)),
                }
            }),
            None => {
                let value: Rubick<dyn SharedValue<T>, 1> = Rubick::erase(reference);
                Self {
                    value: ShareRefStorage::Shared(Shared::new(value)),
                }
            }
        }
    }

    /// Creates a handle to a value whose reference is valid for the program's
    /// lifetime.
    ///
    /// The static reference is stored directly, so construction and cloning do
    /// not allocate.
    #[must_use]
    #[inline]
    pub fn from_static(value: &'static T) -> Self {
        Self {
            value: ShareRefStorage::Static(value),
        }
    }

    /// Creates a shared handle from an existing [`Rc`].
    ///
    /// The `Rc` is retained without cloning its value. Use [`ShareRef::project`]
    /// to narrow a `ShareRef<String>` to a `ShareRef<str>` without copying the
    /// string contents.
    #[must_use]
    #[inline]
    pub fn from_rc(rc: &Rc<T>) -> Self {
        Self::from_projection(Shared::new(Rc::clone(rc)), Rc::as_ref)
    }

    /// Projects this handle to a borrowed subvalue without cloning it.
    #[must_use]
    #[inline]
    pub fn project<U, Select>(self, select: Select) -> ShareRef<U>
    where
        U: ?Sized + 'static,
        Select: for<'a> Fn(&'a T) -> &'a U + 'static,
    {
        match self.value {
            ShareRefStorage::Static(value) => ShareRef::from_static(select(value)),
            ShareRefStorage::Shared(value) => {
                let allocator = UiAllocator::current().or_else(|| value.ui_allocator());
                let source = Self {
                    value: ShareRefStorage::Shared(value),
                };
                match allocator {
                    Some(allocator) => allocator.scope(|| {
                        let projected = ProjectedShareRef {
                            source,
                            select,
                            target: PhantomData,
                        };
                        let value: Rubick<dyn SharedValue<U>, 1> = Rubick::erase(projected);
                        ShareRef {
                            value: ShareRefStorage::Shared(Shared::new_in(value, &allocator)),
                        }
                    }),
                    None => {
                        let projected = ProjectedShareRef {
                            source,
                            select,
                            target: PhantomData,
                        };
                        let value: Rubick<dyn SharedValue<U>, 1> = Rubick::erase(projected);
                        ShareRef {
                            value: ShareRefStorage::Shared(Shared::new(value)),
                        }
                    }
                }
            }
        }
    }

    /// Borrows the retained value.
    #[must_use]
    #[inline]
    pub fn get(&self) -> &T {
        match &self.value {
            ShareRefStorage::Static(value) => value,
            ShareRefStorage::Shared(value) => value.get(),
        }
    }
}

impl<T: FromStr + 'static> FromStr for ShareRef<T> {
    type Err = T::Err;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        T::from_str(value).map(|value| Shared::new(value).project(|value: &T| value))
    }
}

impl FromStr for ShareRef<str> {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let owner = Shared::<str>::from_box(value.to_owned().into_boxed_str());
        Ok(owner.project(|value: &str| value))
    }
}

impl<T: ?Sized + 'static> Clone for ShareRef<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            value: match &self.value {
                ShareRefStorage::Static(value) => ShareRefStorage::Static(*value),
                ShareRefStorage::Shared(value) => ShareRefStorage::Shared(value.clone()),
            },
        }
    }
}

impl<T: ?Sized + 'static> Deref for ShareRef<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T: ?Sized + 'static> AsRef<T> for ShareRef<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.get()
    }
}

impl<T: ?Sized + fmt::Debug + 'static> fmt::Debug for ShareRef<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.get(), formatter)
    }
}

impl<T: ?Sized + fmt::Display + 'static> fmt::Display for ShareRef<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.get(), formatter)
    }
}

impl<T, U> PartialEq<ShareRef<U>> for ShareRef<T>
where
    T: ?Sized + PartialEq<U> + 'static,
    U: ?Sized + 'static,
{
    fn eq(&self, other: &ShareRef<U>) -> bool {
        self.get() == other.get()
    }
}

impl<T: ?Sized + Eq + 'static> Eq for ShareRef<T> {}

#[cfg(test)]
mod share_ref_tests {
    use super::{ShareRef, Shared};
    use crate::UiMemory;
    use std::rc::Rc;

    const TWO_MIB: usize = 2 * 1024 * 1024;

    #[test]
    fn from_rc_borrows_the_original_string_allocation() {
        let value: Rc<str> = Rc::from("shared");
        let reference = ShareRef::from_rc(&value);

        assert!(std::ptr::eq(reference.get(), value.as_ref()));
    }

    #[test]
    fn projected_reference_borrows_the_original_field() {
        struct State {
            title: String,
        }

        let state = Shared::new(State {
            title: String::from("projected"),
        });
        let reference = state.project(|state| &state.title);

        assert!(std::ptr::eq(reference.get(), &state.title));
    }

    #[test]
    fn shared_uses_the_active_ui_memory_through_the_final_weak_drop() {
        let memory = UiMemory::new(TWO_MIB);
        let allocator = memory.allocator();
        let (value, weak) = allocator.scope(|| {
            let value = Shared::new([7_u8; 1024]);
            let weak = Shared::downgrade(&value);
            (value, weak)
        });

        assert_eq!(allocator.committed_bytes(), TWO_MIB);
        drop(allocator);
        drop(memory);

        assert_eq!(value[0], 7);
        drop(value);
        assert!(weak.upgrade().is_none());
        drop(weak);
    }

    #[test]
    fn share_ref_uses_the_active_ui_memory_when_erasing_a_global_owner() {
        let owner = Shared::new(String::from("projected"));
        let memory = UiMemory::new(TWO_MIB);
        let allocator = memory.allocator();

        let reference = allocator.scope(|| owner.project(|value: &String| value.as_str()));

        assert_eq!(allocator.committed_bytes(), TWO_MIB);
        drop(owner);
        drop(allocator);
        drop(memory);

        assert_eq!(&*reference, "projected");
        drop(reference);
    }

    #[test]
    fn projecting_a_share_ref_reuses_its_ui_memory() {
        let memory = UiMemory::new(TWO_MIB);
        let allocator = memory.allocator();
        let source = allocator.scope(|| {
            let value: Rc<str> = Rc::from("projected");
            ShareRef::from_rc(&value)
        });

        let projected = source.project(|value: &str| value);

        assert_eq!(&*projected, "projected");
        assert_eq!(allocator.committed_bytes(), TWO_MIB);
    }
}
