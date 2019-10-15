//! Definition of a lexer for the WebAssembly text format.
//!
//! This module provides a [`Lexer`][] type which is an iterate over the raw
//! tokens of a WebAssembly text file. A [`Lexer`][] accounts for every single
//! byte in a WebAssembly text field, returning tokens even for comments and
//! whitespace. Typically you'll ignore comments and whitespace, however.
//!
//! If you'd like to iterate over the tokens in a file you can do so via:
//!
//! ```
//! # fn foo() -> Result<(), wast_parser::Error> {
//! use wast_parser::lexer::Lexer;
//!
//! let wat = "(module (func $foo))";
//! for token in Lexer::new(wat) {
//!     println!("{:?}", token?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Note that you'll typically not use this module but will rather use
//! [`ParseBuffer`](crate::parser::ParseBuffer) instead.
//!
//! [`Lexer`]: crate::lexer::Lexer

use std::borrow::Cow;
use std::char;
use std::fmt;
use std::iter;
use std::str;

/// A structure used to lex the s-expression syntax of WAT files.
///
/// This structure is used to generate [`Source`] items, which should account for
/// every single byte of the input as we iterate over it. A [`LexError`] is
/// returned for any non-lexable text.
#[derive(Clone)]
pub struct Lexer<'a> {
    it: iter::Peekable<str::CharIndices<'a>>,
    input: &'a str,
}

/// A fragment of source lex'd from an input string.
///
/// This enumeration contains all kinds of fragments, including comments and
/// whitespace. For most cases you'll probably ignore these and simply look at
/// tokens.
#[derive(Debug, PartialEq)]
pub enum Source<'a> {
    /// A fragment of source that is a comment, either a line or a block
    /// comment.
    Comment(Comment<'a>),
    /// A fragment of source that represents whitespace.
    Whitespace(&'a str),
    /// A fragment of source that represents an actual s-expression token.
    Token(Token<'a>),
}

/// The kinds of tokens that can be lexed for WAT s-expressions.
#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    /// A left-parenthesis, including the source text for where it comes from.
    LParen(&'a str),
    /// A right-parenthesis, including the source text for where it comes from.
    RParen(&'a str),

    /// A string literal, which is actually a list of bytes.
    String {
        /// The list of bytes that this string literal represents.
        val: Cow<'a, [u8]>,
        /// The original source text of this string literal.
        src: &'a str,
    },

    /// An identifier (like `$foo`).
    ///
    /// All identifiers start with `$` and the payload here is the original
    /// source text.
    Id(&'a str),

    /// A keyword, or something that starts with an alphabetic character.
    ///
    /// The payload here is the original source text.
    Keyword(&'a str),

    /// A reserved series of `idchar` symbols. Unknown what this is meant to be
    /// used for, you'll probably generate an error about an unexpected token.
    Reserved(&'a str),

    /// An integer.
    Integer(Integer<'a>),

    /// A float.
    Float(Float<'a>),
}

/// The types of comments that can be lexed from WAT source text, including the
/// original text of the comment itself.
///
/// Note that the original text here includes the symbols for the comment
/// itself.
#[derive(Debug, PartialEq)]
pub enum Comment<'a> {
    /// A line comment, preceded with `;;`
    Line(&'a str),

    /// A block comment, surrounded by `(;` and `;)`. Note that these can be
    /// nested.
    Block(&'a str),
}

/// Errors that can be generated while lexing.
///
/// All lexing errors have line/colum/position information as well as a
/// `LexErrorKind` indicating what kind of error happened while lexing.
#[derive(Debug, Clone)]
pub struct LexError {
    inner: Box<LexErrorInner>,
}

#[derive(Debug, Clone)]
struct LexErrorInner {
    line: usize,
    col: usize,
    pos: usize,
    kind: LexErrorKind,
}

/// The different classes of errors that can happen while lexing.
///
/// Do not exhaustively match on this enumeration.
#[derive(Debug, PartialEq, Clone)]
pub enum LexErrorKind {
    /// A dangling block comment was found with an unbalanced `(;` which was
    /// never terminated in the file.
    DanglingBlockComment,

    /// An unexpected character was encountered when generally parsing and
    /// looking for something else.
    Unexpected(char),

    /// An invalid `char` in a string literal was found.
    InvalidStringElement(char),

    /// An invalid string escape letter was found (the thing after the `\` in
    /// string literals)
    InvalidStringEscape(char),

    /// An invalid hexadecimal digit was found.
    InvalidHexDigit(char),

    /// An invalid base-10 digit was found.
    InvalidDigit(char),

    /// Parsing expected `wanted` but ended up finding `found` instead where the
    /// two characters aren't the same.
    Expected {
        /// The character that was expected to be found
        wanted: char,
        /// The character that was actually found
        found: char,
    },

    /// We needed to parse more but EOF (or end of the string) was encountered.
    UnexpectedEof,

    /// A number failed to parse because it was too big to fit within the target
    /// type.
    NumberTooBig,

    /// An invalid unicode value was found in a `\u{...}` escape in a string,
    /// only valid unicode scalars can be escaped that way.
    InvalidUnicodeValue(u32),

    /// A lone underscore was found when parsing a number, since underscores
    /// should always be preceded and succeeded with a digit of some form.
    LoneUnderscore,

    #[doc(hidden)]
    __Nonexhaustive,
}

/// A parsed integer, signed or unsigned.
///
/// Methods can be use to access the value of the integer.
#[derive(Debug, PartialEq)]
pub struct Integer<'a> {
    src: &'a str,
    val: Cow<'a, str>,
    hex: bool,
}

/// A parsed float.
///
/// Methods can be use to access the value of the float.
#[derive(Debug, PartialEq)]
pub struct Float<'a> {
    src: &'a str,
    val: FloatVal<'a>,
}

/// Possible parsed float values
#[derive(Debug, PartialEq)]
pub enum FloatVal<'a> {
    /// A float `NaN` representation
    Nan {
        /// The specific bits to encode for this float, optionally
        val: Option<u64>,
        /// Whether or not this is a negative `NaN` or not.
        negative: bool,
    },
    /// An float infinite representation,
    Inf {
        #[allow(missing_docs)]
        negative: bool,
    },
    /// A parsed and separated floating point value
    Val {
        /// Whether or not the `integral` and `decimal` are specified in hex
        hex: bool,
        /// The float parts before the `.`
        integral: Cow<'a, str>,
        /// The float parts after the `.`
        decimal: Option<Cow<'a, str>>,
        /// The exponent to multiple this `integral.decimal` portion of the
        /// float by. If `hex` is true this is `2^exponent` and otherwise it's
        /// `10^exponent`
        exponent: Option<Cow<'a, str>>,
    },
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer which will lex the `input` source string.
    pub fn new(input: &str) -> Lexer<'_> {
        Lexer {
            it: input.char_indices().peekable(),
            input,
        }
    }

    /// Returns the original source input that we're lexing.
    pub fn input(&self) -> &'a str {
        self.input
    }

    /// Lexes the next token in the input.
    ///
    /// Returns `Some` if a token is found or `None` if we're at EOF.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is malformed.
    pub fn parse(&mut self) -> Result<Option<Source<'a>>, LexError> {
        if let Some(ws) = self.ws() {
            return Ok(Some(Source::Whitespace(ws)));
        }
        if let Some(comment) = self.comment()? {
            return Ok(Some(Source::Comment(comment)));
        }
        if let Some(token) = self.token()? {
            return Ok(Some(Source::Token(token)));
        }
        match self.it.next() {
            Some((i, ch)) => Err(self.error(i, LexErrorKind::Unexpected(ch))),
            None => Ok(None),
        }
    }

    fn token(&mut self) -> Result<Option<Token<'a>>, LexError> {
        // First two are easy, they're just parens
        if let Some(pos) = self.eat_char('(') {
            return Ok(Some(Token::LParen(&self.input[pos..pos + 1])));
        }
        if let Some(pos) = self.eat_char(')') {
            return Ok(Some(Token::RParen(&self.input[pos..pos + 1])));
        }

        // Strings are also pretty easy, leading `"` is a dead giveaway
        if let Some(pos) = self.eat_char('"') {
            let val = self.string()?;
            let src = &self.input[pos..self.cur()];
            return Ok(Some(Token::String { val, src }));
        }

        let (start, prefix) = match self.it.peek().cloned() {
            Some((i, ch)) if is_idchar(ch) => (i, ch),
            Some((i, ch)) => return Err(self.error(i, LexErrorKind::Unexpected(ch))),
            None => return Ok(None),
        };

        while let Some((_, ch)) = self.it.peek().cloned() {
            if is_idchar(ch) {
                self.it.next();
            } else {
                break;
            }
        }

        let reserved = &self.input[start..self.cur()];

        if let Some(number) = self.number(reserved) {
            Ok(Some(number))
        } else if prefix == '$' && reserved.len() > 1 {
            Ok(Some(Token::Id(reserved)))
        } else if 'a' <= prefix && prefix <= 'z' {
            Ok(Some(Token::Keyword(reserved)))
        } else {
            Ok(Some(Token::Reserved(reserved)))
        }
    }

    fn number(&self, src: &'a str) -> Option<Token<'a>> {
        let (negative, num) = if src.starts_with('+') {
            (false, &src[1..])
        } else if src.starts_with('-') {
            (true, &src[1..])
        } else {
            (false, src)
        };

        // Handle `inf` and `nan` which are special numbers here
        if num == "inf" {
            return Some(Token::Float(Float {
                src,
                val: FloatVal::Inf { negative },
            }));
        } else if num == "nan" {
            return Some(Token::Float(Float {
                src,
                val: FloatVal::Nan {
                    val: None,
                    negative,
                },
            }));
        } else if num.starts_with("nan:0x") {
            let mut it = num[6..].chars();
            let to_parse = skip_undescores(&mut it, false, char::is_ascii_hexdigit)?;
            if it.next().is_some() {
                return None;
            }
            let n = u64::from_str_radix(&to_parse, 16).ok()?;
            return Some(Token::Float(Float {
                src,
                val: FloatVal::Nan {
                    val: Some(n),
                    negative,
                },
            }));
        }

        // Figure out if we're a hex number or not
        let (mut it, hex, test_valid) = if num.starts_with("0x") {
            (
                num[2..].chars(),
                true,
                char::is_ascii_hexdigit as fn(&char) -> bool,
            )
        } else {
            (
                num.chars(),
                false,
                char::is_ascii_digit as fn(&char) -> bool,
            )
        };

        // Evaluate the first part, moving out all underscores
        let val = skip_undescores(&mut it, negative, test_valid)?;

        match it.clone().next() {
            // If we're followed by something this may be a float so keep going.
            Some(_) => {}

            // Otherwise this is a valid integer literal!
            None => return Some(Token::Integer(Integer { src, val, hex })),
        }

        // A number can optionally be after the decimal so only actually try to
        // parse one if it's there.
        let decimal = if it.clone().next() == Some('.') {
            it.next();
            match it.clone().next() {
                Some(c) if test_valid(&c) => Some(skip_undescores(&mut it, false, test_valid)?),
                Some(_) | None => None,
            }
        } else {
            None
        };

        // Figure out if there's an exponential part here to make a float, and
        // if so parse it but defer its actual calculation until later.
        let exponent = match (hex, it.next()) {
            (true, Some('p')) | (true, Some('P')) | (false, Some('e')) | (false, Some('E')) => {
                let negative = match it.clone().next() {
                    Some('-') => {
                        it.next();
                        true
                    }
                    Some('+') => {
                        it.next();
                        false
                    }
                    _ => false,
                };
                Some(skip_undescores(&mut it, negative, char::is_ascii_digit)?)
            }
            (_, None) => None,
            _ => return None,
        };

        // We should have eaten everything by now, if not then this is surely
        // not a float or integer literal.
        if it.next().is_some() {
            return None;
        }

        return Some(Token::Float(Float {
            src,
            val: FloatVal::Val {
                hex,
                integral: val,
                exponent,
                decimal,
            },
        }));

        fn skip_undescores<'a>(
            it: &mut str::Chars<'a>,
            negative: bool,
            good: fn(&char) -> bool,
        ) -> Option<Cow<'a, str>> {
            enum State {
                Raw,
                Collecting(String),
            }
            let mut last_underscore = false;
            let mut state = if negative {
                State::Collecting("-".to_string())
            } else {
                State::Raw
            };
            let input = it.as_str();
            let first = it.next()?;
            if !good(&first) {
                return None;
            }
            if let State::Collecting(s) = &mut state {
                s.push(first);
            }
            let mut last = 1;
            while let Some(c) = it.clone().next() {
                if c == '_' {
                    if let State::Raw = state {
                        state = State::Collecting(input[..last].to_string());
                    }
                    it.next();
                    last_underscore = true;
                    continue;
                }
                if !good(&c) {
                    break;
                }
                if let State::Collecting(s) = &mut state {
                    s.push(c);
                }
                last_underscore = false;
                it.next();
                last += 1;
            }
            if last_underscore {
                return None;
            }
            Some(match state {
                State::Raw => input[..last].into(),
                State::Collecting(s) => s.into(),
            })
        }
    }

    /// Attempts to consume whitespace from the input stream, returning `None`
    /// if there's no whitespace to consume
    fn ws(&mut self) -> Option<&'a str> {
        let start = self.cur();
        loop {
            match self.it.peek() {
                Some((_, ' ')) | Some((_, '\n')) | Some((_, '\r')) | Some((_, '\t')) => {
                    drop(self.it.next())
                }
                _ => break,
            }
        }
        let end = self.cur();
        if start != end {
            Some(&self.input[start..end])
        } else {
            None
        }
    }

    /// Attempts to read a comment from the input stream
    fn comment(&mut self) -> Result<Option<Comment<'a>>, LexError> {
        if let Some(start) = self.eat_str(";;") {
            loop {
                match self.it.peek() {
                    None | Some((_, '\n')) => break,
                    _ => drop(self.it.next()),
                }
            }
            let end = self.cur();
            return Ok(Some(Comment::Line(&self.input[start..end])));
        }
        if let Some(start) = self.eat_str("(;") {
            let mut level = 1;
            while let Some((_, ch)) = self.it.next() {
                if ch == '(' && self.eat_char(';').is_some() {
                    level += 1;
                }
                if ch == ';' && self.eat_char(')').is_some() {
                    level -= 1;
                    if level == 0 {
                        let end = self.cur();
                        return Ok(Some(Comment::Block(&self.input[start..end])));
                    }
                }
            }

            return Err(self.error(start, LexErrorKind::DanglingBlockComment));
        }
        Ok(None)
    }

    /// Reads everything for a literal string except the leading `"`. Returns
    /// the string value that has been read.
    fn string(&mut self) -> Result<Cow<'a, [u8]>, LexError> {
        enum State {
            Start(usize),
            String(Vec<u8>),
        }
        let mut state = State::Start(self.cur());
        loop {
            match self.it.next() {
                Some((i, '\\')) => {
                    match state {
                        State::String(_) => {}
                        State::Start(start) => {
                            state = State::String(self.input[start..i].as_bytes().to_vec());
                        }
                    }
                    let buf = match &mut state {
                        State::String(b) => b,
                        State::Start(_) => unreachable!(),
                    };
                    match self.it.next() {
                        Some((_, '"')) => buf.push(b'"'),
                        Some((_, '\'')) => buf.push(b'\''),
                        Some((_, 't')) => buf.push(b'\t'),
                        Some((_, 'n')) => buf.push(b'\n'),
                        Some((_, 'r')) => buf.push(b'\r'),
                        Some((_, '\\')) => buf.push(b'\\'),
                        Some((i, 'u')) => {
                            self.must_eat_char('{')?;
                            let n = self.hexnum()?;
                            let c = char::from_u32(n).ok_or_else(|| {
                                self.error(i, LexErrorKind::InvalidUnicodeValue(n))
                            })?;
                            buf.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
                            self.must_eat_char('}')?;
                        }
                        Some((_, c1)) if c1.is_ascii_hexdigit() => {
                            let (_, c2) = self.hexdigit()?;
                            buf.push(to_hex(c1) * 16 + c2);
                        }
                        Some((i, c)) => {
                            return Err(self.error(i, LexErrorKind::InvalidStringEscape(c)))
                        }
                        None => {
                            return Err(self.error(self.input.len(), LexErrorKind::UnexpectedEof))
                        }
                    }
                }
                Some((_, '"')) => break,
                Some((i, c)) => {
                    if (c as u32) < 0x20 || c as u32 == 0x7f {
                        return Err(self.error(i, LexErrorKind::InvalidStringElement(c)));
                    }
                    match &mut state {
                        State::Start(_) => {}
                        State::String(v) => {
                            v.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                    }
                }
                None => return Err(self.error(self.input.len(), LexErrorKind::UnexpectedEof)),
            }
        }
        match state {
            State::Start(pos) => Ok(self.input[pos..self.cur() - 1].as_bytes().into()),
            State::String(s) => Ok(s.into()),
        }
    }

    fn hexnum(&mut self) -> Result<u32, LexError> {
        let (_, n) = self.hexdigit()?;
        let mut last_underscore = false;
        let mut n = n as u32;
        while let Some((i, c)) = self.it.peek().cloned() {
            if c == '_' {
                self.it.next();
                last_underscore = true;
                continue;
            }
            if !c.is_ascii_hexdigit() {
                break;
            }
            last_underscore = false;
            self.it.next();
            n = n
                .checked_mul(16)
                .and_then(|n| n.checked_add(to_hex(c) as u32))
                .ok_or_else(|| self.error(i, LexErrorKind::NumberTooBig))?;
        }
        if last_underscore {
            let cur = self.cur();
            return Err(self.error(cur, LexErrorKind::LoneUnderscore));
        }
        Ok(n)
    }

    /// Reads a hexidecimal digit from the input stream, returning where it's
    /// defined and the hex value. Returns an error on EOF or an invalid hex
    /// digit.
    fn hexdigit(&mut self) -> Result<(usize, u8), LexError> {
        let (i, ch) = self.must_char()?;
        if ch.is_ascii_hexdigit() {
            Ok((i, to_hex(ch)))
        } else {
            Err(self.error(i, LexErrorKind::InvalidHexDigit(ch)))
        }
    }

    /// Returns where the match started, if any
    fn eat_str(&mut self, s: &str) -> Option<usize> {
        if !self.cur_str().starts_with(s) {
            return None;
        }
        let ret = self.cur();
        for _ in s.chars() {
            self.it.next();
        }
        Some(ret)
    }

    /// Returns where the match happened, if any
    fn eat_char(&mut self, needle: char) -> Option<usize> {
        match self.it.peek() {
            Some((i, c)) if *c == needle => {
                let ret = *i;
                self.it.next();
                Some(ret)
            }
            _ => None,
        }
    }

    /// Reads the next character from the input string and where it's located,
    /// returning an error if the input stream is empty.
    fn must_char(&mut self) -> Result<(usize, char), LexError> {
        self.it
            .next()
            .ok_or_else(|| self.error(self.input.len(), LexErrorKind::UnexpectedEof))
    }

    /// Expects that a specific character must be read next
    fn must_eat_char(&mut self, wanted: char) -> Result<usize, LexError> {
        let (pos, found) = self.must_char()?;
        if wanted == found {
            Ok(pos)
        } else {
            Err(self.error(pos, LexErrorKind::Expected { wanted, found }))
        }
    }

    /// Returns the current position of our iterator through the input string
    fn cur(&mut self) -> usize {
        self.it.peek().map(|p| p.0).unwrap_or(self.input.len())
    }

    /// Returns the remaining string that we have left to parse
    fn cur_str(&mut self) -> &'a str {
        &self.input[self.cur()..]
    }

    /// Creates an error at `pos` with the specified `kind`
    fn error(&self, pos: usize, kind: LexErrorKind) -> LexError {
        let (line, col) = self.to_linecol(pos);
        LexError {
            inner: Box::new(LexErrorInner {
                line,
                col,
                pos,
                kind,
            }),
        }
    }

    fn to_linecol(&self, offset: usize) -> (usize, usize) {
        crate::to_linecol(self.input, offset)
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Source<'a>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parse().transpose()
    }
}

impl<'a> Source<'a> {
    /// Returns the original source text for this token.
    pub fn src(&self) -> &'a str {
        match self {
            Source::Comment(c) => c.src(),
            Source::Whitespace(s) => s,
            Source::Token(t) => t.src(),
        }
    }
}

impl<'a> Comment<'a> {
    /// Returns the original source text for this comment.
    pub fn src(&self) -> &'a str {
        match self {
            Comment::Line(s) => s,
            Comment::Block(s) => s,
        }
    }
}

impl<'a> Token<'a> {
    /// Returns the original source text for this token.
    pub fn src(&self) -> &'a str {
        match self {
            Token::LParen(s) => s,
            Token::RParen(s) => s,
            Token::String { src, .. } => src,
            Token::Id(s) => s,
            Token::Keyword(s) => s,
            Token::Reserved(s) => s,
            Token::Integer(i) => i.src(),
            Token::Float(f) => f.src(),
        }
    }
}

impl<'a> Integer<'a> {
    /// Returns the original source text for this integer.
    pub fn src(&self) -> &'a str {
        self.src
    }

    /// Returns the value string that can be parsed for this integer, as well as
    /// the base that it should be parsed in
    pub fn val(&self) -> (&str, u32) {
        (&self.val, if self.hex { 16 } else { 10 })
    }
}

impl<'a> Float<'a> {
    /// Returns the original source text for this integer.
    pub fn src(&self) -> &'a str {
        self.src
    }

    /// Returns a parsed value of this float with all of the components still
    /// listed as strings.
    pub fn val(&self) -> &FloatVal<'a> {
        &self.val
    }
}

impl LexError {
    /// Returns the associated `LexErrorKind` for this error.
    pub fn kind(&self) -> &LexErrorKind {
        &self.inner.kind
    }
}

fn to_hex(c: char) -> u8 {
    match c {
        'a'..='f' => c as u8 - b'a' + 10,
        'A'..='F' => c as u8 - b'A' + 10,
        _ => c as u8 - b'0',
    }
}

fn is_idchar(c: char) -> bool {
    match c {
        '0'..='9'
        | 'a'..='z'
        | 'A'..='Z'
        | '!'
        | '#'
        | '$'
        | '%'
        | '&'
        | '\''
        | '*'
        | '+'
        | '-'
        | '.'
        | '/'
        | ':'
        | '<'
        | '='
        | '>'
        | '?'
        | '@'
        | '\\'
        | '^'
        | '_'
        | '`'
        | '|'
        | '~' => true,
        _ => false,
    }
}

impl LexError {
    /// Returns the 0-indexed line number that this lex error happened at
    pub fn line(&self) -> usize {
        self.inner.line
    }

    /// Returns the 0-indexed column number that this lex error happened at
    pub fn col(&self) -> usize {
        self.inner.col
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LexErrorKind::*;
        match self.inner.kind {
            DanglingBlockComment => f.write_str("unterminated block comment")?,
            Unexpected(c) => write!(f, "unexpected character {:?}", c)?,
            InvalidStringElement(c) => write!(f, "invalid character in string {:?}", c)?,
            InvalidStringEscape(c) => write!(f, "invalid string escape {:?}", c)?,
            InvalidHexDigit(c) => write!(f, "invalid hex digit {:?}", c)?,
            InvalidDigit(c) => write!(f, "invalid decimal digit {:?}", c)?,
            Expected { wanted, found } => write!(f, "expected {:?} but found {:?}", wanted, found)?,
            UnexpectedEof => write!(f, "unexpected end-of-file")?,
            NumberTooBig => f.write_str("number is too big to parse")?,
            InvalidUnicodeValue(c) => write!(f, "invalid unicode scalar value {:x}", c)?,
            LoneUnderscore => write!(f, "bare underscore in numeric literal")?,
            __Nonexhaustive => unreachable!(),
        }
        Ok(())
    }
}

impl std::error::Error for LexError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_smoke() {
        fn get_whitespace(input: &str) -> &str {
            match Lexer::new(input).parse().expect("no first token") {
                Some(Source::Whitespace(s)) => s,
                other => panic!("unexpected {:?}", other),
            }
        }
        assert_eq!(get_whitespace(" "), " ");
        assert_eq!(get_whitespace("  "), "  ");
        assert_eq!(get_whitespace("  \n "), "  \n ");
        assert_eq!(get_whitespace("  x"), "  ");
        assert_eq!(get_whitespace("  ;"), "  ");
    }

    #[test]
    fn line_comment_smoke() {
        fn get_line_comment(input: &str) -> &str {
            match Lexer::new(input).parse().expect("no first token") {
                Some(Source::Comment(Comment::Line(s))) => s,
                other => panic!("unexpected {:?}", other),
            }
        }
        assert_eq!(get_line_comment(";;"), ";;");
        assert_eq!(get_line_comment(";; xyz"), ";; xyz");
        assert_eq!(get_line_comment(";; xyz\nabc"), ";; xyz");
        assert_eq!(get_line_comment(";;\nabc"), ";;");
        assert_eq!(get_line_comment(";;   \nabc"), ";;   ");
    }

    #[test]
    fn block_comment_smoke() {
        fn get_block_comment(input: &str) -> &str {
            match Lexer::new(input).parse().expect("no first token") {
                Some(Source::Comment(Comment::Block(s))) => s,
                other => panic!("unexpected {:?}", other),
            }
        }
        assert_eq!(get_block_comment("(;;)"), "(;;)");
        assert_eq!(get_block_comment("(; ;)"), "(; ;)");
        assert_eq!(get_block_comment("(; (;;) ;)"), "(; (;;) ;)");
        assert_eq!(
            *Lexer::new("(; ").parse().unwrap_err().kind(),
            LexErrorKind::DanglingBlockComment,
        );
        assert_eq!(
            *Lexer::new("(; (;;)").parse().unwrap_err().kind(),
            LexErrorKind::DanglingBlockComment,
        );
        assert_eq!(
            *Lexer::new("(; ;").parse().unwrap_err().kind(),
            LexErrorKind::DanglingBlockComment,
        );
    }

    fn get_token(input: &str) -> Token<'_> {
        match Lexer::new(input).parse().expect("no first token") {
            Some(Source::Token(t)) => t,
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn lparen() {
        assert_eq!(get_token("(("), Token::LParen("("));
    }

    #[test]
    fn rparen() {
        assert_eq!(get_token(")("), Token::RParen(")"));
    }

    #[test]
    fn strings() {
        fn get_string(input: &str) -> Cow<'_, [u8]> {
            match get_token(input) {
                Token::String { val, src } => {
                    assert_eq!(input, src);
                    val
                }
                other => panic!("not string {:?}", other),
            }
        }
        assert_eq!(&*get_string("\"\""), b"");
        assert_eq!(&*get_string("\"a\""), b"a");
        assert_eq!(&*get_string("\"a b c d\""), b"a b c d");
        assert_eq!(&*get_string("\"\\\"\""), b"\"");
        assert_eq!(&*get_string("\"\\'\""), b"'");
        assert_eq!(&*get_string("\"\\n\""), b"\n");
        assert_eq!(&*get_string("\"\\t\""), b"\t");
        assert_eq!(&*get_string("\"\\r\""), b"\r");
        assert_eq!(&*get_string("\"\\\\\""), b"\\");
        assert_eq!(&*get_string("\"\\01\""), &[1]);
        assert_eq!(&*get_string("\"\\u{1}\""), &[1]);
        assert_eq!(
            &*get_string("\"\\u{0f3}\""),
            '\u{0f3}'.encode_utf8(&mut [0; 4]).as_bytes()
        );
        assert_eq!(
            &*get_string("\"\\u{0_f_3}\""),
            '\u{0f3}'.encode_utf8(&mut [0; 4]).as_bytes()
        );

        for i in 0..=255i32 {
            let s = format!("\"\\{:02x}\"", i);
            assert_eq!(&*get_string(&s), &[i as u8]);
        }

        assert_eq!(
            *Lexer::new("\"").parse().unwrap_err().kind(),
            LexErrorKind::UnexpectedEof,
        );
        assert_eq!(
            *Lexer::new("\"\\x\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidStringEscape('x'),
        );
        assert_eq!(
            *Lexer::new("\"\\0\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidHexDigit('"'),
        );
        assert_eq!(
            *Lexer::new("\"\\0").parse().unwrap_err().kind(),
            LexErrorKind::UnexpectedEof,
        );
        assert_eq!(
            *Lexer::new("\"\\").parse().unwrap_err().kind(),
            LexErrorKind::UnexpectedEof,
        );
        assert_eq!(
            *Lexer::new("\"\u{7f}\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidStringElement('\u{7f}'),
        );
        assert_eq!(
            *Lexer::new("\"\u{0}\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidStringElement('\u{0}'),
        );
        assert_eq!(
            *Lexer::new("\"\u{1f}\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidStringElement('\u{1f}'),
        );
        assert_eq!(
            *Lexer::new("\"\\u{x}\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidHexDigit('x'),
        );
        assert_eq!(
            *Lexer::new("\"\\u{1_}\"").parse().unwrap_err().kind(),
            LexErrorKind::LoneUnderscore,
        );
        assert_eq!(
            *Lexer::new("\"\\u{fffffffffffffffff}\"")
                .parse()
                .unwrap_err()
                .kind(),
            LexErrorKind::NumberTooBig,
        );
        assert_eq!(
            *Lexer::new("\"\\u{ffffffff}\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidUnicodeValue(0xffffffff),
        );
        assert_eq!(
            *Lexer::new("\"\\u\"").parse().unwrap_err().kind(),
            LexErrorKind::Expected {
                wanted: '{',
                found: '"'
            },
        );
        assert_eq!(
            *Lexer::new("\"\\u{\"").parse().unwrap_err().kind(),
            LexErrorKind::InvalidHexDigit('"'),
        );
        assert_eq!(
            *Lexer::new("\"\\u{1\"").parse().unwrap_err().kind(),
            LexErrorKind::Expected {
                wanted: '}',
                found: '"'
            },
        );
        assert_eq!(
            *Lexer::new("\"\\u{1").parse().unwrap_err().kind(),
            LexErrorKind::UnexpectedEof,
        );
    }

    #[test]
    fn id() {
        fn get_id(input: &str) -> &str {
            match get_token(input) {
                Token::Id(s) => s,
                other => panic!("not id {:?}", other),
            }
        }
        assert_eq!(get_id("$x"), "$x");
        assert_eq!(get_id("$xyz"), "$xyz");
        assert_eq!(get_id("$x_z"), "$x_z");
        assert_eq!(get_id("$0^"), "$0^");
        assert_eq!(get_id("$0^;;"), "$0^");
        assert_eq!(get_id("$0^ ;;"), "$0^");
    }

    #[test]
    fn keyword() {
        fn get_keyword(input: &str) -> &str {
            match get_token(input) {
                Token::Keyword(s) => s,
                other => panic!("not id {:?}", other),
            }
        }
        assert_eq!(get_keyword("x"), "x");
        assert_eq!(get_keyword("xyz"), "xyz");
        assert_eq!(get_keyword("x_z"), "x_z");
        assert_eq!(get_keyword("x_z "), "x_z");
        assert_eq!(get_keyword("x_z "), "x_z");
    }

    #[test]
    fn reserved() {
        fn get_reserved(input: &str) -> &str {
            match get_token(input) {
                Token::Reserved(s) => s,
                other => panic!("not reserved {:?}", other),
            }
        }
        assert_eq!(get_reserved("$ "), "$");
        assert_eq!(get_reserved("^_x "), "^_x");
    }

    #[test]
    fn integer() {
        fn get_integer(input: &str) -> Cow<'_, str> {
            match get_token(input) {
                Token::Integer(i) => {
                    assert_eq!(input, i.src());
                    i.val
                }
                other => panic!("not integer {:?}", other),
            }
        }
        assert_eq!(get_integer("1"), "1");
        assert_eq!(get_integer("0"), "0");
        assert_eq!(get_integer("-1"), "-1");
        assert_eq!(get_integer("+1"), "1");
        assert_eq!(get_integer("+1_000"), "1000");
        assert_eq!(get_integer("+1_0______0_0"), "1000");
        assert_eq!(get_integer("+0x10"), "10");
        assert_eq!(get_integer("-0x10"), "-10");
        assert_eq!(get_integer("0x10"), "10");
    }

    #[test]
    fn float() {
        fn get_float(input: &str) -> FloatVal<'_> {
            match get_token(input) {
                Token::Float(i) => {
                    assert_eq!(input, i.src());
                    i.val
                }
                other => panic!("not reserved {:?}", other),
            }
        }
        assert_eq!(
            get_float("nan"),
            FloatVal::Nan {
                val: None,
                negative: false
            },
        );
        assert_eq!(
            get_float("-nan"),
            FloatVal::Nan {
                val: None,
                negative: true,
            },
        );
        assert_eq!(
            get_float("+nan"),
            FloatVal::Nan {
                val: None,
                negative: false,
            },
        );
        assert_eq!(
            get_float("+nan:0x1"),
            FloatVal::Nan {
                val: Some(1),
                negative: false,
            },
        );
        assert_eq!(
            get_float("nan:0x7f_ffff"),
            FloatVal::Nan {
                val: Some(0x7fffff),
                negative: false,
            },
        );
        assert_eq!(get_float("inf"), FloatVal::Inf { negative: false });
        assert_eq!(get_float("-inf"), FloatVal::Inf { negative: true });
        assert_eq!(get_float("+inf"), FloatVal::Inf { negative: false });

        assert_eq!(
            get_float("1.2"),
            FloatVal::Val {
                integral: "1".into(),
                decimal: Some("2".into()),
                exponent: None,
                hex: false,
            },
        );
        assert_eq!(
            get_float("1.2e3"),
            FloatVal::Val {
                integral: "1".into(),
                decimal: Some("2".into()),
                exponent: Some("3".into()),
                hex: false,
            },
        );
        assert_eq!(
            get_float("-1_2.1_1E+0_1"),
            FloatVal::Val {
                integral: "-12".into(),
                decimal: Some("11".into()),
                exponent: Some("01".into()),
                hex: false,
            },
        );
        assert_eq!(
            get_float("+1_2.1_1E-0_1"),
            FloatVal::Val {
                integral: "12".into(),
                decimal: Some("11".into()),
                exponent: Some("-01".into()),
                hex: false,
            },
        );
        assert_eq!(
            get_float("0x1_2.3_4p5_6"),
            FloatVal::Val {
                integral: "12".into(),
                decimal: Some("34".into()),
                exponent: Some("56".into()),
                hex: true,
            },
        );
        assert_eq!(
            get_float("+0x1_2.3_4P-5_6"),
            FloatVal::Val {
                integral: "12".into(),
                decimal: Some("34".into()),
                exponent: Some("-56".into()),
                hex: true,
            },
        );
        assert_eq!(
            get_float("1."),
            FloatVal::Val {
                integral: "1".into(),
                decimal: None,
                exponent: None,
                hex: false,
            },
        );
        assert_eq!(
            get_float("0x1p-24"),
            FloatVal::Val {
                integral: "1".into(),
                decimal: None,
                exponent: Some("-24".into()),
                hex: true,
            },
        );
    }
}