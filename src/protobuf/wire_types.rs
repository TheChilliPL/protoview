use crate::protobuf::ProtobufDecoder;
use crate::protobuf::schema::ProtobufSchema;
use derive_more::{From, TryFrom};
use std::str::Utf8Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireValue<'a> {
    VarInt(VarInt),
    I64(I64),
    LengthDelimited(LengthDelimited<'a>),
    StartGroup,
    EndGroup,
    I32(I32),
}

impl WireValue<'_> {
    pub fn wire_type(&self) -> WireType {
        match self {
            WireValue::VarInt(_) => WireType::VarInt,
            WireValue::I64(_) => WireType::I64,
            WireValue::LengthDelimited(_) => WireType::LengthDelimited,
            WireValue::StartGroup => WireType::StartGroup,
            WireValue::EndGroup => WireType::EndGroup,
            WireValue::I32(_) => WireType::I32,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, From)]
pub struct VarInt(pub(super) u64);

impl VarInt {
    pub fn decode_zigzag(n: u64) -> i64 {
        ((n >> 1) as i64) ^ (-((n & 1) as i64))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn as_i64(self) -> i64 {
        self.0 as i64
    }

    pub fn as_s64(self) -> i64 {
        Self::decode_zigzag(self.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, From)]
pub struct I64(pub(super) u64);

impl I64 {
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn as_i64(self) -> i64 {
        self.0 as i64
    }

    pub fn as_f64(self) -> f64 {
        f64::from_bits(self.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, From)]
pub struct LengthDelimited<'a>(pub(super) &'a [u8]);

impl<'a> LengthDelimited<'a> {
    pub fn as_bytes(&self) -> &[u8] {
        self.0
    }

    pub fn try_as_str(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.0)
    }

    pub fn sub_decoder(
        &self,
        schema: Option<&'a ProtobufSchema>,
        path: Option<String>,
    ) -> ProtobufDecoder<'a> {
        ProtobufDecoder::with_schema_at_path(self.0, schema, path)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, From)]
pub struct I32(pub(super) u32);

impl I32 {
    pub fn as_u32(self) -> u32 {
        self.0
    }

    pub fn as_i32(self) -> i32 {
        self.0 as i32
    }

    pub fn as_f32(self) -> f32 {
        f32::from_bits(self.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum WireType {
    VarInt = 0,
    I64 = 1,
    LengthDelimited = 2,
    StartGroup = 3,
    EndGroup = 4,
    I32 = 5,
}
