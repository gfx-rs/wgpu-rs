use crate::BufferAddress;

mod private {
    pub trait Sealed {}

    impl<'a> Sealed for &'a super::Buffer {}
    impl<'a, Size: super::SizedBuffer> Sealed for super::BufferRange<'a, Size> {}

    impl Sealed for super::Bounded {}
    impl Sealed for super::Unbounded {}
    impl Sealed for super::Unsure {}

    impl Sealed for super::ToEnd {}
    impl Sealed for super::BufferAddress {}
    impl Sealed for Option<super::BufferAddress> {}
}

pub trait SizedBuffer: private::Sealed {
    type Storage: SizeStorage;
}

pub trait SizeStorage: Copy + Clone + PartialEq + std::fmt::Debug + private::Sealed {
    fn to_option(self) -> Option<BufferAddress>;
}

pub trait StaticSizeStorage: SizeStorage {}
pub trait StaticSizedBuffer: SizedBuffer {}
impl<Size: SizedBuffer> StaticSizedBuffer for Size where Size::Storage: StaticSizeStorage {}

pub struct Bounded(());
pub struct Unbounded(());
pub struct Unsure(());

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ToEnd;

impl SizedBuffer for Bounded {
    type Storage = BufferAddress;
}
impl SizedBuffer for Unbounded {
    type Storage = ToEnd;
}
impl SizedBuffer for Unsure {
    type Storage = Option<BufferAddress>;
}
impl SizeStorage for ToEnd {
    fn to_option(self) -> Option<BufferAddress> { None }
}
impl SizeStorage for BufferAddress {
    fn to_option(self) -> Option<BufferAddress> { Some(self) }
}
impl SizeStorage for Option<BufferAddress> {
    fn to_option(self) -> Option<BufferAddress> { self }
}
impl StaticSizeStorage for ToEnd {}
impl StaticSizeStorage for BufferAddress {}


pub trait RangedBuffer<'a, End: SizeStorage, Bounds: SizedBuffer>: private::Sealed {
    /// Index a `Buffer` or `BufferRange` by a range.
    /// 
    /// It's important to note that the range can take on a few forms:
    /// 1. offset, size
    /// 2. offset, [`ToEnd`](../struct.ToEnd.html)
    fn range(self, offset: BufferAddress, size: End) -> BufferRange<'a, Bounds>;
}

/// A handle to a GPU-accessible buffer.
#[derive(Debug, PartialEq)]
pub struct Buffer {
    pub(crate) id: wgc::id::BufferId,
    pub(crate) device_id: wgc::id::DeviceId,
}

/// A handle to a ranged, GPU-accessible buffer.
pub struct BufferRange<'a, Size: SizedBuffer> {
    pub(crate) buffer: &'a Buffer,
    pub(crate) offset: BufferAddress,
    pub(crate) size: Size::Storage,
}

impl<'a, Size: SizedBuffer> Clone for BufferRange<'a, Size> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer,
            offset: self.offset,
            size: self.size,
        }
    }
}
impl<'a, Size: SizedBuffer> Copy for BufferRange<'a, Size> {}
impl<'a, Size: SizedBuffer> PartialEq for BufferRange<'a, Size> {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
        && self.offset == other.offset
        && self.size == other.size
    }
}

impl<'a, Size: SizedBuffer> std::fmt::Debug for BufferRange<'a, Size> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("BufferRange")
            .field("buffer", &self.buffer)
            .field("offset", &self.offset)
            .field("size", &self.size)
            .finish()
    }
}

impl<'a> From<&'a Buffer> for BufferRange<'a, Unbounded> {
    fn from(buffer: &'a Buffer) -> BufferRange<'a, Unbounded> {
        buffer.range(0, ToEnd)
    }
}
impl<'a> From<&'a Buffer> for BufferRange<'a, Unsure> {
    fn from(buffer: &'a Buffer) -> BufferRange<'a, Unsure> {
        buffer.range(0, ToEnd).into()
    }
}
impl<'a, Size: StaticSizedBuffer> From<BufferRange<'a, Size>> for BufferRange<'a, Unsure> {
    fn from(buffer: BufferRange<'a, Size>) -> BufferRange<'a, Unsure> {
        BufferRange {
            buffer: buffer.buffer,
            offset: buffer.offset,
            size: buffer.size.to_option(),
        }
    }
}

/// Implementation of `RangedBuffer` for `&Buffer`.
// impl<'a, Size: SizedBuffer> RangedBuffer<'a , Size::Storage, Size> for &'a Buffer {
//     fn range(self, offset: BufferAddress, size: Size::Storage) -> BufferRange<'a, Size> {
//         BufferRange {
//             buffer: self,
//             offset,
//             size,
//         }
//     }
// }

impl<'a> RangedBuffer<'a, ToEnd, Unbounded> for &'a Buffer {
    fn range(self, offset: BufferAddress, size: ToEnd) -> BufferRange<'a, Unbounded> {
        BufferRange {
            buffer: self,
            offset,
            size,
        }
    }
}
impl<'a> RangedBuffer<'a, BufferAddress, Bounded> for &'a Buffer {
    fn range(self, offset: BufferAddress, size: BufferAddress) -> BufferRange<'a, Bounded> {
        BufferRange {
            buffer: self,
            offset,
            size,
        }
    }
}
impl<'a> RangedBuffer<'a, Option<BufferAddress>, Unsure> for &'a Buffer {
    fn range(self, offset: BufferAddress, size: Option<BufferAddress>) -> BufferRange<'a, Unsure> {
        BufferRange {
            buffer: self,
            offset,
            size,
        }
    }
}

/// `Bounded` -> `Bounded`
impl<'a> RangedBuffer<'a, BufferAddress, Bounded> for BufferRange<'a, Bounded> {
    fn range(self, offset: BufferAddress, size: BufferAddress) -> BufferRange<'a, Bounded> {
        assert!(self.offset + size <= self.size, "range must fit inside size of supplied `BufferRange`.");

        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size,
        }
    }
}

impl<'a> RangedBuffer<'a, ToEnd, Bounded> for BufferRange<'a, Bounded> {
    fn range(self, offset: BufferAddress, _: ToEnd) -> BufferRange<'a, Bounded> {
        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size: self.size,
        }
    }
}

// -----

/// `Unbounded` -> `Bounded` or `Unbounded`
// impl<'a, Size: SizedBuffer> RangedBuffer<'a, Size::Storage, Size> for BufferRange<'a, Unbounded> {
//     fn range(self, offset: BufferAddress, size: Size::Storage) -> BufferRange<'a, Size> {
//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset + offset,
//             size,
//         }
//     }
// }
impl<'a> RangedBuffer<'a, ToEnd, Unbounded> for BufferRange<'a, Unbounded> {
    fn range(self, offset: BufferAddress, size: ToEnd) -> BufferRange<'a, Unbounded> {
        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size,
        }
    }
}
impl<'a> RangedBuffer<'a, BufferAddress, Bounded> for BufferRange<'a, Unbounded> {
    fn range(self, offset: BufferAddress, size: BufferAddress) -> BufferRange<'a, Bounded> {
        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size,
        }
    }
}
impl<'a> RangedBuffer<'a, Option<BufferAddress>, Unsure> for BufferRange<'a, Unbounded> {
    fn range(self, offset: BufferAddress, size: Option<BufferAddress>) -> BufferRange<'a, Unsure> {
        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size,
        }
    }
}

/*
 * Unsure!
 */

/// `Unsure` -> `Unsure`
impl<'a> RangedBuffer<'a, Option<BufferAddress>, Unsure> for BufferRange<'a, Unsure> {
    fn range(self, offset: BufferAddress, size: Option<BufferAddress>) -> BufferRange<'a, Unsure> {
        let size = match (self.size, size) {
            (None, None) => None,
            (Some(old_size), None) => Some(old_size),
            (None, Some(new_size)) => Some(new_size),
            (Some(old_size), Some(new_size)) => {
                assert!(old_size >= new_size, "new size must fit inside the buffer bounds already known");
                Some(new_size)
            }
        };

        BufferRange {
            buffer: self.buffer,
            offset: self.offset + offset,
            size,
        }
    }
}

impl<'a, Storage: StaticSizeStorage> RangedBuffer<'a, Storage, Unsure> for BufferRange<'a, Unsure>
{
    fn range(self, offset: BufferAddress, size: Storage) -> BufferRange<'a, Unsure> {
        self.range(offset, size.to_option())
    }
}

// impl<'a> RangedBuffer<'a, Bounded> for BufferRange<'a, Bounded> {
//     fn range(self, start: BufferAddress, size: BufferAddress) -> BufferRange<'a, Bounded> {
//         assert!(size <= self.size - start, "range must fit inside size of supplied `BufferRange`.");

//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset + start,
//             size,
//         }
//     }
// }

// // ----

// impl<'a> RangedBuffer<'a, Unbounded> for BufferRange<'a, Unbounded> {
//     fn range(self, start: BufferAddress, size: ToEnd) -> BufferRange<'a, Unbounded> {
//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset + self.start,
//             size,
//         }
//     }
// }

// // ----


// impl<'a> RangedBuffer<'a, Unbounded, Bounded> for BufferRange<'a, Unbounded> {
//     fn range(self, start: BufferAddress, size: BufferAddress) -> BufferRange<'a, Bounded> {
//         BufferRange {
//             buffer: self,
//             offset: 0,
//             size,
//         }
//     }
// }

// /*
//  * Implementations of `RangedBuffer` for `BufferRange`
//  */

// /* 
//  * `Range`
//  */



// impl<'a> RangedBuffer<'a, Bounded> for BufferRange<'a, Unbounded> {
//     fn range(self, range: Range<BufferAddress>) -> BufferRange<'a, Bounded> {
//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset + range.start,
//             size: range.end,
//         }
//     }
// }

// /* 
//  * `RangeFull`
//  */

// impl<'a> RangedBuffer<'a, Bounded> for BufferRange<'a, Bounded> {
//     fn range(self, _: RangeFull) -> BufferRange<'a, Bounded> {
//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset,
//             size: self.size,
//         }
//     }
// }

// impl<'a> RangedBuffer<'a, Unbounded> for BufferRange<'a, Unbounded> {
//     fn range(self, _: RangeFull) -> BufferRange<'a, Unbounded> {
//         BufferRange {
//             buffer: self.buffer,
//             offset: self.offset,
//             size: self.size,
//         }
//     }
// }

// /*
//  * Implentation of `RangedBuffer` for `BufferRange<Unsure>`
//  */

// impl<'a> RangedBuffer<'a, Unsure> for BufferRange<'a, Unsure> {
//     fn range(self, range: Range<BufferAddress>) -> BufferRange<'a, Unsure> {
//         match self.size {
//             Some(size) => BufferRange::<Bounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: size,
//             }.range(range).into(),
//             None => BufferRange::<Unbounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: (),
//             }.range(range).into(),
//         }
//     }
// }

// impl<'a> RangedBuffer<'a, Unsure> for BufferRange<'a, Unsure> {
//     fn range(self, range: RangeFrom<BufferAddress>) -> BufferRange<'a, Unsure> {
//         match self.size {
//             Some(size) => BufferRange::<Bounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: size,
//             }.range(range).into(),
//             None => BufferRange::<Unbounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: (),
//             }.range(range).into(),
//         }
//     }
// }

// impl<'a> RangedBuffer<'a, Unsure> for BufferRange<'a, Unsure> {
//     fn range(self, range: RangeFull) -> BufferRange<'a, Unsure> {
//         match self.size {
//             Some(size) => BufferRange::<Bounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: size,
//             }.range(range).into(),
//             None => BufferRange::<Unbounded> {
//                 buffer: self.buffer,
//                 offset: self.offset,
//                 size: (),
//             }.range(range).into(),
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::{RangedBuffer, Buffer, ToEnd};

    #[allow(dead_code, unused)]
    fn compiler_finds_right_implementation(buffer: &Buffer) {
        let b0 = buffer.range(0, ToEnd);
        let b1 = buffer.range(0, 10);
        
        let br0 = b0.range(0, ToEnd);
        let br1 = b0.range(0, 10);
        let br2 = b1.range(0, ToEnd);
        let br3 = b1.range(0, 10);
    }
}