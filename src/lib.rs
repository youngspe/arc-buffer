#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;
extern crate header_slice;

pub mod buf;
#[cfg(test)]
mod test;

pub use buf::ArcBuffer;
