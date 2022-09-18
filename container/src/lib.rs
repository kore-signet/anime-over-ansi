use arrayvec::ArrayVec;
use rend::LittleEndian;
pub mod metadata;
pub mod packet;

#[cfg(feature = "codec")]
pub mod codec;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ValuePair {
    pub key: LittleEndian<u32>,
    pub value: LittleEndian<u32>,
}

#[derive(Debug, Clone)]
pub struct TinyMap {
    pub inner: ArrayVec<ValuePair, 64>,
}

impl TinyMap {
    pub const fn new() -> TinyMap {
        TinyMap {
            inner: ArrayVec::new_const(),
        }
    }

    pub fn get(&self, key: u32) -> Option<u32> {
        self.inner
            .binary_search_by_key(&key, |v| v.key.value())
            .map(|idx| self.inner[idx].value.value())
            .ok()
    }

    pub fn insert(&mut self, key: u32, value: u32) {
        self.inner.push(ValuePair {
            key: LittleEndian::from(key),
            value: LittleEndian::from(value),
        })
    }

    pub fn serialize(&self) -> &[u8] {
        unsafe {
            use std::slice;
            slice::from_raw_parts(self.inner.as_ptr() as *const u8, self.inner.len() * 8)
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<TinyMap> {
        if bytes.len() % 8 != 0 {
            return None;
        }

        let mut inner = ArrayVec::new_const();
        let pair_slice = unsafe {
            use std::slice;
            slice::from_raw_parts(bytes.as_ptr() as *const ValuePair, bytes.len() / 8)
        };

        inner.try_extend_from_slice(pair_slice).ok()?;

        Some(TinyMap { inner })
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for TinyMap {
    fn default() -> Self {
        TinyMap::new()
    }
}

pub mod packet_flags {
    use enumflags2::bitflags;

    pub const VIDEO_PACKET_KEY: u32 =
        unsafe { std::mem::transmute::<[u8; 4], u32>([b'v', b'i', b'd', b'f']) };
    pub const ZSTD_UNCOMPRESSED_LEN_KEY: u32 =
        unsafe { std::mem::transmute::<[u8; 4], u32>([b'z', b's', b't', b'l']) };

    #[bitflags]
    #[repr(u32)]
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub enum VideoPacketFlags {
        Keyframe,
    }
}

// some hacks on the bytes crate - specifically, a reflected copy of BytesMut::from_vec
pub mod bytes_hacking {
    use bytes::BytesMut;
    use std::{mem, ptr::NonNull, sync::atomic::AtomicUsize};

    const KIND_VEC: usize = 0b1;
    const MAX_ORIGINAL_CAPACITY_WIDTH: usize = 17;
    const MIN_ORIGINAL_CAPACITY_WIDTH: usize = 10;
    const ORIGINAL_CAPACITY_OFFSET: usize = 2;

    #[cfg(target_pointer_width = "64")]
    const PTR_WIDTH: usize = 64;
    #[cfg(target_pointer_width = "32")]
    const PTR_WIDTH: usize = 32;

    #[inline]
    fn invalid_ptr<T>(addr: usize) -> *mut T {
        let ptr = core::ptr::null_mut::<u8>().wrapping_add(addr);
        debug_assert_eq!(ptr as usize, addr);
        ptr.cast::<T>()
    }

    #[inline]
    fn vptr(ptr: *mut u8) -> NonNull<u8> {
        if cfg!(debug_assertions) {
            NonNull::new(ptr).expect("Vec pointer should be non-null")
        } else {
            unsafe { NonNull::new_unchecked(ptr) }
        }
    }

    #[inline]
    fn original_capacity_to_repr(cap: usize) -> usize {
        let width = PTR_WIDTH - ((cap >> MIN_ORIGINAL_CAPACITY_WIDTH).leading_zeros() as usize);
        std::cmp::min(
            width,
            MAX_ORIGINAL_CAPACITY_WIDTH - MIN_ORIGINAL_CAPACITY_WIDTH,
        )
    }

    #[allow(dead_code)]
    struct SharedReflection {
        vec: Vec<u8>,
        original_capacity_repr: usize,
        ref_count: AtomicUsize,
    }

    #[allow(dead_code)]
    struct BytesMutReflection {
        ptr: NonNull<u8>,
        len: usize,
        cap: usize,
        data: *mut SharedReflection,
    }

    // from https://docs.rs/bytes/1.2.1/src/bytes/bytes_mut.rs.html#816
    pub unsafe fn bytesmut_from_vec(mut vec: Vec<u8>) -> BytesMut {
        let ptr = vptr(vec.as_mut_ptr());
        let len = vec.len();
        let cap = vec.capacity();
        mem::forget(vec);

        let original_capacity_repr = original_capacity_to_repr(cap);
        let data = (original_capacity_repr << ORIGINAL_CAPACITY_OFFSET) | KIND_VEC;

        mem::transmute(BytesMutReflection {
            ptr,
            len,
            cap,
            data: invalid_ptr(data),
        })
    }
}

pub use packet_flags::*;
