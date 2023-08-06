use bytes::{buf::UninitSlice, BufMut};

/// Implementation of `BufMut` which discards all data
pub struct NullBuf([u8; 16]);

impl NullBuf {
    #[allow(unused)]
    pub fn new() -> Self {
        Self([0; 16])
    }
}

unsafe impl BufMut for NullBuf {
    fn remaining_mut(&self) -> usize {
        usize::MAX
    }

    unsafe fn advance_mut(&mut self, _cnt: usize) {}

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        unsafe { UninitSlice::from_raw_parts_mut(self.0.as_mut_ptr(), self.0.len()) }
    }

    fn put<T: bytes::buf::Buf>(&mut self, _src: T)
    where
        Self: Sized,
    {
    }
    fn put_slice(&mut self, _src: &[u8]) {}
    fn put_bytes(&mut self, _val: u8, _cnt: usize) {}
}
