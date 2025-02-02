use super::internal::{Bounded, Infinite, SizeLimit, SizeType, U16, U32, U64, U8};
use byteorder::{BigEndian, ByteOrder, LittleEndian, NativeEndian};
use de::read::BincodeRead;
use error::Result;
use serde;
use core2::io::{Read, Write};
use core::marker::PhantomData;
use {DeserializerAcceptor, SerializerAcceptor};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

struct DefaultOptions(Infinite);

pub(crate) trait Options {
    type Limit: SizeLimit + 'static;
    type Endian: ByteOrder + 'static;
    type StringSize: SizeType + 'static;
    type ArraySize: SizeType + 'static;

    fn limit(&mut self) -> &mut Self::Limit;
}

pub(crate) trait OptionsExt: Options + Sized {
    fn with_no_limit(self) -> WithOtherLimit<Self, Infinite> {
        WithOtherLimit::new(self, Infinite)
    }

    fn with_limit(self, limit: u64) -> WithOtherLimit<Self, Bounded> {
        WithOtherLimit::new(self, Bounded(limit))
    }

    fn with_little_endian(self) -> WithOtherEndian<Self, LittleEndian> {
        WithOtherEndian::new(self)
    }

    fn with_big_endian(self) -> WithOtherEndian<Self, BigEndian> {
        WithOtherEndian::new(self)
    }

    fn with_native_endian(self) -> WithOtherEndian<Self, NativeEndian> {
        WithOtherEndian::new(self)
    }

    fn with_string_size<S>(self) -> WithOtherStringLength<Self, S>
    where
        S: SizeType,
    {
        WithOtherStringLength::new(self)
    }

    fn with_array_size<S>(self) -> WithOtherArrayLength<Self, S>
    where
        S: SizeType,
    {
        WithOtherArrayLength::new(self)
    }
}

impl<'a, O: Options> Options for &'a mut O {
    type Limit = O::Limit;
    type Endian = O::Endian;
    type StringSize = O::StringSize;
    type ArraySize = O::ArraySize;

    #[inline(always)]
    fn limit(&mut self) -> &mut Self::Limit {
        (*self).limit()
    }
}

impl<T: Options> OptionsExt for T {}

impl DefaultOptions {
    fn new() -> DefaultOptions {
        DefaultOptions(Infinite)
    }
}

impl Options for DefaultOptions {
    type Limit = Infinite;
    type Endian = LittleEndian;
    type StringSize = U64;
    type ArraySize = U64;

    #[inline(always)]
    fn limit(&mut self) -> &mut Infinite {
        &mut self.0
    }
}

#[derive(Clone, Copy, Debug)]
enum LimitOption {
    Unlimited,
    Limited(u64),
}

#[derive(Clone, Copy, Debug)]
enum EndianOption {
    Big,
    Little,
    Native,
}

/// Used to specify the unit used for length of strings and arrays via `config.string_length` or `config.array_length`.
#[derive(Clone, Copy, Debug)]
pub enum LengthOption {
    ///64 unsigned bits
    U64,
    ///32 unsigned bits
    U32,
    ///16 unsigned bits
    U16,
    ///8 unsigned bits
    U8,
}

/// A configuration builder whose options Bincode will use
/// while serializing and deserializing.
///
/// ### Options
/// Endianness: The endianness with which multi-byte integers will be read/written.  *default: little endian*
/// Limit: The maximum number of bytes that will be read/written in a bincode serialize/deserialize. *default: unlimited*
///
/// ### Byte Limit Details
/// The purpose of byte-limiting is to prevent Denial-Of-Service attacks whereby malicious attackers get bincode
/// deserialization to crash your process by allocating too much memory or keeping a connection open for too long.
///
/// When a byte limit is set, bincode will return `Err` on any deserialization that goes over the limit, or any
/// serialization that goes over the limit.
///
/// ### Array and String sizes
/// When writing a string or an array is serialized the length is written at the beginning so that the data
/// can be deserialized. The option is a way to configure how this length is encoded. The default for both
/// is `U64`.
///
/// If a string or array is attempted to be serialized that is not fit within the type specified bincode will return `Err`
/// on serialization.
#[derive(Clone, Debug)]
pub struct Config {
    limit: LimitOption,
    endian: EndianOption,
    string_size: LengthOption,
    array_size: LengthOption,
}

pub(crate) struct WithOtherLimit<O: Options, L: SizeLimit> {
    _options: O,
    pub(crate) new_limit: L,
}

pub(crate) struct WithOtherEndian<O: Options, E: ByteOrder> {
    options: O,
    _endian: PhantomData<E>,
}

pub(crate) struct WithOtherStringLength<O: Options, L: SizeType> {
    options: O,
    _new_string_length: PhantomData<L>,
}

pub(crate) struct WithOtherArrayLength<O: Options, L: SizeType> {
    options: O,
    _new_array_length: PhantomData<L>,
}

impl<O: Options, L: SizeLimit> WithOtherLimit<O, L> {
    #[inline(always)]
    pub(crate) fn new(options: O, limit: L) -> WithOtherLimit<O, L> {
        WithOtherLimit {
            _options: options,
            new_limit: limit,
        }
    }
}

impl<O: Options, E: ByteOrder> WithOtherEndian<O, E> {
    #[inline(always)]
    pub(crate) fn new(options: O) -> WithOtherEndian<O, E> {
        WithOtherEndian {
            options,
            _endian: PhantomData,
        }
    }
}

impl<O: Options, L: SizeType> WithOtherStringLength<O, L> {
    #[inline(always)]
    pub(crate) fn new(options: O) -> WithOtherStringLength<O, L> {
        WithOtherStringLength {
            options,
            _new_string_length: PhantomData,
        }
    }
}

impl<O: Options, L: SizeType> WithOtherArrayLength<O, L> {
    #[inline(always)]
    pub(crate) fn new(options: O) -> WithOtherArrayLength<O, L> {
        WithOtherArrayLength {
            options,
            _new_array_length: PhantomData,
        }
    }
}

impl<O: Options, E: ByteOrder + 'static> Options for WithOtherEndian<O, E> {
    type Limit = O::Limit;
    type Endian = E;
    type StringSize = O::StringSize;
    type ArraySize = O::ArraySize;

    #[inline(always)]
    fn limit(&mut self) -> &mut O::Limit {
        self.options.limit()
    }
}

impl<O: Options, L: SizeLimit + 'static> Options for WithOtherLimit<O, L> {
    type Limit = L;
    type Endian = O::Endian;
    type StringSize = O::StringSize;
    type ArraySize = O::ArraySize;

    fn limit(&mut self) -> &mut L {
        &mut self.new_limit
    }
}

impl<O: Options, L: SizeType + 'static> Options for WithOtherStringLength<O, L> {
    type Limit = O::Limit;
    type Endian = O::Endian;
    type StringSize = L;
    type ArraySize = O::ArraySize;

    fn limit(&mut self) -> &mut O::Limit {
        self.options.limit()
    }
}

impl<O: Options, L: SizeType + 'static> Options for WithOtherArrayLength<O, L> {
    type Limit = O::Limit;
    type Endian = O::Endian;
    type StringSize = O::StringSize;
    type ArraySize = L;

    fn limit(&mut self) -> &mut O::Limit {
        self.options.limit()
    }
}

macro_rules! config_map_limit {
    ($self:expr, $opts:ident => $call:expr) => {
        match $self.limit {
            LimitOption::Unlimited => {
                let $opts = $opts.with_no_limit();
                $call
            }
            LimitOption::Limited(l) => {
                let $opts = $opts.with_limit(l);
                $call
            }
        }
    };
}

macro_rules! config_map_endian {
    ($self:expr, $opts:ident => $call:expr) => {
        match $self.endian {
            EndianOption::Little => {
                let $opts = $opts.with_little_endian();
                $call
            }
            EndianOption::Big => {
                let $opts = $opts.with_big_endian();
                $call
            }
            EndianOption::Native => {
                let $opts = $opts.with_native_endian();
                $call
            }
        }
    };
}

macro_rules! config_map_string_length {
    ($self:expr, $opts:ident => $call:expr) => {
        match $self.string_size {
            LengthOption::U64 => {
                let $opts = $opts.with_string_size::<U64>();
                $call
            }
            LengthOption::U32 => {
                let $opts = $opts.with_string_size::<U32>();
                $call
            }
            LengthOption::U16 => {
                let $opts = $opts.with_string_size::<U16>();
                $call
            }
            LengthOption::U8 => {
                let $opts = $opts.with_string_size::<U8>();
                $call
            }
        }
    };
}

macro_rules! config_map_array_length {
    ($self:expr, $opts:ident => $call:expr) => {
        match $self.array_size {
            LengthOption::U64 => {
                let $opts = $opts.with_array_size::<U64>();
                $call
            }
            LengthOption::U32 => {
                let $opts = $opts.with_array_size::<U32>();
                $call
            }
            LengthOption::U16 => {
                let $opts = $opts.with_array_size::<U16>();
                $call
            }
            LengthOption::U8 => {
                let $opts = $opts.with_array_size::<U8>();
                $call
            }
        }
    };
}

macro_rules! config_map {
    ($self:expr, $opts:ident => $call:expr) => {{
        let $opts = DefaultOptions::new();
        config_map_limit!($self, $opts =>
            config_map_endian!($self, $opts =>
                config_map_string_length!($self, $opts =>
                    config_map_array_length!($self, $opts => $call))))
    }}
}

#[allow(clippy::cognitive_complexity)] // https://github.com/rust-lang/rust-clippy/issues/3900
impl Config {
    #[inline(always)]
    pub(crate) fn new() -> Config {
        Config {
            limit: LimitOption::Unlimited,
            endian: EndianOption::Little,
            string_size: LengthOption::U64,
            array_size: LengthOption::U64,
        }
    }

    /// Sets the byte limit to be unlimited.
    /// This is the default.
    #[inline(always)]
    pub fn no_limit(&mut self) -> &mut Self {
        self.limit = LimitOption::Unlimited;
        self
    }

    /// Sets the byte limit to `limit`.
    #[inline(always)]
    pub fn limit(&mut self, limit: u64) -> &mut Self {
        self.limit = LimitOption::Limited(limit);
        self
    }

    /// Sets the endianness to little-endian
    /// This is the default.
    #[inline(always)]
    pub fn little_endian(&mut self) -> &mut Self {
        self.endian = EndianOption::Little;
        self
    }

    /// Sets the endianness to big-endian
    #[inline(always)]
    pub fn big_endian(&mut self) -> &mut Self {
        self.endian = EndianOption::Big;
        self
    }

    /// Sets the endianness to the the machine-native endianness
    #[inline(always)]
    pub fn native_endian(&mut self) -> &mut Self {
        self.endian = EndianOption::Native;
        self
    }

    /// Sets the size used for lengths of strings
    #[inline(always)]
    pub fn string_length(&mut self, size: LengthOption) -> &mut Self {
        self.string_size = size;
        self
    }

    /// Sets the size used for lengths of arrays
    #[inline(always)]
    pub fn array_length(&mut self, size: LengthOption) -> &mut Self {
        self.array_size = size;
        self
    }

    /// Serializes a serializable object into a `Vec` of bytes using this configuration
    #[inline(always)]
    pub fn serialize<T: ?Sized + serde::Serialize>(&self, t: &T) -> Result<Vec<u8>> {
        config_map!(self, opts => ::internal::serialize(t, opts))
    }

    /// Returns the size that an object would be if serialized using Bincode with this configuration
    #[inline(always)]
    pub fn serialized_size<T: ?Sized + serde::Serialize>(&self, t: &T) -> Result<u64> {
        config_map!(self, opts => ::internal::serialized_size(t, opts))
    }

    /// Serializes an object directly into a `Writer` using this configuration
    ///
    /// If the serialization would take more bytes than allowed by the size limit, an error
    /// is returned and *no bytes* will be written into the `Writer`
    #[inline(always)]
    pub fn serialize_into<W: Write, T: ?Sized + serde::Serialize>(
        &self,
        w: W,
        t: &T,
    ) -> Result<()> {
        config_map!(self, opts => ::internal::serialize_into(w, t, opts))
    }

    /// Deserializes a slice of bytes into an instance of `T` using this configuration
    #[inline(always)]
    pub fn deserialize<'a, T: serde::Deserialize<'a>>(&self, bytes: &'a [u8]) -> Result<T> {
        config_map!(self, opts => ::internal::deserialize(bytes, opts))
    }

    /// TODO: document
    #[doc(hidden)]
    #[inline(always)]
    pub fn deserialize_in_place<'a, R, T>(&self, reader: R, place: &mut T) -> Result<()>
    where
        R: BincodeRead<'a>,
        T: serde::de::Deserialize<'a>,
    {
        config_map!(self, opts => ::internal::deserialize_in_place(reader, opts, place))
    }

    /// Deserializes a slice of bytes with state `seed` using this configuration.
    #[inline(always)]
    pub fn deserialize_seed<'a, T: serde::de::DeserializeSeed<'a>>(
        &self,
        seed: T,
        bytes: &'a [u8],
    ) -> Result<T::Value> {
        config_map!(self, opts => ::internal::deserialize_seed(seed, bytes, opts))
    }

    /// Deserializes an object directly from a `Read`er using this configuration
    ///
    /// If this returns an `Error`, `reader` may be in an invalid state.
    #[inline(always)]
    pub fn deserialize_from<R: Read, T: serde::de::DeserializeOwned>(
        &self,
        reader: R,
    ) -> Result<T> {
        config_map!(self, opts => ::internal::deserialize_from(reader, opts))
    }

    /// Deserializes an object directly from a `Read`er with state `seed` using this configuration
    ///
    /// If this returns an `Error`, `reader` may be in an invalid state.
    #[inline(always)]
    pub fn deserialize_from_seed<'a, R: Read, T: serde::de::DeserializeSeed<'a>>(
        &self,
        seed: T,
        reader: R,
    ) -> Result<T::Value> {
        config_map!(self, opts => ::internal::deserialize_from_seed(seed, reader, opts))
    }

    /// Deserializes an object from a custom `BincodeRead`er using the default configuration.
    /// It is highly recommended to use `deserialize_from` unless you need to implement
    /// `BincodeRead` for performance reasons.
    ///
    /// If this returns an `Error`, `reader` may be in an invalid state.
    #[inline(always)]
    pub fn deserialize_from_custom<'a, R: BincodeRead<'a>, T: serde::de::DeserializeOwned>(
        &self,
        reader: R,
    ) -> Result<T> {
        config_map!(self, opts => ::internal::deserialize_from_custom(reader, opts))
    }

    /// Deserializes an object from a custom `BincodeRead`er with state `seed` using the default
    /// configuration. It is highly recommended to use `deserialize_from` unless you need to
    /// implement `BincodeRead` for performance reasons.
    ///
    /// If this returns an `Error`, `reader` may be in an invalid state.
    #[inline(always)]
    pub fn deserialize_from_custom_seed<
        'a,
        R: BincodeRead<'a>,
        T: serde::de::DeserializeSeed<'a>,
    >(
        &self,
        seed: T,
        reader: R,
    ) -> Result<T::Value> {
        config_map!(self, opts => ::internal::deserialize_from_custom_seed(seed, reader, opts))
    }

    /// Executes the acceptor with a serde::Deserializer instance.
    /// NOT A PART OF THE STABLE PUBLIC API
    #[doc(hidden)]
    pub fn with_deserializer<'a, A, R>(&self, reader: R, acceptor: A) -> A::Output
    where
        A: DeserializerAcceptor<'a>,
        R: BincodeRead<'a>,
    {
        config_map!(self, opts => {
            let mut deserializer = ::de::Deserializer::new(reader, opts);
            acceptor.accept(&mut deserializer)
        })
    }

    /// Executes the acceptor with a serde::Serializer instance.
    /// NOT A PART OF THE STABLE PUBLIC API
    #[doc(hidden)]
    pub fn with_serializer<A, W>(&self, writer: W, acceptor: A) -> A::Output
    where
        A: SerializerAcceptor,
        W: Write,
    {
        config_map!(self, opts => {
            let mut serializer = ::ser::Serializer::new(writer, opts);
            acceptor.accept(&mut serializer)
        })
    }
}
