use lasso::Rodeo;

use crate::{
    ast::{Keyword, Token},
    error::GraphqlParseError,
};

pub struct Lexer<'a> {
    buffer: &'a [u8],
    cursor: usize,
    pub interner: Rodeo,
}

impl<'a> Lexer<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            cursor: 0,
            interner: Rodeo::default(),
        }
    }

    fn next_byte(&mut self) -> Option<u8> {
        self.buffer.get(self.cursor).copied().map(|b| {
            self.cursor += 1;
            b
        })
    }

    pub(crate) fn peek_byte(&mut self) -> Option<u8> {
        self.buffer.get(self.cursor).copied()
    }

    fn go_back(&mut self) {
        self.cursor -= 1;
    }

    pub fn expect_byte(&mut self, byte: u8) -> Result<(), GraphqlParseError> {
        self.skip_ignored_characters();
        match self.next_byte() {
            Some(next) if next == byte => Ok(()),
            Some(next) => Err(GraphqlParseError::ExpectedChar {
                token: byte as char,
                found: Some(next as char),
            }),
            None => Err(GraphqlParseError::ExpectedChar {
                token: byte as char,
                found: None,
            }),
        }
    }

    fn consume_byte_if_name_body(&mut self) -> bool {
        match self.peek_byte() {
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') => {
                self.next_byte();
                true
            }
            Some(..) | None => false,
        }
    }

    pub fn consume_byte_if_eq(&mut self, byte: u8) -> bool {
        self.skip_ignored_characters();

        let next = self.next_byte();

        if Some(byte) != next {
            self.go_back();
            return false;
        }

        true
    }

    fn lex_identifier(&mut self) -> Token {
        let start = self.cursor;

        while self.consume_byte_if_name_body() {}

        let ident = std::str::from_utf8(&self.buffer[start..self.cursor]).unwrap();

        // dbg!(ident);

        match ident {
            "type" => Token::Keyword(Keyword::Type),
            "input" => Token::Keyword(Keyword::Input),
            "enum" => Token::Keyword(Keyword::Enum),
            "implements" => Token::Keyword(Keyword::Implements),
            "scalar" => Token::Keyword(Keyword::Scalar),
            "true" => Token::Keyword(Keyword::True),
            "false" => Token::Keyword(Keyword::False),
            "union" => Token::Keyword(Keyword::Union),
            "fragment" => Token::Keyword(Keyword::Fragment),
            "query" => Token::Keyword(Keyword::Query),
            "mutation" => Token::Keyword(Keyword::Mutation),
            "subscription" => Token::Keyword(Keyword::Subscription),
            "extend" => Token::Keyword(Keyword::Extend),
            "null" => Token::Keyword(Keyword::Null),
            "interface" => Token::Keyword(Keyword::Interface),
            "on" => Token::Keyword(Keyword::On),
            _ => Token::Name(self.interner.get_or_intern(ident)),
        }
    }

    pub fn peek_token(&mut self) -> Result<Option<Token>, GraphqlParseError> {
        self.skip_ignored_characters();

        let start = self.cursor;

        let token = self.next_token();

        self.cursor = start;

        token
    }

    fn skip_ignored_characters(&mut self) {
        while let Some(b) = self.peek_byte() {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b',') {
                self.next_byte();
            } else {
                return;
            }
        }
    }

    // todo: more complex parsing rules for this, but works for now
    //
    // see https://spec.graphql.org/June2018/#BlockStringValue()
    fn lex_block_string(&mut self) -> Result<Token, GraphqlParseError> {
        let mut buffer = String::new();

        while let Some(byte) = self.next_byte() {
            if byte != b'"' {
                buffer.push(byte as char);
                continue;
            }

            let next_is_quote = self.next_byte() == Some(b'"');
            let two_from_now_is_quote = self.next_byte() == Some(b'"');

            if next_is_quote && two_from_now_is_quote {
                return Ok(Token::String(self.interner.get_or_intern(buffer.trim())));
            }

            self.go_back();
            self.go_back();
        }

        Err(GraphqlParseError::ExpectedChar {
            token: '"',
            found: None,
        })
    }

    fn lex_string(&mut self) -> Result<Token, GraphqlParseError> {
        let has_two_quotes = self.consume_byte_if_eq(b'"');
        let has_three_quotes = self.consume_byte_if_eq(b'"');

        let is_triple = if has_three_quotes {
            true
        // empty string ""
        } else if has_two_quotes {
            return Ok(Token::String(self.interner.get_or_intern("")));
        } else {
            false
        };

        if is_triple {
            return self.lex_block_string();
        }

        let mut buffer = String::new();

        let mut is_escaped = false;

        while let Some(b) = self.next_byte() {
            match b {
                b'"' if is_escaped => buffer.push('"'),
                b'\\' if is_escaped => buffer.push('\\'),
                b'/' if is_escaped => buffer.push('/'),
                b'n' if is_escaped => buffer.push('\n'),
                b'b' if is_escaped => todo!(),
                b'f' if is_escaped => todo!(),
                b'u' if is_escaped => todo!(),
                b'r' if is_escaped => buffer.push('\r'),
                b't' if is_escaped => buffer.push('\t'),
                b'\\' => is_escaped = true,
                b'\n' => {
                    return Err(GraphqlParseError::ExpectedChar {
                        token: '"',
                        found: Some('\n'),
                    })
                }
                b'"' => return Ok(Token::String(self.interner.get_or_intern(buffer))),
                c => buffer.push(c as char),
            }
        }

        Err(GraphqlParseError::ExpectedChar {
            token: '"',
            found: None,
        })
    }

    pub fn next_token(&mut self) -> Result<Option<Token>, GraphqlParseError> {
        self.skip_ignored_characters();

        Ok(Some(match self.next_byte() {
            Some(b' ' | b'\t' | b'\n' | b'\r' | b',') => return self.next_token(),
            Some(b'#') => todo!("comment"),
            Some(b'!') => Token::Bang,
            Some(b'$') => Token::Dollar,
            Some(b'(') => Token::OpenParen,
            Some(b')') => Token::CloseParen,
            Some(b'.') => {
                self.expect_byte(b'.')?;
                self.expect_byte(b'.')?;

                Token::DotDotDot
            }
            Some(b':') => Token::Colon,
            Some(b'=') => Token::Eq,
            Some(b'@') => Token::AtSign,
            Some(b'[') => Token::OpenSquareBrace,
            Some(b']') => Token::CloseSquareBrace,
            Some(b'{') => Token::OpenCurlyBrace,
            Some(b'|') => Token::Pipe,
            Some(b'}') => Token::CloseCurlyBrace,
            Some(b'&') => Token::Ampersand,
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                self.go_back();
                self.lex_identifier()
            }
            Some(b'0'..=b'9') => todo!(),
            Some(b'"') => self.lex_string()?,
            None => return Ok(None),
            _ => todo!(),
        }))
    }
}
