use crate::{arc_buf, ArcBuffer};

#[test]
pub fn create_with_macro() {
    let v = arc_buf![1, 2, 3, 4];
    assert_eq!(v, [1, 2, 3, 4]);
}

#[test]
pub fn create_with_macro_empty() {
    let v: ArcBuffer<i32> = arc_buf![];
    assert_eq!(v, []);
}

#[test]
pub fn create_with_macro_len() {
    let v = arc_buf![7; 6];
    assert_eq!(v, [7, 7, 7, 7, 7, 7]);
}

#[test]
pub fn create_with_macro_len_empty() {
    let v = arc_buf![7; 0];
    assert_eq!(v, []);
}
