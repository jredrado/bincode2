use serde;
use core2::io::{Read, Write};
use core::marker::PhantomData;

use config::{Options, OptionsExt};
use de::read::BincodeRead;
use core::convert::TryFrom;
use core::convert::TryInto;
use {ErrorKind, Result};

use alloc::boxed::Box;
use alloc::vec::Vec;

#[derive(Clone)]
struct CountSize<L: SizeLimit> {
    total: u64,
    other_limit: L,
}

pub(crate) fn serialize_into<W, T: ?Sized, O>(writer: W, value: &T, mut options: O) -> Result<()>
where
    W: Write,
    T: serde::Serialize,
    O: Options,
{
    if options.limit().limit().is_some() {
        // "compute" the size for the side-effect
        // of returning Err if the bound was reached.
        serialized_size(value, &mut options)?;
    }

    let mut serializer = ::ser::Serializer::<_, O>::new(writer, options);
    serde::Serialize::serialize(value, &mut serializer)
}

pub(crate) fn serialize<T: ?Sized, O>(value: &T, mut options: O) -> Result<Vec<u8>>
where
    T: serde::Serialize,
    O: Options,
{
    let mut writer = {
        let actual_size = serialized_size(value, &mut options)?;
        Vec::with_capacity(actual_size as usize)
    };

    serialize_into(&mut writer, value, options.with_no_limit())?;
    Ok(writer)
}

impl<L: SizeLimit> SizeLimit for CountSize<L> {
    fn add(&mut self, c: u64) -> Result<()> {
        self.other_limit.add(c)?;
        self.total += c;
        Ok(())
    }

    fn limit(&self) -> Option<u64> {
        unreachable!();
    }
}

pub(crate) fn serialized_size<T: ?Sized, O: Options>(value: &T, mut options: O) -> Result<u64>
where
    T: serde::Serialize,
{
    let old_limiter = options.limit().clone();
    let mut size_counter = ::ser::SizeChecker {
        options: ::config::WithOtherLimit::new(
            options,
            CountSize {
                total: 0,
                other_limit: old_limiter,
            },
        ),
    };

    let result = value.serialize(&mut size_counter);
    result.map(|_| size_counter.options.new_limit.total)
}

pub(crate) fn deserialize_from<R, T, O>(reader: R, options: O) -> Result<T>
where
    R: Read,
    T: serde::de::DeserializeOwned,
    O: Options,
{
    deserialize_from_seed(PhantomData, reader, options)
}

pub(crate) fn deserialize_from_seed<'a, R, T, O>(seed: T, reader: R, options: O) -> Result<T::Value>
where
    R: Read,
    T: serde::de::DeserializeSeed<'a>,
    O: Options,
{
    let reader = ::de::read::IoReader::new(reader);
    deserialize_from_custom_seed(seed, reader, options)
}

pub(crate) fn deserialize_from_custom<'a, R, T, O>(reader: R, options: O) -> Result<T>
where
    R: BincodeRead<'a>,
    T: serde::de::DeserializeOwned,
    O: Options,
{
    deserialize_from_custom_seed(PhantomData, reader, options)
}

pub(crate) fn deserialize_from_custom_seed<'a, R, T, O>(
    seed: T,
    reader: R,
    options: O,
) -> Result<T::Value>
where
    R: BincodeRead<'a>,
    T: serde::de::DeserializeSeed<'a>,
    O: Options,
{
    let mut deserializer = ::de::Deserializer::<_, O>::new(reader, options);
    seed.deserialize(&mut deserializer)
}

pub(crate) fn deserialize_in_place<'a, R, T, O>(reader: R, options: O, place: &mut T) -> Result<()>
where
    R: BincodeRead<'a>,
    T: serde::de::Deserialize<'a>,
    O: Options,
{
    let mut deserializer = ::de::Deserializer::<_, _>::new(reader, options);
    serde::Deserialize::deserialize_in_place(&mut deserializer, place)
}

pub(crate) fn deserialize<'a, T, O>(bytes: &'a [u8], options: O) -> Result<T>
where
    T: serde::de::Deserialize<'a>,
    O: Options,
{
    deserialize_seed(PhantomData, bytes, options)
}

pub(crate) fn deserialize_seed<'a, T, O>(seed: T, bytes: &'a [u8], options: O) -> Result<T::Value>
where
    T: serde::de::DeserializeSeed<'a>,
    O: Options,
{
    let reader = ::de::read::SliceReader::new(bytes);
    let options = ::config::WithOtherLimit::new(options, Infinite);
    deserialize_from_custom_seed(seed, reader, options)
}

pub(crate) trait SizeLimit: Clone {
    /// Tells the SizeLimit that a certain number of bytes has been
    /// read or written.  Returns Err if the limit has been exceeded.
    fn add(&mut self, n: u64) -> Result<()>;
    /// Returns the hard limit (if one exists)
    fn limit(&self) -> Option<u64>;
}

/// A SizeLimit that restricts serialized or deserialized messages from
/// exceeding a certain byte length.
#[derive(Copy, Clone)]
pub struct Bounded(pub u64);

/// A SizeLimit without a limit!
/// Use this if you don't care about the size of encoded or decoded messages.
#[derive(Copy, Clone)]
pub struct Infinite;

impl SizeLimit for Bounded {
    #[inline(always)]
    fn add(&mut self, n: u64) -> Result<()> {
        if self.0 >= n {
            self.0 -= n;
            Ok(())
        } else {
            Err(Box::new(ErrorKind::SizeLimit))
        }
    }

    #[inline(always)]
    fn limit(&self) -> Option<u64> {
        Some(self.0)
    }
}

impl SizeLimit for Infinite {
    #[inline(always)]
    fn add(&mut self, _: u64) -> Result<()> {
        Ok(())
    }

    #[inline(always)]
    fn limit(&self) -> Option<u64> {
        None
    }
}

pub(crate) trait SizeType: Clone {
    type Primitive: serde::de::DeserializeOwned + TryFrom<usize> + Into<u64>;

    fn read(reader: &mut dyn FnMut() -> Result<Self::Primitive>) -> Result<u64> {
        let result: Self::Primitive = reader()?;
        Ok(result.into())
    }

    fn write<S>(writer: S, value: usize) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>,
    {
        let value: Self::Primitive = value.try_into().map_err(|_e| ErrorKind::SizeTypeLimit)?;
        Self::write_to(writer, value)
    }

    fn write_to<S>(writer: S, value: Self::Primitive) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>;
}

/// An 8 byte length
#[derive(Copy, Clone)]
pub struct U64;
impl SizeType for U64 {
    type Primitive = u64;
    fn write_to<S>(writer: S, value: Self::Primitive) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>,
    {
        writer.serialize_u64(value).map_err(Into::into)
    }
}

/// A 4 byte length
#[derive(Copy, Clone)]
pub struct U32;
impl SizeType for U32 {
    type Primitive = u32;
    fn write_to<S>(writer: S, value: Self::Primitive) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>,
    {
        writer.serialize_u32(value).map_err(Into::into)
    }
}

/// A 2 byte length
#[derive(Copy, Clone)]
pub struct U16;
impl SizeType for U16 {
    type Primitive = u16;
    fn write_to<S>(writer: S, value: Self::Primitive) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>,
    {
        writer.serialize_u16(value).map_err(Into::into)
    }
}

/// A 1 byte length
#[derive(Copy, Clone)]
pub struct U8;
impl SizeType for U8 {
    type Primitive = u8;
    fn write_to<S>(writer: S, value: Self::Primitive) -> Result<S::Ok>
    where
        S: serde::Serializer,
        Box<ErrorKind>: From<S::Error>,
    {
        writer.serialize_u8(value).map_err(Into::into)
    }
}
