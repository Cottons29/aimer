use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::AtomicUsize;

pub struct MyArc<T> {
    ptr: NonNull<MyAtomicInner<T>>,
}

unsafe impl<T> Send for MyArc<T> {}
unsafe impl<T> Sync for MyArc<T> {}

struct MyAtomicInner<T> {
    counter: AtomicUsize,
    value: T,
}

impl<T> MyArc<T> {
    pub fn new(data: T) -> Self {
        let boxed = Box::new(MyAtomicInner { counter: AtomicUsize::new(1), value: data });

        MyArc { ptr: unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) } }
    }

    fn inner(&self) -> &MyAtomicInner<T> {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Clone for MyArc<T> {
    fn clone(&self) -> Self {
        let inner = self.inner();
        let count_before = inner.counter.load(std::sync::atomic::Ordering::Relaxed);

        let count_after = count_before + 1;
        inner.counter.store(count_after, std::sync::atomic::Ordering::Relaxed);

        MyArc { ptr: self.ptr }
    }
}

impl<T> Deref for MyArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner().value
    }
}

impl<T> Drop for MyArc<T> {
    fn drop(&mut self) {
        let inner = unsafe { self.ptr.as_ref() };
        let count_before = inner.counter.load(std::sync::atomic::Ordering::Relaxed);
        let count_after = count_before - 1;
        inner.counter.store(count_after, std::sync::atomic::Ordering::Relaxed);

        if count_after == 0 {
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) }
        }
    }
}
