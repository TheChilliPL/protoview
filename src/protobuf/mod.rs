use crate::protobuf::types::ProtobufType;
use derive_more::with_trait::From;
use eyre::{WrapErr, bail, eyre};
use schema::ProtobufSchema;
use std::iter::FusedIterator;
use std::ops::Deref;
use wire_types::{WireType, WireValue};

pub mod schema;
pub mod types;
pub mod wire_types;

#[derive(Clone)]
pub struct ProtobufDecoder<'a> {
    data: &'a [u8],
    schema: Option<&'a ProtobufSchema>,
    at_path: Option<String>,
}

impl<'a> ProtobufDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        ProtobufDecoder {
            data,
            schema: None,
            at_path: None,
        }
    }

    pub fn with_schema(data: &'a [u8], schema: Option<&'a ProtobufSchema>) -> Self {
        ProtobufDecoder {
            data,
            schema,
            at_path: schema
                .and_then(|s| s.default_message_type())
                .map(|s| s.to_string()),
        }
    }

    pub fn with_schema_at_path(
        data: &'a [u8],
        schema: Option<&'a ProtobufSchema>,
        at_path: Option<String>,
    ) -> Self {
        ProtobufDecoder {
            data,
            schema,
            at_path,
        }
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn schema(&self) -> Option<&ProtobufSchema> {
        self.schema
    }

    fn fuse(&mut self) {
        self.data = &self.data[..0];
    }

    fn take_byte(&mut self) -> eyre::Result<u8> {
        let Some(byte) = self.data.first() else {
            self.fuse();
            bail!("unexpected end of input");
        };
        self.data = &self.data[1..];
        Ok(*byte)
    }

    fn take_n_bytes<const N: usize>(&mut self) -> eyre::Result<[u8; N]> {
        Ok(self.take_slice(N)?.try_into()?)
    }

    fn take_slice(&mut self, n: usize) -> eyre::Result<&'a [u8]> {
        let Some(bytes) = self.data.get(..n) else {
            self.fuse();
            bail!("unexpected end of input");
        };
        self.data = &self.data[n..];
        Ok(bytes)
    }

    /// May panic if the data is empty
    fn decode_unchecked(&mut self) -> eyre::Result<DecodedEntry<'a>> {
        let (field_number, wire_type) = self.decode_tag();

        let range_at_start = self.data;

        let at = self.at_path.as_deref().unwrap_or_default();
        let parent_message = self.schema.and_then(|s| s.resolve_message_absolute(at));
        let field = parent_message
            .as_ref()
            .and_then(|(_, p)| p.field.iter().find(|f| f.number() == field_number as i32));
        let field_type = field.and_then(|f| f.r#type).and_then(|it| it.try_into().ok());
        let field_type_name = field.and_then(|f| f.type_name.as_deref());
        let field_type_message = match field_type {
            Some(ProtobufType::Message) => {
                field_type_name.and_then(|n| {
                    self.schema
                        .unwrap()
                        .resolve_message(&parent_message.as_ref().unwrap().0, n)
                        .map(|x| x.0)
                })
            }
            Some(ProtobufType::Enum) => {
                field_type_name.and_then(|n| {
                    self.schema
                        .unwrap()
                        .resolve_enum(&parent_message.as_ref().unwrap().0, n)
                        .map(|x| x.0)
                })
            },
            _ => None,
        };

        let value = match wire_type {
            Ok(WireType::VarInt) => WireValue::VarInt(self.decode_varint()?.into()),
            Ok(WireType::I64) => WireValue::I64(self.decode_i64()?.into()),
            Ok(WireType::LengthDelimited) => {
                WireValue::LengthDelimited(self.decode_length_delimited()?.into())
            }
            Ok(WireType::StartGroup) => WireValue::StartGroup,
            Ok(WireType::EndGroup) => WireValue::EndGroup,
            Ok(WireType::I32) => WireValue::I32(self.decode_i32()?.into()),
            Err(e) => {
                self.fuse();
                return Err(e);
            }
        };

        let data_range = &range_at_start[..range_at_start.len() - self.data.len()];

        Ok(DecodedEntry {
            range: data_range,
            field_number,
            value,
            name: field.and_then(|f| f.name.clone()),
            protobuf_type: field_type,
            protobuf_type_name: field_type_message,
        })
    }

    /// May panic if the data is empty
    fn decode_tag(&mut self) -> (u8, eyre::Result<WireType>) {
        let tag_byte = self.data[0];
        self.data = &self.data[1..];

        let field_num = tag_byte >> 3;
        let wire_type = WireType::try_from(tag_byte & 0b111)
            .inspect_err(|_| self.fuse())
            .map_err(Into::into);

        (field_num, wire_type)
    }

    fn decode_varint(&mut self) -> eyre::Result<u64> {
        let mut result: u64 = 0;

        let mut i: usize = 0;
        while let Ok(byte) = self.take_byte() {
            result |= (byte as u64 & 0x7F)
                .checked_shl(7 * i as u32)
                .ok_or_else(|| {
                    self.fuse();
                    eyre!("varint overflow")
                })?;

            i += 1;

            if (byte & 0x80) == 0 {
                return Ok(result);
            }
        }

        Err(eyre!("unexpected end of varint"))
    }

    fn decode_i64(&mut self) -> eyre::Result<u64> {
        Ok(u64::from_le_bytes(self.take_n_bytes::<8>()?))
    }

    fn decode_i32(&mut self) -> eyre::Result<u32> {
        Ok(u32::from_le_bytes(self.take_n_bytes::<4>()?))
    }

    fn decode_length_delimited(&mut self) -> eyre::Result<&'a [u8]> {
        let length: usize = self
            .decode_varint()
            .wrap_err("error decoding length")
            .and_then(|it| it.try_into().wrap_err("length does not fit in usize"))
            .inspect_err(|_| self.fuse())?;
        self.take_slice(length)
    }
}

impl<'a> Iterator for ProtobufDecoder<'a> {
    type Item = eyre::Result<DecodedEntry<'a>>;

    /// Decodes the next direct entry.
    ///
    /// May fuse after returning a decoding error (if previous element length is unknown).
    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            return None;
        }

        let decode = self.decode_unchecked();

        Some(decode)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.data.is_empty() {
            (0, Some(0))
        } else {
            // Either 1..n entries or 0..(n-1) valid entries and an error.
            (1, Some(self.data.len()))
        }
    }
}

impl FusedIterator for ProtobufDecoder<'_> {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedEntry<'a> {
    /// Inner range of the entry data (without its tag byte, but for [WireType::LengthDelimited] with its length)
    pub range: &'a [u8],
    pub field_number: u8,
    pub value: WireValue<'a>,
    pub name: Option<String>,
    pub protobuf_type: Option<ProtobufType>,
    pub protobuf_type_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{wire_types::*, *};
    use crate::protobuf::wire_types::{I32, I64, LengthDelimited, VarInt};
    use std::assert_matches;

    #[test]
    fn decode_empty() {
        let mut decoder = ProtobufDecoder::new(&[]);

        assert_eq!(decoder.data.len(), 0);
        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_varint() {
        const DATA: &[u8] = &[0x08, 0x96, 0x01];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_eq!(decoder.data.len(), 3);
        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::VarInt(VarInt(150)),
            ..
        } if range == &DATA[1..]);
        assert_eq!(decoder.data.len(), 0);
        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_varint_incomplete() {
        const DATA: &[u8] = &[0x08, 0x80];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert!(decoder.next().unwrap().is_err());
    }

    #[test]
    fn decode_varint_max() {
        const DATA: &[u8] = &[
            0x08, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, // u32::MAX
            0x10, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01,
        ];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::VarInt(VarInt(value)),
            ..
        } if range == &DATA[1..6] && value == u32::MAX as u64);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 2,
            value: WireValue::VarInt(VarInt(value)),
            ..
        } if range == &DATA[7..] && value == u64::MAX);

        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_varint_timestamp() {
        const DATA: &[u8] = &[0x08, 0xE0, 0x99, 0xBA, 0xD6, 0x06];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_eq!(decoder.data.len(), 6);
        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::VarInt(VarInt(value)),
            ..
        } if range == &DATA[1..] && value == 1791921376);

        assert_eq!(decoder.data.len(), 0);
        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_i32_i64() {
        const DATA: &[u8] = &[
            0x0D, 0xFF, 0xFF, 0xFF, 0xFF, // u32::MAX
            0x11, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // u64::MAX
        ];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::I32(I32(value)),
            ..
        } if range == &DATA[1..5] && value == u32::MAX);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 2,
            value: WireValue::I64(I64(value)),
            ..
        } if range == &DATA[6..] && value == u64::MAX);

        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_len() {
        const DATA: &[u8] = &[0x0A, 0x05, b'h', b'e', b'l', b'l', b'o'];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::LengthDelimited(LengthDelimited(value)),
            ..
        } if range == &DATA[1..] && value == b"hello");

        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_empty_len() {
        const DATA: &[u8] = &[0x0A, 0x00];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_matches!(decoder.next().unwrap().unwrap(), DecodedEntry {
            range,
            field_number: 1,
            value: WireValue::LengthDelimited(LengthDelimited(value)),
            ..
        } if range == &DATA[1..] && value == &[]);

        assert!(decoder.next().is_none());
    }

    #[test]
    fn decode_len_truncated() {
        const DATA: &[u8] = &[
            0x0A, 0x80, // unfinished
        ];

        let mut decoder = ProtobufDecoder::new(DATA);

        assert_matches!(decoder.next().unwrap(), Err(e) if e.to_string() == "error decoding length");
        assert!(decoder.next().is_none());
    }
}
