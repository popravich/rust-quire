//! Validators for your config
//!
//! All of the validators are configures using builder/fluent pattern.
//!
//! Also note that while validator can do various different transforms to the
//! AST, but it works on the AST level, so it must put something that decoder
//! is able to decode in the result.

use std::str::FromStr;
use std::fmt::{Display};
use std::path::{PathBuf, Path, Component};
use std::default::Default;
use std::collections::{BTreeMap, HashSet};

use super::errors::{Error, ErrorCollector};
pub use super::tokenizer::Pos;
use super::ast::Ast as A;
use super::ast::Tag as T;
use super::ast::{Ast, NullKind};
use super::ast::ScalarKind::{Quoted, Plain};


static NUMERIC_SUFFIXES: &'static [(&'static str, i64)] = &[
    ("k", 1000),
    ("M", 1000000),
    ("G", 1000000000),
    ("ki", 1024),
    ("Mi", 1048576),
    ("Gi", 1024*1024*1024),
    ];

/// The trait every validator implements
pub trait Validator {
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast;
    fn default(&self, pos: Pos) -> Option<Ast>;
}

/// Scalar YAML value
///
/// This may be different kind of value:
///
/// * string
/// * bool
/// * path
/// * number
///
/// But some of the scalars might have better validators, for example
/// `Numeric` has minimum and maximum value as well as decodes human-friendly
/// unit values
#[derive(Default)]
pub struct Scalar {
    descr: Option<String>,
    optional: bool,
    default: Option<String>,
    min_length: Option<usize>,
    max_length: Option<usize>,
}

impl Scalar {
    pub fn new() -> Scalar {
        Default::default()
    }
    pub fn optional(mut self) -> Scalar {
        self.optional = true;
        self
    }
    pub fn default<S:ToString>(mut self, value: S) -> Scalar {
        self.default = Some(value.to_string());
        self
    }
    pub fn min_length(mut self, len: usize) -> Scalar {
        self.min_length = Some(len);
        self
    }
    pub fn max_length(mut self, len: usize) -> Scalar {
        self.max_length = Some(len);
        self
    }
}

impl Validator for Scalar {
    fn default(&self, pos: Pos) -> Option<Ast> {
        if self.default.is_none() && self.optional {
            return Some(A::Null(pos.clone(), T::NonSpecific, NullKind::Implicit));
        }
        self.default.as_ref().map(|val| {
            A::Scalar(pos.clone(), T::NonSpecific, Quoted, val.clone()) })
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, kind, val) = match ast {
            A::Scalar(pos, _, kind, string) => {
                (pos, kind, string)
            }
            A::Null(_, _, _) if self.optional => {
                return ast;
            }
            ast => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Value must be scalar")));
                return ast;
            }
        };
        self.min_length.map(|minl| {
            if val.len() < minl {
                err.add_error(Error::validation_error(&pos,
                    format!("Value must be at least {} characters", minl)));
            }
        });
        self.max_length.map(|maxl| {
            if val.len() > maxl {
                err.add_error(Error::validation_error(&pos,
                    format!("Value must be at most {} characters", maxl)));
            }
        });
        return A::Scalar(pos, T::NonSpecific, kind, val);
    }
}

/// Numeric validator
///
/// Similar to `Scalar` but validates that value is a number and also allows
/// limit the range of the value.
#[derive(Default)]
pub struct Numeric {
    descr: Option<String>,
    optional: bool,
    default: Option<i64>,
    min: Option<i64>,
    max: Option<i64>,
}

fn from_numeric(mut src: &str) -> Option<i64>
{
    let mut mult = 1;
    for &(suffix, value) in NUMERIC_SUFFIXES.iter() {
        if suffix.len() < src.len() &&
            &src[(src.len() - suffix.len())..] == suffix
        {
            mult = value;
            src = &src[..(src.len() - suffix.len())];
            break;
        }
    }
    let mut val: Option<i64> = FromStr::from_str(src).ok();
    if val.is_none() && src.len() > 2 {
        val = match &src[..2] {
            "0x" => i64::from_str_radix(&src[2..], 16).ok(),
            "0o" => i64::from_str_radix(&src[2..], 8).ok(),
            "0b" => i64::from_str_radix(&src[2..], 2).ok(),
            _    => None,
        };
    }
    return val.map(|x| x*mult);
}

impl Numeric {
    pub fn new() -> Numeric {
        Default::default()
    }
    pub fn optional(mut self) -> Numeric {
        self.optional = true;
        self
    }
    pub fn default(mut self, value: i64) -> Numeric {
        self.default = Some(value);
        self
    }
    pub fn min(mut self, val: i64) -> Numeric {
        self.min = Some(val);
        self
    }
    pub fn max(mut self, val: i64) -> Numeric {
        self.max = Some(val);
        self
    }
}

impl Validator for Numeric {

    fn default(&self, pos: Pos) -> Option<Ast> {
        if self.default.is_none() && self.optional {
            return Some(A::Null(pos.clone(), T::NonSpecific, NullKind::Implicit));
        }
        self.default.as_ref().map(|val| {
            A::Scalar(pos.clone(), T::NonSpecific, Quoted, val.to_string())
        })
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, val): (Pos, i64)  = match ast {
            A::Scalar(pos, tag, kind, string)
            => match from_numeric(&string) {
                Some(val) => (pos, val),
                None => {
                    err.add_error(Error::validation_error(&pos,
                        format!("Value must be numeric")));
                    return A::Scalar(pos, tag, kind, string);
                }
            },
            A::Null(_, _, _) if self.optional => {
                return ast;
            }
            ast => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Value must be scalar")));
                return ast;
            }
        };
        self.min.as_ref().map(|min| {
            if val < *min {
                err.add_error(Error::validation_error(&pos,
                    format!("Value must be at least {}", min)));
            }
        });
        self.max.as_ref().map(|max| {
            if val > *max {
                err.add_error(Error::validation_error(&pos,
                    format!("Value must be at most {}", max)));
            }
        });
        return A::Scalar(pos, T::NonSpecific, Plain, val.to_string());
    }
}


/// Directory validator
///
/// Similar to `Scalar` but also allows to force absolute or relative paths
#[derive(Default)]
pub struct Directory {
    descr: Option<String>,
    optional: bool,
    default: Option<PathBuf>,
    absolute: Option<bool>,
}

impl Directory {
    pub fn new() -> Directory {
        Default::default()
    }
    pub fn optional(mut self) -> Directory {
        self.optional = true;
        self
    }
    pub fn default<P:AsRef<Path>>(mut self, value: P) -> Directory {
        self.default = Some(value.as_ref().to_path_buf());
        self
    }
    pub fn is_absolute(mut self, value: bool) -> Directory {
        self.absolute = Some(value);
        self
    }
}

impl Validator for Directory {
    fn default(&self, pos: Pos) -> Option<Ast> {
        if self.default.is_none() && self.optional {
            return Some(A::Null(pos.clone(), T::NonSpecific, NullKind::Implicit));
        }
        self.default.as_ref().map(|val| {
            A::Scalar(pos.clone(), T::NonSpecific, Quoted,
                      val.display().to_string()) })
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, kind, val) = match ast {
            A::Scalar(pos, _, kind, string) => {
                (pos, kind, string)
            }
            A::Null(_, _, _) if self.optional => {
                return ast;
            }
            ast => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Path expected")));
                return ast;
            }
        };
        {
            let path = Path::new(&val);
            match self.absolute {
                Some(true) => {
                    if !path.is_absolute() {
                        err.add_error(Error::validation_error(&pos,
                            format!("Path must be absolute")));
                    }
                }
                Some(false) => {
                    if path.is_absolute() {
                        err.add_error(Error::validation_error(&pos,
                            format!("Path must not be absolute")));
                    } else {
                        // Still for non-absolute paths we must check if
                        // there are ../../something
                        //
                        // If you don't want this check, just set self.absolute
                        // to None instead of Some(false)
                        for cmp in path.components() {
                            if cmp == Component::ParentDir {
                                err.add_error(Error::validation_error(&pos,
                                    format!("The /../ is not allowed in path")));
                            }
                        }
                    }
                }
                None => {}
            };
        }
        return A::Scalar(pos, T::NonSpecific, kind, val);
    }
}

/// Structure validator
///
/// In yaml terms this validates that value is a map (or a null value, if all
/// defaults are okay).
///
/// Additionally this validator allows to parse some scalar and convert it to
/// the structure. This feature is useful to upgrade scalar value to
/// a structure maintaining backwards compatiblity as well as for configuring
/// common case more easily.
#[derive(Default)]
pub struct Structure<'a> {
    descr: Option<String>,
    members: Vec<(String, Box<Validator + 'a>)>,
    optional: bool,
    from_scalar: Option<fn (scalar: Ast) -> BTreeMap<String, Ast>>,
}

impl<'a> Structure<'a> {
    pub fn new() -> Structure<'a> {
        Default::default()
    }
    pub fn member<S: Display, V: Validator + 'a>(mut self, name: S, value: V)
        -> Structure<'a>
    {
        self.members.push((name.to_string(), Box::new(value)));
        self
    }
    pub fn optional(mut self) -> Structure<'a> {
        self.optional = true;
        self
    }
    pub fn parser(mut self,
        f: fn (scalar: Ast) -> BTreeMap<String, Ast>)
        -> Structure<'a>
    {
        self.from_scalar = Some(f);
        self
    }
}

impl<'a> Validator for Structure<'a> {
    fn default(&self, pos: Pos) -> Option<Ast> {
        if self.optional {
            return Some(A::Null(pos.clone(), T::NonSpecific, NullKind::Implicit));
        }
        let mut map = BTreeMap::new();
        for &(ref k, ref validator) in self.members.iter() {
            match validator.default(pos.clone()) {
                Some(val) => {
                    map.insert(k.clone(), val);
                }
                None => continue,
            }
        }
        return Some(A::Map(pos, T::NonSpecific, map));
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, mut map) = match (ast, self.from_scalar) {
            (A::Map(pos, _, items), _) => {
                (pos, items)
            }
            (A::Null(pos, _, NullKind::Implicit), _) => {
                return self.default(pos).unwrap();
            }
            (ast@A::Scalar(..), Some(from_scalar)) => {
                (ast.pos(), from_scalar(ast))
            }
            (ast, _) => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Value must be mapping")));
                return ast;
            }
        };
        for &(ref k, ref validator) in self.members.iter() {
            let value = match map.remove(k)
                .or(map.remove(&k[..].replace("_", "-"))) {
                Some(src) => {
                    validator.validate(src, err)
                }
                None => {
                    match validator.default(pos.clone()) {
                        Some(x) => x,
                        None => {
                            err.add_error(Error::validation_error(&pos,
                                format!("Field {} is expected", k)));
                            continue;
                        }
                    }
                }
            };
            map.insert(k.clone(), value);
        }
        let mut keys: HashSet<String>;
        keys = map.keys()
            .filter(|s| !&s[..].starts_with("_"))
            .map(|s| s.clone()).collect();
        for &(ref k, _) in self.members.iter() {
            keys.remove(k);
        }
        if keys.len() > 0 {
            err.add_error(Error::validation_error(&pos,
                format!("Keys {:?} are not expected", keys)));
        }
        return A::Map(pos, T::NonSpecific, map);
    }
}

/// Enum validator
///
/// This validates that the value is enum in the rust meaning of enum.
///
/// Enumeration can contain:
///
/// * A selection from constants: `enum T {a, b, c}`,
///   if you enable `allow_plain()`, this allows to select both as scalar
///   value `a` and as a tag with null value `!a`
///
/// * A set of different values with their own data: `enum T { a(x), b(y) }`.
///   In this case value must be specified with tag `!a "hello"` and may
///   contain different types inside `!b ["x", "y"]` or `!c {x: y}`. Only
///   one enum field is supported. Structure enums `enum T { a { x: u8 }` are
///   equivalent to an option with that struct as single value
///   `struct A { x: u8 }; enum T { a(A) }`
#[derive(Default)]
pub struct Enum<'a> {
    descr: Option<String>,
    options: Vec<(String, Box<Validator + 'a>)>,
    optional: bool,
    default_tag: Option<String>,
    allow_plain: bool,
}

impl<'a> Enum<'a> {
    pub fn new() -> Enum<'a> {
        Default::default()
    }
    pub fn optional(mut self) -> Enum<'a> {
        self.optional = true;
        self
    }
    /// For variant that doesn't
    pub fn allow_plain(mut self) -> Enum<'a> {
        assert!(self.default_tag.is_none(),
            "Default tag and allow_plain are not compatible");
        self.allow_plain = true;
        self
    }
    pub fn option<S: ToString, V: Validator + 'a>(mut self, name: S, value: V)
        -> Enum<'a>
    {
        self.options.push((name.to_string(), Box::new(value)));
        self
    }
    pub fn default_tag<S: ToString>(mut self, name: S) -> Enum<'a> {
        assert!(!self.allow_plain,
            "Default tag and allow_plain are not compatible");
        self.default_tag = Some(name.to_string());
        self
    }
}

impl<'a> Validator for Enum<'a> {
    fn default(&self, pos: Pos) -> Option<Ast> {
        if self.default_tag.is_some() && self.optional {
            return Some(A::Null(pos.clone(),
                T::LocalTag(self.default_tag.as_ref().unwrap().clone()),
                NullKind::Implicit));
        } else if self.optional {
            return Some(A::Null(pos.clone(), T::NonSpecific,
                                NullKind::Implicit));
        }
        return None;
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let tag_name = match ast.tag() {
            &T::LocalTag(ref tag_name) => {
                Some(tag_name.clone())
            }
            &T::NonSpecific => {
                if self.allow_plain {
                    if let A::Scalar(ref pos, _, _, ref val) = ast {
                        for &(ref k, ref validator) in self.options.iter() {
                            if &k[..] == val {
                                let value = validator.validate(
                                    A::Null(pos.clone(), T::NonSpecific,
                                            NullKind::Implicit),
                                    err);
                                return value.with_tag(
                                            T::LocalTag(k.to_string()));
                            }
                        }
                    }
                }
                if let Some(ref tag) = self.default_tag {
                    Some(tag.clone())
                } else {
                    err.add_error(Error::validation_error(&ast.pos(),
                        format!("One of the tags {:?} expected",
                            self.options.iter().map(|&(ref k, _)| k)
                                .collect::<Vec<&String>>())));
                    None
                }
            }
            _ => unimplemented!(),
        };
        if let Some(tag_name) = tag_name {
            let pos = ast.pos().clone();
            for &(ref k, ref validator) in self.options.iter() {
                if &k[..] == &tag_name[..] {
                    let value = validator.validate(ast, err);
                    return value.with_tag(T::LocalTag(tag_name));
                }
            }
            err.add_error(Error::validation_error(&pos,
                format!("The tag {} is not expected", tag_name)));
        }
        return ast;
    }
}

/// Validates yaml mapping
///
/// This type has type for a key and value and also can be converted
/// from scalar as shortcut.
#[derive(Default)]
pub struct Mapping<'a> {
    descr: Option<String>,
    key_element: Box<Validator + 'a>,
    value_element: Box<Validator + 'a>,
    from_scalar: Option<fn (scalar: Ast) -> BTreeMap<String, Ast>>,
}

impl<'a> Mapping<'a> {
    pub fn new<V: Validator + 'a, W: Validator + 'a>(key: V, val: W)
        -> Mapping<'a>
    {
        Mapping {
            descr: None,
            key_element: Box::new(key),
            value_element: Box::new(val),
            from_scalar: None,
        }
    }
    pub fn parser(mut self,
        f: fn (scalar: Ast) -> BTreeMap<String, Ast>)
        -> Mapping<'a>
    {
        self.from_scalar = Some(f);
        self
    }
}

impl<'a> Validator for Mapping<'a> {
    fn default(&self, pos: Pos) -> Option<Ast> {
        return Some(A::Map(pos, T::NonSpecific, BTreeMap::new()));
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, map) = match (ast, self.from_scalar) {
            (A::Map(pos, _, items), _) => {
                (pos, items)
            }
            (A::Null(pos, _, NullKind::Implicit), _) => {
                return A::Map(pos, T::NonSpecific, BTreeMap::new());
            }
            (ast@A::Scalar(..), Some(from_scalar)) => {
                (ast.pos(), from_scalar(ast))
            }
            (ast, _) => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Value must be mapping")));
                return ast;
            }
        };
        let mut res = BTreeMap::new();
        for (k, v) in map.into_iter() {
            let key = match self.key_element.validate(
                A::Scalar(v.pos().clone(), T::NonSpecific, Plain, k), err) {
                A::Scalar(_, _, _, val) => val,
                _ => unreachable!(),
            };
            let value = self.value_element.validate(v, err);
            res.insert(key, value);
        }
        return A::Map(pos, T::NonSpecific, res);
    }
}

/// Validates yaml sequence
///
/// Every element must be of single type, but it maybe an enum too.
///
/// This validator can also parse a scalar and convert it into a list in
/// application-specific way.
#[derive(Default)]
pub struct Sequence<'a> {
    descr: Option<String>,
    element: Box<Validator + 'a>,
    from_scalar: Option<fn (scalar: Ast) -> Vec<Ast>>,
}

impl<'a> Sequence<'a> {
    pub fn new<V: Validator + 'a>(el: V) -> Sequence<'a> {
        Sequence {
            descr: None,
            element: Box::new(el),
            from_scalar: None,
        }
    }
    pub fn parser(mut self, f: fn (scalar: Ast) -> Vec<Ast>) -> Sequence<'a> {
        self.from_scalar = Some(f);
        self
    }
}

impl<'a> Validator for Sequence<'a> {
    fn default(&self, pos: Pos) -> Option<Ast> {
        return Some(A::List(pos, T::NonSpecific, Vec::new()));
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        let (pos, children) = match (ast, self.from_scalar) {
            (A::List(pos, _, items), _) => {
                (pos, items)
            }
            (A::Null(pos, _, NullKind::Implicit), _) => {
                return A::List(pos, T::NonSpecific, Vec::new());
            }
            (ast@A::Scalar(_, _, _, _), Some(fun)) => {
                (ast.pos().clone(), fun(ast))
            }
            (ast, _) => {
                err.add_error(Error::validation_error(&ast.pos(),
                    format!("Value must be sequence")));
                return ast;
            }
        };
        let mut res = Vec::new();
        for val in children.into_iter() {
            let value = self.element.validate(val, err);
            res.push(value);
        }
        return A::List(pos, T::NonSpecific, res);
    }
}

/// Skips the validation of this value
///
/// It's useful to accept any value (of any type) at some place, or to
/// rely on `Decodable::decode` for doing validation.
pub struct Anything;

impl Validator for Anything {
    fn default(&self, _: Pos) -> Option<Ast> {
        return None;
    }
    fn validate(&self, ast: Ast, _err: &ErrorCollector) -> Ast {
        return ast;
    }
}

/// Only expect null at this place
///
/// This is mostly useful for enums, i.e. `!SomeTag null`
pub struct Nothing;

impl Validator for Nothing {
    fn default(&self, _: Pos) -> Option<Ast> {
        return None;
    }
    fn validate(&self, ast: Ast, err: &ErrorCollector) -> Ast {
        if let A::Null(_, _, _) = ast {
        } else {
            err.add_error(Error::parse_error(&ast.pos(),
                format!("Null expected, {} found", ast)));
        }
        return ast;
    }
}

impl<'a> Default for Box<Validator + 'a> {
    fn default() -> Box<Validator + 'a> {
        return Box::new(Nothing) as Box<Validator>;
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;
    use std::default::Default;
    use std::path::PathBuf;
    use rustc_serialize::Decodable;
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    use {Options};
    use super::super::decode::YamlDecoder;
    use super::super::ast::{process, Ast as A};
    use super::super::ast::Tag::{NonSpecific};
    use super::super::ast::ScalarKind::{Plain};
    use super::super::parser::parse;
    use super::super::sky::parse_string;
    use super::{Validator, Structure, Scalar, Numeric, Mapping, Sequence};
    use super::{Enum, Nothing, Directory};
    use super::super::errors::ErrorCollector;
    use self::TestEnum::*;

    #[derive(Clone, Debug, PartialEq, Eq, RustcDecodable)]
    struct TestStruct {
        intkey: usize,
        strkey: String,
    }

    fn parse_str(body: &str) -> TestStruct {
        let str_val = Structure { members: vec!(
            ("intkey".to_string(), Box::new(Numeric {
                default: Some(123),
                .. Default::default() }) as Box<Validator>),
            ("strkey".to_string(), Box::new(Scalar {
                default: Some("default_value".to_string()),
                .. Default::default() }) as Box<Validator>),
        ), .. Default::default()};
        parse_string("<inline text>", body, &str_val, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_all_fields() {
        assert_eq!(parse_str("intkey: 1\nstrkey: test"), TestStruct {
            intkey: 1,
            strkey: "test".to_string(),
        });
    }

    #[test]
    fn test_no_fields() {
        assert_eq!(parse_str("{}"), TestStruct {
            intkey: 123,
            strkey: "default_value".to_string(),
        });
    }

    #[test]
    fn test_only_int() {
        assert_eq!(parse_str("intkey: 777"), TestStruct {
            intkey: 777,
            strkey: "default_value".to_string(),
        });
    }

    #[test]
    fn test_only_str() {
        assert_eq!(parse_str("strkey: strvalue"), TestStruct {
            intkey: 123,
            strkey: "strvalue".to_string(),
        });
    }

    #[test]
    fn test_unit() {
        assert_eq!(parse_str("intkey: 100M"), TestStruct {
            intkey: 100000000,
            strkey: "default_value".to_string(),
        });
    }

    #[derive(Clone, Debug, PartialEq, Eq, RustcDecodable)]
    struct TestDash {
        some_key: usize,
    }

    fn parse_dash_str(body: &str) -> TestDash {
        let str_val = Structure { members: vec!(
            ("some_key".to_string(), Box::new(Numeric {
                default: Some(123),
                .. Default::default() }) as Box<Validator>),
        ), .. Default::default()};
        parse_string("<inline text>", body, &str_val, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_dash_str() {
        assert_eq!(parse_dash_str("some-key: 13"), TestDash {
            some_key: 13,
        });
    }

    #[test]
    fn test_underscore_str() {
        assert_eq!(parse_dash_str("some_key: 7"), TestDash {
            some_key: 7,
        });
    }

    fn parse_with_warnings(body: &str) -> (TestDash, Vec<String>) {
        let str_val = Structure { members: vec!(
            ("some_key".to_string(), Box::new(Numeric {
                default: Some(123),
                .. Default::default() }) as Box<Validator>),
        ), .. Default::default()};
        let err = ErrorCollector::new();
        let ast = parse(
                Rc::new("<inline text>".to_string()),
                body,
                |doc| { process(&Options::default(), doc, &err) }
            ).map_err(|e| err.into_fatal(e)).unwrap();
        let ast = str_val.validate(ast, &err);
        match Decodable::decode(&mut YamlDecoder::new(ast, &err)) {
            Ok(val) => {
                (val, err.unwrap().errors().map(|x| x.to_string()).collect())
            }
            Err(e) => {
                panic!("{}", err.into_fatal(e));
            }
        }
    }

    #[test]
    fn test_underscore_keys() {
        assert_eq!(parse_with_warnings("some-key: 13\n_another-key: 12"),
            (TestDash {
                some_key: 13,
            }, vec!()));
    }

    #[test]
    fn test_additional_keys() {
        assert_eq!(parse_with_warnings("some-key: 13\nanother-key: 12"),
            (TestDash {
                some_key: 13,
            }, vec!("<inline text>:1:1: Validation Error: \
                 Keys {\"another-key\"} are not expected"
                 .to_string())));
    }

    #[derive(Clone, Debug, PartialEq, Eq, RustcDecodable)]
    struct TestOpt {
        some_key: Option<usize>,
    }

    fn parse_opt_str(body: &str) -> TestOpt {
        let str_val = Structure { members: vec!(
            ("some_key".to_string(), Box::new(Numeric {
                default: None,
                optional: true,
                .. Default::default() }) as Box<Validator>),
        ), .. Default::default()};
        parse_string("<inline text>", body, &str_val, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_opt_val() {
        assert_eq!(parse_opt_str("some-key: 13"), TestOpt {
            some_key: Some(13),
        });
    }

    #[test]
    fn test_opt_none() {
        assert_eq!(parse_opt_str("some_key:"), TestOpt {
            some_key: None,
        });
    }

    #[test]
    fn test_opt_no_key() {
        assert_eq!(parse_opt_str("{}"), TestOpt {
            some_key: None,
        });
    }

    fn parse_map<T:Decodable>(body: &str) -> T {
        fn parse_default(ast: A) -> BTreeMap<String, A> {
            match ast {
                A::Scalar(pos, _, style, value) => {
                    let mut map = BTreeMap::new();
                    map.insert("default_value".to_string(),
                        A::Scalar(pos.clone(), NonSpecific, style, value));
                    map
                },
                _ => unreachable!(),
            }
        }

        let validator = Mapping {
            key_element: Box::new(Scalar { .. Default::default()}),
            value_element: Box::new(Numeric {
                default: Some(0),
                .. Default::default()}),
            .. Default::default()
        }.parser(parse_default);
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_map_1() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), 1);
        let res: BTreeMap<String, usize> = parse_map("a: 1");
        assert_eq!(res, m);
    }

    #[test]
    fn test_map_2() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), 1);
        m.insert("bc".to_string(), 2);
        let res: BTreeMap<String, usize> = parse_map("a: 1\nbc: 2");
        assert_eq!(res, m);
    }

    #[test]
    fn test_hash_map() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 1);
        m.insert("bc".to_string(), 2);
        let res: HashMap<String, usize> = parse_map("a: 1\nbc: 2");
        assert_eq!(res, m);
    }

    #[test]
    fn test_map_empty() {
        let m = BTreeMap::new();
        let res: BTreeMap<String, usize> = parse_map("{}");
        assert_eq!(res, m);
    }

    #[test]
    fn test_map_null() {
        let m = BTreeMap::new();
        let res: BTreeMap<String, usize> = parse_map("");
        assert_eq!(res, m);
    }

    #[test]
    fn test_map_with_parser() {
        let mut m = HashMap::new();
        m.insert("default_value".to_string(), 404);
        let res: HashMap<String, usize> = parse_map("404");
        assert_eq!(res, m);
    }

    fn parse_complex_map<T:Decodable>(body: &str)
        -> T
    {
        let validator = Mapping {
            key_element: Box::new(Scalar { .. Default::default()}),
            value_element: Box::new(Structure { members: vec!(
                ("some_key".to_string(), Box::new(Numeric {
                    default: Some(123),
                    .. Default::default() }) as Box<Validator>),
            ),.. Default::default()}) as Box<Validator>,
            .. Default::default()
        };
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_cmap_1() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), TestDash {
            some_key: 13,
        });
        let res: BTreeMap<String, TestDash>;
        res = parse_complex_map("a:\n some_key: 13");
        assert_eq!(res, m);
    }

    #[test]
    fn test_cmap_2() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), TestDash {
            some_key: 123,
        });
        let res: BTreeMap<String, TestDash>;
        res = parse_complex_map("a:\n");
        assert_eq!(res, m);
    }

    #[test]
    fn test_cmap_3() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), TestDash {
            some_key: 123,
        });
        let res: BTreeMap<String, TestDash> = parse_complex_map("a: {}");
        assert_eq!(res, m);
    }

    fn parse_seq(body: &str) -> Vec<usize> {
        fn split(ast: A) -> Vec<A> {
            match ast {
                A::Scalar(pos, _, style, value) => {
                    value
                        .split(" ")
                        .map(|v| {
                            A::Scalar(pos.clone(), NonSpecific, Plain, v.to_string())
                        })
                        .collect::<Vec<_>>()
                },
                _ => unreachable!(),
            }
        }

        let validator = Sequence {
            element: Box::new(Numeric {
                default: None,
                .. Default::default()}),
            .. Default::default()
        }.parser(split);
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_seq_1() {
        let m = vec!(1);
        let res: Vec<usize> = parse_seq("- 1");
        assert_eq!(res, m);
    }

    #[test]
    fn test_seq_2() {
        let m = vec!(1, 2);
        let res: Vec<usize> = parse_seq("- 1\n- 2");
        assert_eq!(res, m);
    }

    #[test]
    fn test_seq_empty() {
        let m = Vec::new();
        let res: Vec<usize> = parse_seq("[]");
        assert_eq!(res, m);
    }

    #[test]
    fn test_seq_null() {
        let m = Vec::new();
        let res: Vec<usize> = parse_seq("");
        assert_eq!(res, m);
    }

    #[test]
    fn test_seq_parser() {
        let m = vec!(1, 2, 3);
        let res: Vec<usize> = parse_seq("1 2 3");
        assert_eq!(res, m);
    }

    #[test]
    fn test_numeric() {
        let m = vec!(100, 200, 300);
        let res: Vec<usize> = parse_seq("- 0o144\n- 0b11001000\n- 0x12c");
        assert_eq!(res, m);
    }

    #[derive(PartialEq, Eq, RustcDecodable, Debug)]
    enum TestEnum {
        Alpha,
        Beta,
        Gamma(isize),
        Delta(TestStruct),
        Epsilon(Option<TestStruct>),
    }

    fn enum_validator<'x>() -> Enum<'x> {
        Enum {
            allow_plain: true,
            options: vec!(
                ("Alpha".to_string(), Box::new(Nothing) as Box<Validator>),
                ("Beta".to_string(), Box::new(Nothing) as Box<Validator>),
                ("Gamma".to_string(), Box::new(Numeric {
                    optional: true,
                    default: Some(7),
                    .. Default::default() }) as Box<Validator>),
                ("Delta".to_string(), Box::new(Structure { members: vec!(
                    ("intkey".to_string(), Box::new(Numeric {
                        default: Some(123),
                        .. Default::default() }) as Box<Validator>),
                    ("strkey".to_string(), Box::new(Scalar {
                        default: Some("default_value".to_string()),
                        .. Default::default()}) as Box<Validator>),
                    ), .. Default::default()}) as Box<Validator>),
                ("Epsilon".to_string(), Box::new(Structure {
                    optional: true,
                    members: vec!(
                    ("intkey".to_string(), Box::new(Numeric {
                        default: Some(457),
                        .. Default::default() }) as Box<Validator>),
                    ("strkey".to_string(), Box::new(Scalar {
                        default: Some("epsilon".to_string()),
                        .. Default::default() }) as Box<Validator>),
                    ), .. Default::default()}) as Box<Validator>),
            ), .. Default::default()
        }
    }

    fn parse_enum(body: &str) -> TestEnum {
        let validator = enum_validator();
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_enum_1() {
        assert_eq!(parse_enum("!Alpha"), Alpha);
    }

    #[test]
    fn test_enum_2() {
        assert_eq!(parse_enum("!Beta"), Beta);
    }

    #[test]
    fn test_enum_str() {
        assert_eq!(parse_enum("Alpha"), Alpha);
    }

    #[test]
    fn test_enum_3() {
        assert_eq!(parse_enum("!Alpha null"), Alpha);
    }

    #[test]
    fn test_enum_4() {
        assert_eq!(parse_enum("!Gamma 5"), Gamma(5));
    }

    #[test]
    #[should_panic]
    fn test_enum_5() {
        parse_enum("!Gamma");
    }

    #[test]
    fn test_enum_6() {
        assert_eq!(parse_enum("!Delta\nintkey: 1\nstrkey: a"),
            Delta(TestStruct {
                intkey: 1,
                strkey: "a".to_string(),
            }));
    }

    #[test]
    fn test_enum_7() {
        assert_eq!(parse_enum("!Delta"), Delta(TestStruct {
            intkey: 123,
            strkey: "default_value".to_string(),
            }));
    }

    #[test]
    fn test_enum_opt_struct() {
        assert_eq!(parse_enum("!Epsilon\nintkey: 457\nstrkey: a"),
            Epsilon(Some(TestStruct {
                intkey: 457,
                strkey: "a".to_string(),
            })));
    }

    #[test]
    fn test_enum_opt_struct_2() {
        assert_eq!(parse_enum("!Epsilon"), Epsilon(None));
    }

    #[test]
    fn test_enum_opt_struct_3() {
        assert_eq!(parse_enum("!Epsilon {}"),
            Epsilon(Some(TestStruct {
                intkey: 457,
                strkey: "epsilon".to_string(),
            })));
    }

    #[derive(Clone, PartialEq, Eq, RustcDecodable)]
    struct TestPath {
        path: PathBuf,
    }

    fn parse_path(body: &str, abs: Option<bool>) -> TestPath {
        let str_val = Structure { members: vec!(
            ("path".to_string(), Box::new(Directory {
                default: Some(PathBuf::from("/test")),
                absolute: abs,
                .. Default::default() }) as Box<Validator>),
        ), .. Default::default()};
        parse_string("<inline text>", body, &str_val, &Options::default())
        .unwrap()
    }

    #[test]
    #[should_panic(expected = "Path expected")]
    fn test_path_null() {
        assert!(parse_path("path:", None) == TestPath {
            path: PathBuf::from("/test"),
        });
    }

    #[test]
    fn test_path_abs() {
        assert!(parse_path("path: /root/dir", None) == TestPath {
            path: PathBuf::from("/root/dir"),
        });
    }

    #[test]
    fn test_path_rel() {
        assert!(parse_path("path: root/dir", None) == TestPath {
            path: PathBuf::from("root/dir"),
        });
    }

    #[test]
    fn test_path_down() {
        assert!(parse_path("path: ../root/dir", None) == TestPath {
            path: PathBuf::from("../root/dir"),
        });
    }

    #[test]
    fn test_path_abs_abs() {
        assert!(parse_path("path: /root/dir", Some(true)) == TestPath {
            path: PathBuf::from("/root/dir"),
        });
    }

    #[test]
    #[should_panic(expected = "must be absolute")]
    fn test_path_rel_abs() {
        assert!(parse_path("path: root/dir", Some(true)) == TestPath {
            path: PathBuf::from("root/dir"),
        });
    }

    #[test]
    #[should_panic(expected = "must be absolute")]
    fn test_path_down_abs() {
        assert!(parse_path("path: ../root/dir", Some(true)) == TestPath {
            path: PathBuf::from("../root/dir"),
        });
    }

    #[test]
    #[should_panic(expected = "must not be absolute")]
    fn test_path_abs_rel() {
        assert!(parse_path("path: /root/dir", Some(false)) == TestPath {
            path: PathBuf::from("/root/dir"),
        });
    }

    #[test]
    fn test_path_rel_rel() {
        assert!(parse_path("path: root/dir", Some(false)) == TestPath {
            path: PathBuf::from("root/dir"),
        });
    }

    #[test]
    #[should_panic(expected = "/../ is not allowed")]
    fn test_path_down_rel() {
        assert!(parse_path("path: ../root/dir", Some(false)) == TestPath {
            path: PathBuf::from("../root/dir"),
        });
    }

    fn parse_enum_list(body: &str) -> Vec<TestEnum> {
        let validator = Sequence {
            element: Box::new(enum_validator()),
            .. Default::default() };
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_enum_list_null() {
        assert_eq!(parse_enum_list("- !Delta\n- !Epsilon"), vec!(
            Delta(TestStruct {
                intkey: 123,
                strkey: "default_value".to_string(),
                }),
            Epsilon(None)
            ));
    }

    #[derive(PartialEq, Eq, RustcDecodable, Debug)]
    struct EnumOpt {
        val: Option<TestEnum>,
    }

    fn parse_enum_opt(body: &str) -> EnumOpt {
        let validator = Structure::new()
            .member("val", enum_validator().optional());
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_enum_val_none() {
        assert_eq!(parse_enum_opt("{}"), EnumOpt { val: None });
    }

    #[test]
    fn test_enum_val_some() {
        assert_eq!(parse_enum_opt("val: !Epsilon"),
                   EnumOpt { val: Some(Epsilon(None)) });
    }

    #[derive(PartialEq, Eq, RustcDecodable, Debug)]
    struct Parsed {
        value: String,
    }

    fn parse_struct_with_parser(body: &str) -> Parsed {
        fn value_parser(ast: A) -> BTreeMap<String, A> {
            match ast {
                A::Scalar(pos, _, style, value) => {
                    let mut map = BTreeMap::new();
                    map.insert("value".to_string(),
                        A::Scalar(pos.clone(), NonSpecific, style, value));
                    map
                }
                _ => unreachable!(),
            }
        }

        let validator = Structure::new()
            .member("value", Scalar::new())
            .parser(value_parser);
        parse_string("<inline text>", body, &validator, &Options::default())
        .unwrap()
    }

    #[test]
    fn test_struct_with_parser() {
        assert_eq!(parse_struct_with_parser("value: test"),
                   Parsed { value: "test".to_string() });
    }

    #[test]
    fn test_struct_with_parser_custom() {
        assert_eq!(parse_struct_with_parser("test"),
                   Parsed { value: "test".to_string() });
    }
}
