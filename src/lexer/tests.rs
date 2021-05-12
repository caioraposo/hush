use super::*;

use assert_matches::assert_matches;


macro_rules! token {
	($kind:pat) => {
		Ok(Token { token: $kind, .. })
	};
}

macro_rules! error {
	($error:pat) => {
		Err(Error { error: $error, .. })
	};
}


#[test]
fn test_simple_function() {
	let input = r#"
		function foo(bar, baz)
			if bar or baz == nil then # here's a comment
				let result = do_something()
				return result
			end
		end
	"#;

	let cursor = Cursor::new(input.as_bytes());
	let mut interner = SymbolInterner::new();
	let lexer = Lexer::new(cursor, &mut interner);

	let tokens: Vec<Result<Token, Error<'_>>> = lexer.collect();

	macro_rules! assert_symbol {
		($symbol:ident, $expected:literal) => {
			assert_eq!(interner.resolve(*$symbol), Some($expected))
		};
	}

	assert_matches!(
		&tokens[..],
		[
			token!(TokenKind::Keyword(Keyword::Function)),
			token!(TokenKind::Identifier(foo)),
			token!(TokenKind::OpenParens),
			token!(TokenKind::Identifier(bar1)),
			token!(TokenKind::Comma),
			token!(TokenKind::Identifier(baz1)),
			token!(TokenKind::CloseParens),
			token!(TokenKind::Keyword(Keyword::If)),
			token!(TokenKind::Identifier(bar2)),
			token!(TokenKind::Operator(Operator::Or)),
			token!(TokenKind::Identifier(baz2)),
			token!(TokenKind::Operator(Operator::Equals)),
			token!(TokenKind::Literal(Literal::Nil)),
			token!(TokenKind::Keyword(Keyword::Then)),
			token!(TokenKind::Keyword(Keyword::Let)),
			token!(TokenKind::Identifier(result1)),
			token!(TokenKind::Operator(Operator::Assign)),
			token!(TokenKind::Identifier(do_something)),
			token!(TokenKind::OpenParens),
			token!(TokenKind::CloseParens),
			token!(TokenKind::Keyword(Keyword::Return)),
			token!(TokenKind::Identifier(result2)),
			token!(TokenKind::Keyword(Keyword::End)),
			token!(TokenKind::Keyword(Keyword::End)),
		]
			=> {
				assert_symbol!(foo, "foo");
				assert_symbol!(bar1, "bar");
				assert_symbol!(bar2, "bar");
				assert_symbol!(baz1, "baz");
				assert_symbol!(baz2, "baz");
				assert_symbol!(result1, "result");
				assert_symbol!(result2, "result");
				assert_symbol!(do_something, "do_something");
			}
	);
}


#[test]
fn test_invalid_tokens() {
	let input = r#"
		function foo(bar, baz) |
			if bar or baz == nil then # here's a comment
				let $result = do_something()
				return @}result
			end
		end
	"#;

	let cursor = Cursor::new(input.as_bytes());
	let mut interner = SymbolInterner::new();
	let lexer = Lexer::new(cursor, &mut interner);

	let tokens: Vec<Result<Token, Error<'_>>> = lexer.collect();

	macro_rules! assert_symbol {
		($symbol:ident, $expected:literal) => {
			assert_eq!(interner.resolve(*$symbol), Some($expected))
		};
	}

	assert_matches!(
		&tokens[..],
		[
			token!(TokenKind::Keyword(Keyword::Function)),
			token!(TokenKind::Identifier(foo)),
			token!(TokenKind::OpenParens),
			token!(TokenKind::Identifier(bar1)),
			token!(TokenKind::Comma),
			token!(TokenKind::Identifier(baz1)),
			token!(TokenKind::CloseParens),
			error!(ErrorKind::Unexpected(b'|')),
			token!(TokenKind::Keyword(Keyword::If)),
			token!(TokenKind::Identifier(bar2)),
			token!(TokenKind::Operator(Operator::Or)),
			token!(TokenKind::Identifier(baz2)),
			token!(TokenKind::Operator(Operator::Equals)),
			token!(TokenKind::Literal(Literal::Nil)),
			token!(TokenKind::Keyword(Keyword::Then)),
			token!(TokenKind::Keyword(Keyword::Let)),
			error!(ErrorKind::Unexpected(b'$')),
			token!(TokenKind::Identifier(result1)),
			token!(TokenKind::Operator(Operator::Assign)),
			token!(TokenKind::Identifier(do_something)),
			token!(TokenKind::OpenParens),
			token!(TokenKind::CloseParens),
			token!(TokenKind::Keyword(Keyword::Return)),
			error!(ErrorKind::Unexpected(b'@')),
			error!(ErrorKind::Unexpected(b'}')),
			token!(TokenKind::Identifier(result2)),
			token!(TokenKind::Keyword(Keyword::End)),
			token!(TokenKind::Keyword(Keyword::End)),
		]
			=> {
				assert_symbol!(foo, "foo");
				assert_symbol!(bar1, "bar");
				assert_symbol!(bar2, "bar");
				assert_symbol!(baz1, "baz");
				assert_symbol!(baz2, "baz");
				assert_symbol!(result1, "result");
				assert_symbol!(result2, "result");
				assert_symbol!(do_something, "do_something");
			}
	);
}


#[test]
fn test_string_literals() {
	let input = r#"
		let var = "hello world" ++ "escape \n sequences \" are \0 cool"
	"#;

	let cursor = Cursor::new(input.as_bytes());
	let mut interner = SymbolInterner::new();
	let lexer = Lexer::new(cursor, &mut interner);

	let tokens: Vec<Result<Token, Error<'_>>> = lexer.collect();

	macro_rules! assert_symbol {
		($symbol:ident, $expected:literal) => {
			assert_eq!(interner.resolve(*$symbol), Some($expected))
		};
	}

	println!("{:#?}", tokens);

	assert_matches!(
		&tokens[..],
		[
			token!(TokenKind::Keyword(Keyword::Let)),
			token!(TokenKind::Identifier(var)),
			token!(TokenKind::Operator(Operator::Assign)),
			token!(TokenKind::Literal(Literal::String(lit1))),
			token!(TokenKind::Operator(Operator::Concat)),
			token!(TokenKind::Literal(Literal::String(lit2))),
		]
			=> {
				assert_symbol!(var, "var");
				assert_matches!(lit1.as_ref(), b"hello world");
				assert_matches!(lit2.as_ref(), b"escape \n sequences \" are \0 cool");
			}
	);
}
