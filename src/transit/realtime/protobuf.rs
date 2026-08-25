//! Minimal protobuf wire-format reader.
//!
//! GTFS-Realtime is protobuf, but pulling in `prost` would mean a `prost-build`
//! step that shells out to `protoc` — a binary absent from both this machine
//! and the CI runners, and the kind of external toolchain the project already
//! avoids elsewhere (rustls rather than native-tls, for the same reason).
//!
//! The subset of the wire format GTFS-Realtime actually uses is small: varints,
//! length-delimited bytes, and the two fixed-width forms. Decoding it directly
//! costs ~150 lines and keeps the dependency tree unchanged.
//!
//! This is a *reader*, not a full implementation: it walks fields in order and
//! hands each one back as a [`WireValue`]. Unknown fields cost nothing to skip,
//! which is what makes the decoder forward-compatible with feed extensions.

/// Why a buffer could not be walked as protobuf.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The buffer ended in the middle of a field.
    Truncated,
    /// A varint ran past 10 bytes, so it cannot fit in 64 bits.
    VarintOverflow,
    /// Wire types 3 and 4 (deprecated groups) are not supported; anything
    /// else means the buffer is not protobuf at all.
    UnknownWireType(u8),
    /// A length-delimited field held bytes that are not valid UTF-8.
    InvalidUtf8,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "buffer ended mid-field"),
            Self::VarintOverflow => write!(f, "varint longer than 64 bits"),
            Self::UnknownWireType(w) => write!(f, "unsupported wire type {w}"),
            Self::InvalidUtf8 => write!(f, "string field is not valid UTF-8"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// One field's payload, still untyped: protobuf does not record whether a
/// varint meant an `int32`, a `bool` or an enum, so interpretation is the
/// caller's job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireValue<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

impl<'a> WireValue<'a> {
    /// Interpret a varint as an unsigned integer.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Varint(v) => Some(*v),
            _ => None,
        }
    }

    /// Interpret a varint as a `uint32`.
    pub fn as_u32(&self) -> Option<u32> {
        self.as_u64().map(|v| v as u32)
    }

    /// Interpret a varint as an `int64`.
    ///
    /// Protobuf encodes negative `int32`/`int64` as their two's-complement
    /// 64-bit value, which is why this is a plain cast and not a zigzag decode
    /// (zigzag applies to `sint32`/`sint64`, which GTFS-Realtime never uses).
    pub fn as_i64(&self) -> Option<i64> {
        self.as_u64().map(|v| v as i64)
    }

    /// Interpret a varint as an `int32`, preserving negative values.
    pub fn as_i32(&self) -> Option<i32> {
        self.as_i64().map(|v| v as i32)
    }

    /// Borrow a length-delimited payload (a string, or a nested message).
    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match self {
            Self::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Borrow a length-delimited payload as a UTF-8 string.
    pub fn as_str(&self) -> Result<Option<&'a str>, DecodeError> {
        match self.as_bytes() {
            None => Ok(None),
            Some(b) => std::str::from_utf8(b)
                .map(Some)
                .map_err(|_| DecodeError::InvalidUtf8),
        }
    }
}

/// Walks the fields of one protobuf message.
///
/// Nested messages are decoded by handing their payload to a new [`Reader`],
/// which is why the borrow is tied to the original buffer: nothing is copied
/// during decoding.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Read the next `(field_number, value)` pair, or `None` at end of message.
    pub fn next_field(&mut self) -> Result<Option<(u32, WireValue<'a>)>, DecodeError> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.read_varint()?;
        let field_number = (key >> 3) as u32;
        let wire_type = (key & 0b111) as u8;

        let value = match wire_type {
            0 => WireValue::Varint(self.read_varint()?),
            1 => WireValue::Fixed64(u64::from_le_bytes(self.read_fixed::<8>()?)),
            2 => {
                let len = self.read_varint()? as usize;
                WireValue::Bytes(self.read_slice(len)?)
            }
            5 => WireValue::Fixed32(u32::from_le_bytes(self.read_fixed::<4>()?)),
            other => return Err(DecodeError::UnknownWireType(other)),
        };
        Ok(Some((field_number, value)))
    }

    /// Base-128 varint, little-endian groups of 7 bits, high bit = continue.
    fn read_varint(&mut self) -> Result<u64, DecodeError> {
        let mut result = 0u64;
        let mut shift = 0u32;
        loop {
            if shift >= 64 {
                return Err(DecodeError::VarintOverflow);
            }
            let byte = *self.buf.get(self.pos).ok_or(DecodeError::Truncated)?;
            self.pos += 1;
            // At shift 63 only the low bit can still land inside the u64; the
            // rest is discarded by the shift, matching protobuf's own
            // truncation of over-long varints.
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
        }
    }

    fn read_slice(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(len).ok_or(DecodeError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let slice = self.read_slice(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }
}

/// Protobuf *writer*, for tests only.
///
/// Building fixtures by hand-writing byte arrays is unreadable and makes the
/// GTFS-Realtime connector tests impossible to review. Encoding is the exact
/// inverse of [`Reader`], so a fixture that round-trips also exercises the
/// reader.
#[cfg(test)]
pub mod encoding {
    use super::WireValue;

    /// Encode a base-128 varint.
    fn varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    /// Encode a `(field_number, wire_type)` key.
    fn key(field: u32, wire_type: u8, out: &mut Vec<u8>) {
        varint((u64::from(field) << 3) | u64::from(wire_type), out);
    }

    /// Encode a whole message from its fields, in the given order.
    pub fn message(fields: &[(u32, WireValue<'_>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (field, value) in fields {
            match value {
                WireValue::Varint(v) => {
                    key(*field, 0, &mut out);
                    varint(*v, &mut out);
                }
                WireValue::Fixed64(v) => {
                    key(*field, 1, &mut out);
                    out.extend_from_slice(&v.to_le_bytes());
                }
                WireValue::Bytes(b) => {
                    key(*field, 2, &mut out);
                    varint(b.len() as u64, &mut out);
                    out.extend_from_slice(b);
                }
                WireValue::Fixed32(v) => {
                    key(*field, 5, &mut out);
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::encoding::message;
    use super::*;

    fn fields(buf: &[u8]) -> Vec<(u32, WireValue<'_>)> {
        let mut reader = Reader::new(buf);
        let mut out = Vec::new();
        while let Some(field) = reader.next_field().expect("valid fixture") {
            out.push(field);
        }
        out
    }

    #[test]
    fn reads_every_supported_wire_type() {
        let buf = message(&[
            (1, WireValue::Varint(300)),
            (2, WireValue::Bytes(b"hello")),
            (3, WireValue::Fixed32(7)),
            (4, WireValue::Fixed64(9)),
        ]);
        assert_eq!(
            fields(&buf),
            vec![
                (1, WireValue::Varint(300)),
                (2, WireValue::Bytes(b"hello")),
                (3, WireValue::Fixed32(7)),
                (4, WireValue::Fixed64(9)),
            ]
        );
    }

    #[test]
    fn empty_buffer_yields_no_fields() {
        assert!(Reader::new(&[]).next_field().unwrap().is_none());
    }

    #[test]
    fn negative_int32_round_trips_through_the_two_complement_encoding() {
        // protoc encodes -30 as the 10-byte two's-complement varint.
        let buf = message(&[(5, WireValue::Varint((-30i64) as u64))]);
        let value = fields(&buf)[0].1;
        assert_eq!(value.as_i32(), Some(-30));
        assert_eq!(value.as_i64(), Some(-30));
    }

    #[test]
    fn strings_are_borrowed_not_copied() {
        let buf = message(&[(1, WireValue::Bytes("Châtelet".as_bytes()))]);
        assert_eq!(fields(&buf)[0].1.as_str().unwrap(), Some("Châtelet"));
    }

    #[test]
    fn invalid_utf8_is_reported_rather_than_replaced() {
        let buf = message(&[(1, WireValue::Bytes(&[0xff, 0xfe]))]);
        assert_eq!(fields(&buf)[0].1.as_str(), Err(DecodeError::InvalidUtf8));
    }

    #[test]
    fn truncated_length_delimited_field_is_rejected() {
        let mut buf = message(&[(1, WireValue::Bytes(b"hello"))]);
        buf.truncate(buf.len() - 2);
        assert_eq!(Reader::new(&buf).next_field(), Err(DecodeError::Truncated));
    }

    #[test]
    fn truncated_varint_is_rejected() {
        // A key byte with the continuation bit set and nothing after it.
        assert_eq!(
            Reader::new(&[0x80]).next_field(),
            Err(DecodeError::Truncated)
        );
    }

    #[test]
    fn overlong_varint_is_rejected() {
        let buf = [
            0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
        ];
        assert_eq!(
            Reader::new(&buf).next_field(),
            Err(DecodeError::VarintOverflow)
        );
    }

    #[test]
    fn deprecated_group_wire_types_are_rejected() {
        let buf = [(1u32 << 3 | 3) as u8];
        assert_eq!(
            Reader::new(&buf).next_field(),
            Err(DecodeError::UnknownWireType(3))
        );
    }

    #[test]
    fn type_accessors_reject_mismatched_wire_types() {
        assert_eq!(WireValue::Varint(1).as_bytes(), None);
        assert_eq!(WireValue::Bytes(b"x").as_u64(), None);
        assert_eq!(WireValue::Varint(1).as_str(), Ok(None));
    }
}
