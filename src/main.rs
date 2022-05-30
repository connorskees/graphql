use parse::GraphqlParser;

mod ast;
mod error;
mod lexer;
mod parse;

fn main() {
    let buffer = Vec::new();

    let start = std::time::Instant::now();

    let document = GraphqlParser::parse(&buffer).unwrap();

    dbg!(start.elapsed());
}
