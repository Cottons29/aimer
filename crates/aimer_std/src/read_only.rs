//! Single-threaded shared ownership with read-only field projection.
//!
//! [`Shared`] owns one heap allocation containing strong and weak-reference
//! counters and a value. Allocation is performed directly through
//! [`std::alloc`]; neither `Box` nor another smart pointer owns the value.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::borrow::Borrow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::{self, NonNull};
use std::rc::Rc;

struct SharedAllocation<T> {
    strong: Cell<usize>,
    // Includes one implicit weak reference while the value is alive.
    weak: Cell<usize>,
    value: ManuallyDrop<T>,
}

/// Releases the implicit weak reference even when dropping the value unwinds.
struct ReleaseImplicitWeak<T> {
    allocation: NonNull<SharedAllocation<T>>,
}

impl<T> Drop for ReleaseImplicitWeak<T> {
    fn drop(&mut self) {
        let weak = unsafe { self.allocation.as_ref() }.weak.get();
        debug_assert!(weak > 0, "the implicit weak reference must be alive");

        if weak == 1 {
            // SAFETY: the implicit weak reference is the final reference to the
            // allocation, and the value has already been dropped or moved out.
            unsafe { deallocate_allocation(self.allocation) };
        } else {
            unsafe { self.allocation.as_ref() }.weak.set(weak - 1);
        }
    }
}

/// Deallocates an allocation whose value has already been destroyed.
unsafe fn deallocate_allocation<T>(allocation: NonNull<SharedAllocation<T>>) {
    // SAFETY: the caller guarantees that no strong or weak reference can access
    // the allocation again and that `value` has already been destroyed.
    unsafe {
        dealloc(
            allocation.as_ptr().cast::<u8>(),
            Layout::new::<SharedAllocation<T>>(),
        );
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
/// `Shared<T>` stores `T` and its strong-reference count in one allocation.
/// Cloning a `Shared` increments that count. The value is destroyed when the
/// final strong handle is dropped; a [`Weak`] handle keeps only the allocation
/// alive.
///
/// The value is read-only while shared. Mutable access is available through
/// [`get_mut`](Self::get_mut) when there is exactly one strong owner and no
/// [`Weak`] handles, or through [`make_mut`](Self::make_mut) using copy-on-write.
///
/// `Shared` is deliberately neither [`Send`] nor [`Sync`]. Cycles made entirely
/// from strong handles still leak; use [`Weak`] for non-owning back references.
///
/// # Examples
///
/// ```
/// use aimer_std::read_only::Shared;
///
/// let first = Shared::new(String::from("pineapple"));
/// let second = first.clone();
///
/// assert_eq!(&*second, "pineapple");
/// assert_eq!(Shared::strong_count(&first), 2);
/// ```
pub struct Shared<T> {
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
/// use aimer_std::read_only::Shared;
///
/// let value = Shared::new(String::from("pineapple"));
/// let weak = Shared::downgrade(&value);
/// drop(value);
///
/// assert!(weak.upgrade().is_none());
/// ```
pub struct Weak<T> {
    allocation: NonNull<SharedAllocation<T>>,
    not_thread_safe: PhantomData<*const Cell<()>>,
}

impl<T> Shared<T> {
    /// Allocates `value` with an initial strong-reference count of one.
    #[must_use]
    pub fn new(value: T) -> Self {
        let layout = Layout::new::<SharedAllocation<T>>();

        // SAFETY: the layout is non-zero because the allocation contains a
        // `usize`. A null result is handled before the pointer is written.
        let raw = unsafe { alloc(layout).cast::<SharedAllocation<T>>() };
        let Some(allocation) = NonNull::new(raw) else {
            handle_alloc_error(layout);
        };

        // SAFETY: `allocation` is suitably sized and aligned uninitialized
        // storage returned for exactly `layout`.
        unsafe {
            allocation.as_ptr().write(SharedAllocation {
                strong: Cell::new(1),
                weak: Cell::new(1),
                value: ManuallyDrop::new(value),
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

    /// Returns mutable access when `this` is the only strong owner and no weak
    /// handles can later be upgraded to the allocation.
    #[inline]
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if Self::strong_count(this) == 1 && Self::weak_count(this) == 0 {
            // SAFETY: `&mut Self` prevents use through this handle, one strong
            // owner proves that no other `Shared` or projection exists, and no
            // weak handle can be upgraded concurrently on this thread.
            Some(unsafe { &mut *this.value_ptr() })
        } else {
            None
        }
    }

    /// Returns mutable access, cloning into a new allocation when another
    /// strong or weak handle exists.
    #[inline]
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        if Self::strong_count(this) != 1 || Self::weak_count(this) != 0 {
            *this = Self::new((**this).clone());
        }

        Self::get_mut(this).expect("a newly allocated Shared has one owner")
    }

    /// Moves the value out when `this` is its only owner.
    ///
    /// If another strong handle exists, ownership of `this` is returned in
    /// `Err`. Existing weak handles remain valid but cannot be upgraded.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        if Self::strong_count(&this) != 1 {
            return Err(this);
        }

        let this = ManuallyDrop::new(this);
        let allocation = this.allocation;
        unsafe { allocation.as_ref() }.strong.set(0);
        let _release_weak = ReleaseImplicitWeak { allocation };

        // SAFETY: the strong count is zero and `this` cannot run `Drop`, so the
        // value is uniquely owned and cannot be observed or dropped again.
        let value = unsafe { ManuallyDrop::take(&mut (*allocation.as_ptr()).value) };
        Ok(value)
    }

    /// Returns the value, cloning it only when other owners exist.
    pub fn unwrap_or_clone(this: Self) -> T
    where
        T: Clone,
    {
        Self::try_unwrap(this).unwrap_or_else(|shared| (*shared).clone())
    }

    /// Creates a read-only handle to a borrowed part of `T`.
    ///
    /// The returned [`SharedRef`] owns a strong reference to the complete value,
    /// so it can outlive this root handle. Its selector is evaluated on every
    /// access; no raw interior field pointer is stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_std::read_only::Shared;
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
    pub fn project<Field, Select>(&self, select: Select) -> SharedRef<T, Field, Select>
    where
        Field: ?Sized,
        Select: for<'a> Fn(&'a T) -> &'a Field,
    {
        SharedRef {
            owner: self.clone(),
            select,
            field: PhantomData,
        }
    }

    #[inline]
    fn inner(&self) -> &SharedAllocation<T> {
        // SAFETY: every live `Shared` owns one strong reference, so its
        // allocation remains initialized for the duration of this borrow.
        unsafe { self.allocation.as_ref() }
    }

    #[inline]
    fn value_ptr(&self) -> *mut T {
        // SAFETY: the allocation remains live while `self` owns a strong count.
        // `ManuallyDrop<T>` has the same layout and address as `T`.
        unsafe { ptr::addr_of_mut!((*self.allocation.as_ptr()).value).cast::<T>() }
    }
}

impl<T> Weak<T> {
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

impl<T> Clone for Weak<T> {
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

impl<T> Drop for Weak<T> {
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

impl<T> Clone for Shared<T> {
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

impl<T> Drop for Shared<T> {
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
            ManuallyDrop::drop(&mut (*self.allocation.as_ptr()).value);
        }
    }
}

impl<T> Deref for Shared<T> {
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

impl<T> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T> Borrow<T> for Shared<T> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, formatter)
    }
}

impl<T: fmt::Display> fmt::Display for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, formatter)
    }
}

impl<T> fmt::Pointer for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&self.value_ptr(), formatter)
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: Eq> Eq for Shared<T> {}

impl<T: PartialOrd> PartialOrd for Shared<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for Shared<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash> Hash for Shared<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

/// An owning, read-only projection into a [`Shared`] value.
///
/// A `SharedRef` stores a strong owner plus a selector. It never stores a raw
/// pointer into the selected field, so access remains valid without exposing a
/// lifetime parameter on the persistent handle.
pub struct SharedRef<Owner, Field: ?Sized, Select> {
    owner: Shared<Owner>,
    select: Select,
    field: PhantomData<*const Field>,
}

impl<T: ?Sized> SharedRef<Rc<T>, T, fn(&Rc<T>) -> &T> {
    /// Creates a read-only handle to the value held by an [`Rc`].
    ///
    /// The returned handle clones the `Rc` and keeps its allocation alive after
    /// `rc` is dropped. The value is borrowed through the `Rc` on every access;
    /// it is not cloned.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_std::read_only::SharedRef;
    /// use std::rc::Rc;
    ///
    /// let value = Rc::new(String::from("pineapple"));
    /// let reference = SharedRef::from_rc(&value);
    /// drop(value);
    ///
    /// assert_eq!(&*reference, "pineapple");
    /// ```
    #[must_use]
    #[inline]
    pub fn from_rc(rc: &Rc<T>) -> Self {
        Self {
            owner: Shared::new(Rc::clone(rc)),
            select: Rc::as_ref,
            field: PhantomData,
        }
    }
}

impl<Owner, Field, Select> SharedRef<Owner, Field, Select>
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

    /// Returns the complete owning value.
    #[must_use]
    #[inline]
    pub fn owner(&self) -> &Shared<Owner> {
        &self.owner
    }
}

impl<Owner, Field, Select> Clone for SharedRef<Owner, Field, Select>
where
    Field: ?Sized,
    Select: Clone,
{
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            select: self.select.clone(),
            field: PhantomData,
        }
    }
}

impl<Owner, Field, Select> Deref for SharedRef<Owner, Field, Select>
where
    Field: ?Sized,
    Select: for<'a> Fn(&'a Owner) -> &'a Field,
{
    type Target = Field;

    #[inline]
    fn deref(&self) -> &Field {
        self.get()
    }
}

impl<Owner, Field, Select> AsRef<Field> for SharedRef<Owner, Field, Select>
where
    Field: ?Sized,
    Select: for<'a> Fn(&'a Owner) -> &'a Field,
{
    fn as_ref(&self) -> &Field {
        self.get()
    }
}

impl<Owner, Field, Select> fmt::Debug for SharedRef<Owner, Field, Select>
where
    Field: ?Sized + fmt::Debug,
    Select: for<'a> Fn(&'a Owner) -> &'a Field,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.get(), formatter)
    }
}

impl<Owner, Field, Select> fmt::Display for SharedRef<Owner, Field, Select>
where
    Field: ?Sized + fmt::Display,
    Select: for<'a> Fn(&'a Owner) -> &'a Field,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.get(), formatter)
    }
}
