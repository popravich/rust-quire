use ast::Ast;
use errors::{Error, ErrorCollector};
use tokenizer::Pos;

/// Function that handles file include
pub type IncludeHandler<'a> =
    Fn(&Pos, &Include, &ErrorCollector, &Options) -> Ast + 'a;

/// The kind of include tag that encountered in config
pub enum Include<'a> {
    /// Looks like `!Include some/file.yaml`
    File { filename: &'a str },
    // TODO(tailhook)
    // /// Looks like `!*Include some/file.yaml:some_key`
    // SubKey { filename: &'a str, key: &'a str },
    // /// Looks like `!*IncludeSeq some/*.yaml`
    // Sequence { directory: &'a str, prefix: &'a str, suffix: &'a str },
    // /// Looks like `!*IncludeMap some/*.yaml`.
    // /// Everything matched by star is used as a key
    // Mapping { directory: &'a str, prefix: &'a str, suffix: &'a str },
}

/// Options for parsing configuration file
pub struct Options<'a> {
    include_handler: Box<IncludeHandler<'a>>,
}

pub trait DoInclude {
    fn include(&self, pos: &Pos, _: &Include, err: &ErrorCollector) -> Ast;
}

impl<'a> DoInclude for Options<'a> {
    fn include(&self, pos: &Pos, incl: &Include, err: &ErrorCollector) -> Ast {
        (self.include_handler)(pos, incl, err, self)
    }
}


fn unsupported_include(pos: &Pos, _: &Include,
    err: &ErrorCollector, _: &Options)
    -> Ast
{
    err.add_error(Error::preprocess_error(pos,
        format!("Includes are not supported")));
    return Ast::void(pos);
}

impl<'a> Options<'a> {
    /// Default options
    pub fn default() -> Options<'a> {
        Options {
            include_handler: Box::new(unsupported_include),
        }
    }
    /// Enables including files using specified handler function for reading
    /// included file
    pub fn allow_include<F>(&mut self, f: F)
        -> &mut Options<'a>
        where F: Fn(&Pos, &Include, &ErrorCollector, &Options) -> Ast + 'a
    {
        self.include_handler = Box::new(f);
        self
    }
}
