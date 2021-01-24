
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp;
use core::fmt;
use core::iter::FromIterator;
use core::mem::{self, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ops::{Index, IndexMut};
use core::ptr;
use core::slice::SliceIndex;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::*;
use header_slice::HeaderVec;

unsafe fn assume_init_drop<T>(un: &mut MaybeUninit<T>) {
    ptr::drop_in_place(un.as_mut_ptr());
}

unsafe fn assume_init_ref<T>(un: &MaybeUninit<T>) -> &T {
    &*(un as *const MaybeUninit<T> as *const T)
}

unsafe fn assume_init_mut<T>(un: &mut MaybeUninit<T>) -> &mut T {
    &mut *(un as *mut MaybeUninit<T> as *mut T)
}

struct Header {
    pub count: AtomicUsize,
    #[cfg(test)]
    pub drop_hook: Option<Box<dyn FnMut() + Send + 'static>>,
}

impl Default for Header {
    fn default() -> Self {
        Header {
            count: AtomicUsize::new(1),
            #[cfg(test)]
            drop_hook: None,
        }
    }
}
#[cfg(test)]
fn panic_hook() {
    panic!("drop hook not registered");
}

#[cfg(test)]
impl Drop for Header {
    fn drop(&mut self) {
        if let Some(ref mut hook) = self.drop_hook.take() {
            hook()
        }
    }
}

pub struct ArcBuffer<T> {
    inner: MaybeUninit<HeaderVec<Header, T>>,
}

impl<T> ArcBuffer<T> {
    #[cfg(test)]
    pub(crate) unsafe fn set_drop_hook<F: FnMut() + Send + 'static>(&mut self, hook: Option<F>) {
        self.inner_mut().head.drop_hook = match hook {
            Some(hook) => Some(Box::new(hook)),
            None => None,
        };
    }
    pub fn new() -> Self {
        Self::from_inner(Default::default())
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self::from_inner(HeaderVec::with_capacity(Default::default(), cap))
    }

    fn from_inner(mut inner: HeaderVec<Header, T>) -> Self {
        inner.head.count = AtomicUsize::new(1);

        Self {
            inner: MaybeUninit::new(inner),
        }
    }

    fn inner(&self) -> &HeaderVec<Header, T> {
        unsafe { assume_init_ref(&self.inner) }
    }

    unsafe fn inner_mut(&mut self) -> &mut HeaderVec<Header, T> {
        assume_init_mut(&mut self.inner)
    }

    pub fn len(&self) -> usize {
        self.inner().len()
    }

    pub unsafe fn copy_from_ptr_unsafe(src: *const T, len: usize) -> Self {
        let mut uninit = ArcBuffer::new_uninit(len);
        ptr::copy_nonoverlapping(
            src as *const MaybeUninit<T>,
            uninit.inner_mut().body.as_mut_ptr(),
            len,
        );
        uninit.assume_init()
    }
}

impl<T> ArcBuffer<MaybeUninit<T>> {
    pub fn new_uninit(len: usize) -> Self {
        Self::from_inner(HeaderVec::new_uninit_values(Default::default(), len))
    }

    pub unsafe fn assume_init(self) -> ArcBuffer<T> {
        let new_inner = ptr::read(self.inner.as_ptr()).assume_init_values();
        mem::forget(self);
        ArcBuffer {
            inner: MaybeUninit::new(new_inner),
        }
    }
}

impl<T: Copy> ArcBuffer<T> {
    pub fn copy_from_slice(src: &[T]) -> Self {
        Self {
            inner: MaybeUninit::new(HeaderVec::copy_from_slice(Header::default(), src)),
        }
    }

    pub fn copy_to_new(&self) -> Self {
        Self::copy_from_slice(self)
    }

    pub fn filled(value: T, len: usize) -> Self {
        unsafe {
            let mut uninit = HeaderVec::new_uninit_values(Header::default(), len);

            for x in &mut uninit.body {
                *x = MaybeUninit::new(value);
            }

            Self {
                inner: MaybeUninit::new(uninit.assume_init_values()),
            }
        }
    }

    fn inner_unique(&mut self) -> &mut HeaderVec<Header, T> {
        if self.inner().head.count.load(Acquire) > 1 {
            #[allow(unused_mut)]
            let mut temp = self.copy_to_new();

            #[cfg(test)]
            if self.inner().head.drop_hook.is_some() {
                unsafe {
                    temp.set_drop_hook(Some(panic_hook));
                }
            }

            *self = temp;
        }

        unsafe { self.inner_mut() }
    }

    pub fn make_mut(&mut self) -> &mut [T] {
        &mut self.inner_unique().body
    }

    pub fn extend_from_slice(&mut self, src: &[T]) {
        self.inner_unique().extend_from_slice(src)
    }

    pub fn push(&mut self, value: T) {
        self.inner_unique().push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner_unique().pop()
    }

    pub fn insert(&mut self, index: usize, value: T) {
        self.inner_unique().insert(index, value);
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        self.inner_unique().remove(index)
    }

    pub fn swap_remove(&mut self, index: usize) -> Option<T> {
        self.inner_unique().swap_remove(index)
    }
}

pub struct IntoIter<T> {
    inner: ArcBuffer<T>,
    index: usize,
}

impl<T: Copy> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.index < self.inner.len() {
            let index = self.index;
            self.index += 1;
            Some(self.inner[index])
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.inner.len() - self.index;
        (size, Some(size))
    }
}

impl<T: Copy> ExactSizeIterator for IntoIter<T> {}

impl<T: Copy> IntoIterator for ArcBuffer<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self,
            index: 0,
        }
    }
}

impl<'a, T> IntoIterator for &'a ArcBuffer<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner().body.iter()
    }
}

impl<'a, T: Copy> IntoIterator for &'a mut ArcBuffer<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner_unique().body.iter_mut()
    }
}

impl<T> Clone for ArcBuffer<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.inner().head.count.fetch_add(1, Acquire);
            let mut new = Self {
                inner: MaybeUninit::uninit(),
            };
            ptr::copy(self.inner.as_ptr(), new.inner.as_mut_ptr(), 1);
            new
        }
    }
}

impl<T> Drop for ArcBuffer<T> {
    fn drop(&mut self) {
        if self.inner().head.count.fetch_sub(1, Release) == 1 {
            unsafe {
                assume_init_drop(&mut self.inner);
            }
        }
    }
}

impl<T, S: SliceIndex<[T]>> Index<S> for ArcBuffer<T> {
    type Output = S::Output;
    fn index(&self, i: S) -> &Self::Output {
        self.inner().body.index(i)
    }
}

impl<T: Copy, S: SliceIndex<[T]>> IndexMut<S> for ArcBuffer<T> {
    fn index_mut(&mut self, i: S) -> &mut Self::Output {
        self.inner_unique().body.index_mut(i)
    }
}

impl<T> Deref for ArcBuffer<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.inner().body
    }
}

impl<T: Copy> DerefMut for ArcBuffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.make_mut()
    }
}

impl<T> FromIterator<T> for ArcBuffer<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        Self::from_inner(it.into_iter().collect())
    }
}

impl<T: Copy> Extend<T> for ArcBuffer<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        self.inner_unique().extend(it);
    }
}

impl<T> From<Box<[T]>> for ArcBuffer<T> {
    fn from(src: Box<[T]>) -> Self {
        unsafe {
            let new = Self::copy_from_ptr_unsafe(src.as_ptr(), src.len());
            let box_ptr = Box::into_raw(src);
            mem::drop(Box::from_raw(box_ptr as *mut [MaybeUninit<T>]));
            new
        }
    }
}

impl<T> From<Vec<T>> for ArcBuffer<T> {
    fn from(src: Vec<T>) -> Self {
        src.into_boxed_slice().into()
    }
}

impl<T> AsRef<[T]> for ArcBuffer<T> {
    fn as_ref(&self) -> &[T] {
        &*self
    }
}

impl<T: Copy> AsMut<[T]> for ArcBuffer<T> {
    fn as_mut(&mut self) -> &mut [T] {
        &mut *self
    }
}

impl<T, S: AsRef<[T]>> PartialEq<S> for ArcBuffer<T>
where
    [T]: PartialEq,
{
    fn eq(&self, rhs: &S) -> bool {
        <[T] as PartialEq>::eq(self, rhs.as_ref())
    }
    fn ne(&self, rhs: &S) -> bool {
        <[T] as PartialEq>::ne(self, rhs.as_ref())
    }
}

impl<T> Eq for ArcBuffer<T> where [T]: Eq {}

impl<T, S: AsRef<[T]>> PartialOrd<S> for ArcBuffer<T>
where
    [T]: PartialOrd,
{
    fn partial_cmp(&self, rhs: &S) -> Option<cmp::Ordering> {
        <[T] as PartialOrd>::partial_cmp(self, rhs.as_ref())
    }
}

impl<T> Ord for ArcBuffer<T>
where
    [T]: Ord,
{
    fn cmp(&self, rhs: &Self) -> cmp::Ordering {
        <[T] as Ord>::cmp(self, rhs)
    }
}

impl<T> fmt::Debug for ArcBuffer<T>
where
    [T]: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <[T] as fmt::Debug>::fmt(self, f)
    }
}

impl<T> fmt::Display for ArcBuffer<T>
where
    [T]: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <[T] as fmt::Display>::fmt(self, f)
    }
}

unsafe impl<T> Send for ArcBuffer<T> {}
unsafe impl<T> Sync for ArcBuffer<T> {}

#[macro_export]
macro_rules! arc_buf {
    ($($val:expr),* $(,)?) => {{
        let vals = [$($val),*];
        #[allow(unused_unsafe)]
        let v = unsafe {
            $crate::buf::ArcBuffer::copy_from_ptr_unsafe(vals.as_ptr(), vals.len())
        };
        core::mem::forget(vals);
        v
    }};
    ($val:expr; $len:expr) => {
        core::iter::repeat($val).take($len).collect::<$crate::buf::ArcBuffer<_>>()
    }
}
