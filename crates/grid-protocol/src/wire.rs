//! Wire primitives: LEB128 integers, floats, strings, bounded lists.
//!
//! Every message is untrusted on the receiving side, so the reader bounds each length before it
//! allocates and never panics: a list's count may not exceed the bytes left (each item is at least
//! one byte), and strings are capped at [`MAX_STR_BYTES`].

use std::fmt;

/// Longest string on the wire, in UTF-8 bytes. Excel caps cell text at 32,767 characters
/// (at most 128 KiB of UTF-8), so this leaves room for formulas and names.
pub const MAX_STR_BYTES: usize = 1 << 20;

/// Why a message could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The message ended inside a field.
    Truncated,
    /// The message had bytes left after its last field.
    TrailingBytes,
    /// A LEB128 integer was longer than 10 bytes or overflowed its type.
    BadInteger,
    /// A string was not UTF-8 or was longer than [`MAX_STR_BYTES`].
    BadString,
    /// A list held more items than its limit allows (or than the bytes left could hold).
    TooMany {
        /// What the list holds.
        what: &'static str,
        /// The count on the wire.
        count: u64,
    },
    /// A tag or enumerated field held a value this version does not know.
    BadTag {
        /// The field.
        what: &'static str,
        /// The value on the wire.
        value: u8,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "message is truncated"),
            Self::TrailingBytes => write!(f, "message has trailing bytes"),
            Self::BadInteger => write!(f, "malformed integer"),
            Self::BadString => write!(f, "string is not UTF-8 or too long"),
            Self::TooMany { what, count } => write!(f, "too many {what}: {count}"),
            Self::BadTag { what, value } => write!(f, "unknown {what}: {value:#04x}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Appends encoded fields to a buffer.
#[derive(Default)]
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub(crate) fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub(crate) fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    /// Unsigned LEB128.
    pub(crate) fn uv(&mut self, mut v: u64) {
        loop {
            // Masked to 7 bits, so the cast is exact.
            #[allow(clippy::cast_possible_truncation)]
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }

    pub(crate) fn u32v(&mut self, v: u32) {
        self.uv(u64::from(v));
    }

    pub(crate) fn len(&mut self, n: usize) {
        // usize is at most 64 bits on every supported target.
        self.uv(n as u64);
    }

    pub(crate) fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub(crate) fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub(crate) fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    pub(crate) fn str(&mut self, v: &str) {
        self.len(v.len());
        self.buf.extend_from_slice(v.as_bytes());
    }
}

/// Reads fields from a message, failing (never panicking) on malformed input.
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    /// Fails unless every byte was consumed.
    pub(crate) fn finish(&self) -> Result<(), DecodeError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }

    pub(crate) fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let (head, tail) = self
            .rest
            .split_first_chunk::<N>()
            .ok_or(DecodeError::Truncated)?;
        self.rest = tail;
        Ok(*head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take::<1>()?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(DecodeError::BadTag {
                what: "bool",
                value,
            }),
        }
    }

    /// Unsigned LEB128, at most 10 bytes. Overlong encodings of small values are accepted, as
    /// they decode to one value unambiguously.
    pub(crate) fn uv(&mut self) -> Result<u64, DecodeError> {
        let mut v: u64 = 0;
        for i in 0..10u32 {
            let byte = self.u8()?;
            let bits = u64::from(byte & 0x7f);
            // The 10th byte holds bit 63 only.
            if i == 9 && bits > 1 {
                return Err(DecodeError::BadInteger);
            }
            v |= bits << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(DecodeError::BadInteger)
    }

    pub(crate) fn u32v(&mut self) -> Result<u32, DecodeError> {
        u32::try_from(self.uv()?).map_err(|_| DecodeError::BadInteger)
    }

    /// A list count, bounded by `max` and by the bytes left (every item is at least one byte),
    /// so a forged count cannot make the receiver allocate.
    pub(crate) fn count(&mut self, what: &'static str, max: usize) -> Result<usize, DecodeError> {
        let count = self.uv()?;
        match usize::try_from(count) {
            Ok(n) if n <= max && n <= self.rest.len() => Ok(n),
            _ => Err(DecodeError::TooMany { what, count }),
        }
    }

    pub(crate) fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.take::<4>()?))
    }

    pub(crate) fn f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_le_bytes(self.take::<8>()?))
    }

    pub(crate) fn str(&mut self) -> Result<String, DecodeError> {
        let len = usize::try_from(self.uv()?).map_err(|_| DecodeError::BadString)?;
        if len > MAX_STR_BYTES {
            return Err(DecodeError::BadString);
        }
        if len > self.rest.len() {
            return Err(DecodeError::Truncated);
        }
        let (head, tail) = self.rest.split_at(len);
        self.rest = tail;
        String::from_utf8(head.to_vec()).map_err(|_| DecodeError::BadString)
    }

    /// A list of at most `max` items, each read by `item`.
    pub(crate) fn list<T>(
        &mut self,
        what: &'static str,
        max: usize,
        mut item: impl FnMut(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Vec<T>, DecodeError> {
        let n = self.count(what, max)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(item(self)?);
        }
        Ok(out)
    }
}

/// A one-byte enumeration on the wire: `#[repr(u8)]` style enums with checked decoding.
macro_rules! wire_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident ($what:literal) {
        $($(#[$vmeta:meta])* $variant:ident = $value:literal,)+
    }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant,)+
        }

        impl $name {
            /// The wire value.
            #[must_use]
            pub fn to_u8(self) -> u8 {
                match self {
                    $(Self::$variant => $value,)+
                }
            }

            /// Decodes a wire value.
            pub fn from_u8(value: u8) -> Result<Self, $crate::DecodeError> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err($crate::DecodeError::BadTag { what: $what, value }),
                }
            }
        }
    };
}

pub(crate) use wire_enum;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_round_trips_edges() {
        for v in [
            0,
            1,
            127,
            128,
            300,
            u64::from(u32::MAX),
            u64::MAX - 1,
            u64::MAX,
        ] {
            let mut w = Writer::default();
            w.uv(v);
            let bytes = w.into_bytes();
            let mut r = Reader::new(&bytes);
            assert_eq!(r.uv().unwrap(), v);
            r.finish().unwrap();
        }
    }

    #[test]
    fn leb128_rejects_overflow_and_overlength() {
        // 10th byte carrying more than bit 63.
        let mut too_big = vec![0xff; 9];
        too_big.push(0x02);
        assert_eq!(Reader::new(&too_big).uv(), Err(DecodeError::BadInteger));
        // 11 continuation bytes.
        assert_eq!(Reader::new(&[0x80; 11]).uv(), Err(DecodeError::BadInteger));
        assert_eq!(Reader::new(&[0x80]).uv(), Err(DecodeError::Truncated));
    }

    #[test]
    fn counts_cannot_outrun_the_message() {
        // A count of 2^40 with three bytes left must fail before allocating.
        let mut w = Writer::default();
        w.uv(1 << 40);
        w.bytes(&[0, 0, 0]);
        let bytes = w.into_bytes();
        let err = Reader::new(&bytes).list("cells", usize::MAX, Reader::u8);
        assert!(matches!(err, Err(DecodeError::TooMany { .. })));
    }

    #[test]
    fn strings_are_checked() {
        let mut w = Writer::default();
        w.uv(2);
        w.bytes(&[0xc3, 0x28]); // invalid UTF-8
        assert_eq!(
            Reader::new(&w.into_bytes()).str(),
            Err(DecodeError::BadString)
        );
        let mut w = Writer::default();
        w.len(MAX_STR_BYTES + 1);
        assert_eq!(
            Reader::new(&w.into_bytes()).str(),
            Err(DecodeError::BadString)
        );
    }
}
