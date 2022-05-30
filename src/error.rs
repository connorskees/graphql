use crate::ast::Token;

#[derive(Debug)]
pub enum GraphqlParseError {
    ExpectedChar { token: char, found: Option<char> },
    ExpectedToken { token: Token, found: Option<Token> },
}
