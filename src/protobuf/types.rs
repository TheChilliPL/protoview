use derive_more::{TryFrom, TryFromReprError};

#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum ProtobufType {
    Double = 1,
    Float = 2,
    Int64 = 3,
    UInt64 = 4,
    Int32 = 5,
    Fixed64 = 6,
    Fixed32 = 7,
    Bool = 8,
    String = 9,
    Group = 10,
    Message = 11,
    Bytes = 12,
    UInt32 = 13,
    Enum = 14,
    SFixed32 = 15,
    SFixed64 = 16,
    SInt32 = 17,
    SInt64 = 18,
}

impl TryFrom<i32> for ProtobufType {
    type Error = TryFromReprError<i32>;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_from(value as u8).map_err(|_| TryFromReprError { input: value })
    }
}
