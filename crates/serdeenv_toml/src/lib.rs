pub fn builder_from_env() -> DeserializerBuilder<impl Iterator<Item = (String, String)>> {
    DeserializerBuilder::new(std::env::vars())
}

pub fn builder_default() -> DeserializerBuilder<impl Iterator<Item = (String, String)>> {
    builder_from_env().lowercased()
}

pub fn from_env<T: serde::de::DeserializeOwned>() -> Result<T, Error> {
    builder_default().deserialize()
}

pub struct DeserializerBuilder<I: Iterator<Item = (String, String)>> {
    iter: I,
}

impl<I: Iterator<Item = (String, String)>> DeserializerBuilder<I> {
    pub fn new(iter: I) -> Self {
        Self { iter }
    }

    pub fn prefixed(
        self,
        prefix: &str,
    ) -> DeserializerBuilder<impl Iterator<Item = (String, String)>> {
        let iter = self.iter.filter_map(move |(key, val)| {
            let key = key.strip_prefix(prefix)?;
            Some((key.to_owned(), val))
        });

        DeserializerBuilder { iter }
    }

    pub fn lowercased(self) -> DeserializerBuilder<impl Iterator<Item = (String, String)>> {
        let iter = self.iter.map(|(key, val)| (key.to_lowercase(), val));
        DeserializerBuilder { iter }
    }

    pub fn deserialize<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        let der = RootDeserializer { iter: self.iter };
        T::deserialize(der)
    }
}

mod macros {
    #[macro_export]
    macro_rules! forward_to_not_implemented {
        (@ $func: ident [enum]) => {
            fn $func<V>(
                self,
                _name: &'static str,
                _variants: &'static [&'static str],
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: "enum" })
            }
        };

        (@ $func: ident [ tuple_struct ]) => {
            fn $func<V>(
                self,
                _name: &'static str,
                _len: usize,
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: "tuple_struct" })
            }
        };

        (@ $func: ident [ tuple ]) => {
            fn $func<V>(
                self,
                _len: usize,
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: "tuple" })
            }
        };

        (@ $func: ident [ newtype_struct ]) => {
            fn $func<V>(
                self,
                _name: &'static str,
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: "newtype_structk" })
            }
        };

        (@ $func: ident [ unit_struct ]) => {
            fn $func<V>(
                self,
                _name: &'static str,
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: "unit_struct" })
            }
        };

        (@ $func: ident [ struct ]) => {
            fn $func<V>(self,
                _name: &'static str,
                _fields: &'static [&'static str],
                _visitor: V,
            ) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: stringify!(struct) })
            }
        };

        (@ $func: ident [ $name: ident ]) => {
            fn $func<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                Err(Error::NotImplemented { typename: stringify!($name) })
            }
        };

        ($($func:ident : $name: ident);*) => {
            $(
                forward_to_not_implemented!(@ $func [ $name ]);
            )*
        };
    }

    #[macro_export]
    macro_rules! forward_to_other {
        (
            $this: ident => $parser: expr;
            $($func: ident),*
        ) => {
            $(
                forward_to_other!(@ $func ($this) => $parser);
            )*
        };

        (@ $func: ident ($this: ident) => $parser: expr  ) => {
            fn $func<V>(self, visitor: V) -> Result<V::Value, Self::Error>
            where
                V: serde::de::Visitor<'de>,
            {
                let $this = self;
                let der = $parser;
                let out = der.$func(visitor)?;
                Ok(out)
            }
        };
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Deserialization not implemented for `{typename}`")]
    NotImplemented { typename: &'static str },

    #[error("{0}")]
    Custom(Box<str>),

    #[error("`{name}`: {error}")]
    Field { name: String, error: FieldError },
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Error::Custom(msg.to_string().into())
    }
}

struct RootDeserializer<I: Iterator<Item = (String, String)>> {
    iter: I,
}

impl<'de, I: Iterator<Item = (String, String)>> serde::de::Deserializer<'de>
    for RootDeserializer<I>
{
    type Error = Error;
    forward_to_not_implemented! {
        deserialize_bool: bool;
        deserialize_i8: i8;
        deserialize_i16: i16;
        deserialize_i32: i32;
        deserialize_i64: i64;
        deserialize_u8: u8;
        deserialize_u16: u16;
        deserialize_u32: u32;
        deserialize_u64: u64;
        deserialize_f32: f32;
        deserialize_f64: f64;
        deserialize_char: char;
        deserialize_str: str;
        deserialize_string: string;
        deserialize_bytes: bytes;
        deserialize_byte_buf: byte_buf;
        deserialize_option: option;
        deserialize_unit: unit;
        deserialize_unit_struct: unit_struct;
        deserialize_newtype_struct: newtype_struct;
        deserialize_seq: seq;
        deserialize_tuple: tuple;
        deserialize_enum: enum;
        deserialize_tuple_struct: tuple_struct;
        deserialize_identifier: identifier;
        deserialize_ignored_any: ignored_any;
        deserialize_any: any
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_map(MapTomlAccesor::new(self.iter))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }
}

pub struct MapTomlAccesor<I: Iterator<Item = (String, String)>> {
    iter: I,
    current: Option<(String, String)>,
}

impl<I: Iterator<Item = (String, String)>> MapTomlAccesor<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            current: None,
        }
    }
}

impl<'de, I: Iterator<Item = (String, String)>> serde::de::MapAccess<'de> for MapTomlAccesor<I> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                let der_key = seed.deserialize(KeyDeserializer(&key)).map(Some);
                self.current = Some((key, value));

                der_key
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        let (key, content) = self.current.take().unwrap();

        // Safety: only owned deserialize is permitted
        let content: &'de str = unsafe { std::mem::transmute::<&str, &'de str>(&content) };
        let der = ValueDeserializer { value: content };

        seed.deserialize(der).map_err(|e| Error::Field {
            name: key,
            error: e,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FieldError {
    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error("{0}")]
    Custom(Box<str>),
}

impl serde::de::Error for FieldError {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        FieldError::Custom(msg.to_string().into())
    }
}

pub struct ValueDeserializer<'de> {
    value: &'de str,
}

impl<'de> ValueDeserializer<'de> {
    pub fn toml_parser(self) -> toml::de::ValueDeserializer<'de> {
        toml::de::ValueDeserializer::new(self.value)
    }
}

impl<'de> serde::de::Deserializer<'de> for ValueDeserializer<'de> {
    type Error = FieldError;

    forward_to_other!(
        this => this.toml_parser();
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_f32,
        deserialize_f64,
        deserialize_bytes,
        deserialize_byte_buf,
        deserialize_option,
        deserialize_unit,
        deserialize_seq,
        deserialize_map,
        deserialize_identifier,
        deserialize_any
    );

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_unit_struct(name, visitor)?;
        Ok(out)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_newtype_struct(name, visitor)?;
        Ok(out)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_struct(name, fields, visitor)?;
        Ok(out)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_enum(name, variants, visitor)?;
        Ok(out)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_tuple(len, visitor)?;
        Ok(out)
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let der = self.toml_parser();
        let out = der.deserialize_tuple_struct(name, len, visitor)?;
        Ok(out)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.value.to_owned())
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_str(self.value)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        use serde::de::Error as _;
        use std::str::FromStr;

        let v = char::from_str(self.value).map_err(FieldError::custom)?;
        visitor.visit_char(v)
    }

    // ignored any avoid toml handle string
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }
}

pub struct KeyDeserializer<'s>(&'s str);
impl<'de, 's> serde::de::Deserializer<'de> for KeyDeserializer<'s> {
    type Error = Error;

    forward_to_not_implemented! {
        deserialize_bool: bool;
        deserialize_i8: i8;
        deserialize_i16: i16;
        deserialize_i32: i32;
        deserialize_i64: i64;
        deserialize_u8: u8;
        deserialize_u16: u16;
        deserialize_u32: u32;
        deserialize_u64: u64;
        deserialize_f32: f32;
        deserialize_f64: f64;
        deserialize_char: char;
        deserialize_str: str;
        deserialize_string: string;
        deserialize_bytes: bytes;
        deserialize_byte_buf: byte_buf;
        deserialize_option: option;
        deserialize_unit: unit;
        deserialize_unit_struct: unit_struct;
        deserialize_newtype_struct: newtype_struct;
        deserialize_seq: seq;
        deserialize_tuple: tuple;
        deserialize_map: map;
        deserialize_struct: struct;
        deserialize_enum: enum;
        deserialize_tuple_struct: tuple_struct;
        deserialize_ignored_any: ignored_any;
        deserialize_any: any
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Data {
            u8: u8,
            u16: u16,
            u32: u32,
            u64: u64,
            i8: i8,
            i16: i16,
            i32: i32,
            i64: i64,
            f32: f32,
            f64: f64,
            bool: bool,
            char: char,
            str: String,
        }

        let data = vec![
            ("u8", "8"),
            ("u16", "16"),
            ("u32", "32"),
            ("u64", "64"),
            ("i8", "-8"),
            ("i16", "-16"),
            ("i32", "-32"),
            ("i64", "-64"),
            ("f32", "32.0"),
            ("f64", "64.0"),
            ("bool", "true"),
            ("char", "a"),
            ("str", "string"),
            ("da", "fcad"),
        ];

        let input = data.iter().map(|(k, v)| (k.to_uppercase(), v.to_string()));
        let data: Data = DeserializerBuilder::new(input)
            .lowercased()
            .deserialize()
            .unwrap();

        assert_eq!(
            data,
            Data {
                u8: 8,
                u16: 16,
                u32: 32,
                u64: 64,
                i8: -8,
                i16: -16,
                i32: -32,
                i64: -64,
                f32: 32.0,
                f64: 64.0,
                bool: true,
                char: 'a',
                str: "string".to_string(),
            }
        );
    }

    #[test]
    fn nested() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Data {
            usize: usize,
            nested: Nested,
        }

        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Nested {
            message: String,
        }

        let data = [("usize", "8"), ("nested", "{ message=\"hello\" }")];

        let input = data.iter().map(|(k, v)| (k.to_uppercase(), v.to_string()));
        let data: Data = DeserializerBuilder::new(input)
            .lowercased()
            .deserialize()
            .unwrap();

        assert_eq!(
            data,
            Data {
                usize: 8,
                nested: Nested {
                    message: "hello".to_string(),
                },
            }
        );
    }
}
