//! Big-endian cursor over a binary value. Every read is bounds-checked.

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.bytes.len() {
            return None;
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.array::<1>().map(|[b]| b)
    }

    pub fn i16(&mut self) -> Option<i16> {
        self.array().map(i16::from_be_bytes)
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_be_bytes)
    }

    pub fn i32(&mut self) -> Option<i32> {
        self.array().map(i32::from_be_bytes)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_be_bytes)
    }

    pub fn i64(&mut self) -> Option<i64> {
        self.array().map(i64::from_be_bytes)
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_be_bytes)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.bytes)
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// `Some(value)` only if the whole input was consumed.
    pub fn finish<T>(&self, value: T) -> Option<T> {
        self.is_empty().then_some(value)
    }
}
