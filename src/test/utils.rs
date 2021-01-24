use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem;
use std::thread::{self, JoinHandle};

pub struct TempThread<'a> {
    handle: Option<JoinHandle<()>>,
    _lt: PhantomData<&'a ()>,
}

impl<'a> TempThread<'a> {
    pub fn spawn(f: impl FnOnce() + Send + 'a) -> Self {
        let box_fn = unsafe {
            mem::transmute::<Box<dyn FnOnce() + Send + 'a>, Box<dyn FnOnce() + Send + 'static>>(
                Box::new(f),
            )
        };
        Self {
            handle: Some(thread::spawn(box_fn)),
            _lt: PhantomData,
        }
    }

    fn try_join(&mut self) -> Option<thread::Result<()>> {
        self.handle.take().map(JoinHandle::join)
    }

    #[allow(unused)]
    pub fn join(mut self) -> thread::Result<()> {
        self.try_join().unwrap()
    }
}

impl<'a> Drop for TempThread<'a> {
    fn drop(&mut self) {
        self.try_join().transpose().unwrap();
    }
}

#[macro_export]
macro_rules! parallel {
    ($($(let $v:ident <-)? $([$($cap:tt)*])? $f:block;)+ $(,)?) => {
        let ($($($v,)?)*) = {
            $($(let mut $v = core::mem::MaybeUninit::uninit();)?)*
            {
                $($(let $v = &mut $v;)?)*
                let threads = ($({
                    $($crate::parallel!(@capture $($cap)*);)?
                    $crate::test::utils::TempThread::spawn(move || {
                        $(*$v =)? core::mem::MaybeUninit::new($f);
                    })
                },)*);
                core::mem::drop(threads);
            }
            #[allow(unused_unsafe)]
            unsafe { ($($($v.assume_init(),)?)*) }
        };
    };
    (@capture) => {};
    (@capture $p:pat = $ex:expr $(,$($rest:tt)*)?) => {
        let $p = $ex;
        $crate::parallel!(@capture $($($rest)*)?);
    };
    (@capture ref $v:ident $(,$($rest:tt)*)?) => {
        let $v = &$v;
        $crate::parallel!(@capture $($($rest)*)?);
    };
    (@capture ref mut $v:ident $(,$($rest:tt)*)?) => {
        let $v = &mut $v;
        $crate::parallel!(@capture $($($rest)*)?);
    };
    (@capture $method:ident $v:ident $(,$($rest:tt)*)?) => {
        let $v = $v.$method();
        $crate::parallel!(@capture $($($rest)*)?);
    };
    (@capture $method:ident mut $v:ident $(,$($rest:tt)*)?) => {
        let mut $v = $v.$method();
        $crate::parallel!(@capture $($($rest)*)?);
    };
}

#[test]
fn parallel_test() {
    struct Q(i32);
    let a = 1;
    let b = 2;
    parallel! {
       let x <- [clone a] { Q(a) };
       let y <- [ref b] { Q(*b) };
       {};
    };
    assert_eq!(x.0, 1);
    assert_eq!(y.0, 2);
}
