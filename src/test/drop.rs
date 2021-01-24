use crate::parallel;
use crate::{arc_buf, ArcBuffer};
use core::mem;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::*;

macro_rules! dropflag {
    ($(static $($name:ident),+$(,)?;)*) => {
        $($(
            static $name: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
            $name.store(false, core::sync::atomic::Ordering::SeqCst);
        )+)*
    };
}

fn set_drop_hook<T>(buf: &mut ArcBuffer<T>, dropped: &'static AtomicBool) {
    unsafe {
        buf.set_drop_hook(Some(move || {
            dropped
                .compare_exchange(false, true, SeqCst, SeqCst)
                .expect("dropped twice!");
        }));
    }
}

fn with_drop_hook<T>(mut buf: ArcBuffer<T>, dropped: &'static AtomicBool) -> ArcBuffer<T> {
    set_drop_hook(&mut buf, dropped);
    buf
}

#[test]
fn drop_no_clones() {
    dropflag! { static DROPPED; }

    let buf = with_drop_hook(arc_buf![1, 2, 3], &DROPPED);
    assert_eq!(DROPPED.load(SeqCst), false);
    mem::drop(buf);
    assert_eq!(DROPPED.load(SeqCst), true);
}

#[test]
fn drop_clone_first() {
    dropflag! { static DROPPED; }

    let buf1 = with_drop_hook(arc_buf![1, 2, 3], &DROPPED);
    assert_eq!(DROPPED.load(SeqCst), false);
    let buf2 = buf1.clone();
    mem::drop(buf2);
    assert_eq!(DROPPED.load(SeqCst), false);
    mem::drop(buf1);
    assert_eq!(DROPPED.load(SeqCst), true);
}

#[test]
fn drop_clone_last() {
    dropflag! { static DROPPED; }

    let buf1 = with_drop_hook(arc_buf![1, 2, 3], &DROPPED);
    assert_eq!(DROPPED.load(SeqCst), false);
    let buf2 = buf1.clone();
    mem::drop(buf1);
    assert_eq!(DROPPED.load(SeqCst), false);
    mem::drop(buf2);
    assert_eq!(DROPPED.load(SeqCst), true);
}

#[test]
fn drop_after_mutating_clone() {
    dropflag! { static D1, D2; }

    let buf1 = with_drop_hook(arc_buf![1, 2, 3], &D1);
    assert_eq!(D1.load(SeqCst), false);
    let mut buf2 = buf1.clone();
    assert_eq!(D1.load(SeqCst), false);
    buf2.push(4);
    set_drop_hook(&mut buf2, &D2);
    mem::drop(buf1);
    assert_eq!(D1.load(SeqCst), true);
    assert_eq!(D2.load(SeqCst), false);
    mem::drop(buf2);
    assert_eq!(D1.load(SeqCst), true);
    assert_eq!(D2.load(SeqCst), true);
}

#[test]
fn drop_after_mutating_original() {
    dropflag! { static D1, D2; }

    let mut buf1 = with_drop_hook(arc_buf![1, 2, 3], &D1);
    assert_eq!(D1.load(SeqCst), false);
    let buf2 = buf1.clone();
    assert_eq!(D1.load(SeqCst), false);
    buf1.push(4);
    set_drop_hook(&mut buf1, &D2);
    mem::drop(buf2);
    assert_eq!(D1.load(SeqCst), true);
    assert_eq!(D2.load(SeqCst), false);
    mem::drop(buf1);
    assert_eq!(D1.load(SeqCst), true);
    assert_eq!(D2.load(SeqCst), true);
}

#[test]
fn drop_after_mutating_clone_parallel() {
    dropflag! { static D1, D2, D3; }

    let buf = with_drop_hook(arc_buf![1, 2, 3, 4], &D1);
    let buf2 = buf.clone();

    parallel! {
        [clone mut buf] {
            buf.push(4);
            set_drop_hook(&mut buf, &D2);
            assert_eq!(D2.load(SeqCst), false);
            assert_eq!(buf, [1, 2, 3, 4, 4]);
            mem::drop(buf);
            assert_eq!(D2.load(SeqCst), true);
        };
        [clone mut buf] {
            buf.push(5);
            set_drop_hook(&mut buf, &D3);
            assert_eq!(D3.load(SeqCst), false);
            assert_eq!(buf, [1, 2, 3, 4, 5]);
            mem::drop(buf);
            assert_eq!(D3.load(SeqCst), true);
        };
    }

    assert_eq!(buf, [1, 2, 3, 4]);
    assert_eq!(buf2, [1, 2, 3, 4]);
    assert_eq!(D1.load(SeqCst), false);
    assert_eq!(D2.load(SeqCst), true);
    assert_eq!(D3.load(SeqCst), true);
    mem::drop(buf);
    assert_eq!(buf2, [1, 2, 3, 4]);
    assert_eq!(D1.load(SeqCst), false);
    mem::drop(buf2);
    assert_eq!(D1.load(SeqCst), true);
}
