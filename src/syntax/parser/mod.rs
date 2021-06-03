mod command;
mod error;

use std::{
	collections::HashMap,
	iter::Peekable,
};

use super::{
	SourcePos,
	ast::{self, CommandBlockKind},
	lexer::{ArgPart, ArgUnit, Keyword, Token, TokenKind, Operator, CommandOperator}
};
pub use error::Error;


/// The parser may report multiple errors before finishing. Instead of allocating those in
/// an vector, we delegate such handling to the caller.
pub trait ErrorReporter {
	fn report(&mut self, error: Error);
}


impl<F> ErrorReporter for F
where
	F: FnMut(Error),
{
	fn report(&mut self, error: Error) {
		self(error)
	}
}


/// The parser for Hush syntax.
#[derive(Debug)]
pub struct Parser<I, E>
where
	I: Iterator<Item = Token>,
{
	// We don't use a std::iter::Peekable instead of a (Iterator, Option<Token>) pair
	// because we must be able to move from `token`, but Peekable only returns a reference.
	cursor: Peekable<I>,
	token: Option<Token>,
	error_reporter: E,
}


impl<I, E> Parser<I, E>
where
	I: Iterator<Item = Token>,
	E: ErrorReporter,
{
	/// Create a new parser for the given input.
	pub fn new(mut cursor: I, error_reporter: E) -> Self {
		let token = cursor.next();

		Self { cursor: cursor.peekable(), token, error_reporter }
	}


	/// Parse the input, producing a top-level block.
	pub fn parse(mut self) -> ast::Block {
		loop {
			match self.parse_block() {
				Ok(block) => return block,
				Err(error) => self.error_reporter.report(error),
			};
		}
	}


	/// Peek the next token.
	fn peek(&mut self) -> Option<&Token> {
		self.cursor.peek()
	}


	/// Step the cursor, placing the next token on self.token.
	fn step(&mut self) {
		self.token = self.cursor.next();
	}


	/// Try and eat a token.
	fn eat<F, T>(&mut self, eat: F) -> Result<T, Error>
	where
		F: FnOnce(Token) -> Result<T, (Error, Token)>,
	{
		if let Some(token) = self.token.take() {
			match eat(token) {
				Ok(value) => {
					// Token successfully consumed.
					self.step();
					Ok(value)
				}

				Err((error, token)) => {
					// Fail, rollback the token and produce an error.
					self.token = Some(token);
					Err(error)
				}
			}
		} else {
			Err(Error::unexpected_eof())
		}
	}


	/// Consume the expected token, or produce an error.
	fn expect(&mut self, expected: TokenKind) -> Result<TokenKind, Error> {
		self.eat(|token| match token {
			Token { token, .. } if token == expected => Ok(token),
			token => Err((Error::unexpected(token.clone(), expected), token)),
		})
	}


	/// Parse a block of statements, stopping when END of EOF are reached, or after a return
	/// is parsed. The lua-like grammar requires stopping after such conditions.
	fn parse_block(&mut self) -> Result<ast::Block, Error> {
		let mut block = Vec::new();

		loop {
			match &self.token {
				// Break on end of block.
				Some(Token { token: TokenKind::Keyword(Keyword::Else), .. }) => break,
				Some(Token { token: TokenKind::Keyword(Keyword::End), .. }) => break,

				Some(_) => {
					let statement = self.parse_statement()?;
					let is_return = matches!(statement, ast::Statement::Return { .. });

					block.push(statement);

					if is_return {
						// There may be no statements following a return in a block.
						break;
					}
				}

				// Break on eof.
				None => break,
			}
		}

		Ok(block.into_boxed_slice().into())
	}


	/// Parse a single statement.
	fn parse_statement(&mut self) -> Result<ast::Statement, Error> {
		match self.token.take() {
			// Let.
			Some(Token { token: TokenKind::Keyword(Keyword::Let), pos }) => {
				self.step();

				let (identifier, _) = self.parse_identifier()?;
				let init;
				if matches!(self.token, Some(Token { token: TokenKind::Operator(Operator::Assign), .. })) {
					self.step();

					init = self.parse_expression()?;
				} else {
					init = ast::Expr::Literal {
						literal: ast::Literal::Nil,
						pos,
					};
				}

				Ok(ast::Statement::Let { identifier, init, pos })
			}

			// Let function.
			Some(Token { token: TokenKind::Keyword(Keyword::Function), pos })
				if matches!(self.peek(), Some(Token { token: TokenKind::Identifier(_), .. })) => {
					self.step();

					let (identifier, id_pos) = self.parse_identifier()?;
					let (args, body) = self.parse_function()?;

					Ok(
						ast::Statement::Let {
							identifier,
							init: ast::Expr::Literal { literal: ast::Literal::Function { args, body }, pos },
							pos: id_pos,
						}
					)
				}

			// Return.
			Some(Token { token: TokenKind::Keyword(Keyword::Return), pos }) => {
				self.step();

				let expr = self.parse_expression()?;

				Ok(ast::Statement::Return { expr, pos })
			}

			// Break.
			Some(Token { token: TokenKind::Keyword(Keyword::Break), pos }) => {
				self.step();

				Ok(ast::Statement::Break { pos })
			}

			// While.
			Some(Token { token: TokenKind::Keyword(Keyword::While), pos }) => {
				self.step();

				let condition = self.parse_expression()?;
				self.expect(TokenKind::Keyword(Keyword::Do))?;
				let block = self.parse_block()?;
				self.expect(TokenKind::Keyword(Keyword::End))?;

				Ok(ast::Statement::While { condition, block, pos })
			}

			// For.
			Some(Token { token: TokenKind::Keyword(Keyword::For), pos }) => {
				self.step();

				let (identifier, _) = self.parse_identifier()?;
				self.expect(TokenKind::Keyword(Keyword::In))?;
				let expr = self.parse_expression()?;
				self.expect(TokenKind::Keyword(Keyword::Do))?;
				let block = self.parse_block()?;
				self.expect(TokenKind::Keyword(Keyword::End))?;

				Ok(ast::Statement::For { identifier, expr, block, pos })
			}

			// Expr.
			Some(token) => {
				self.token = Some(token);

				let expr = self.parse_expression()?;

				if matches!(self.token, Some(Token { token: TokenKind::Operator(Operator::Assign), .. })) {
					self.step();

					let right = self.parse_expression()?;

					Ok(
						ast::Statement::Assign { left: expr, right }
					)
				} else {
					Ok(ast::Statement::Expr(expr))
				}
			}

			// EOF.
			None => Err(Error::unexpected_eof()),
		}
	}


	/// Parse a single expression.
	fn parse_expression(&mut self) -> Result<ast::Expr, Error> {
		macro_rules! binop {
			($parse_higher_prec:expr, $check:expr) => {
				move |parser: &mut Self| parser.parse_binop($parse_higher_prec, $check)
			}
		}

		let parse_factor     = binop!(Self::parse_unop, Operator::is_factor);
		let parse_term       = binop!(parse_factor,     Operator::is_term);
		let parse_concat     = binop!(parse_term,       |&op| op == Operator::Concat);
		let parse_comparison = binop!(parse_concat,     Operator::is_comparison);
		let parse_equality   = binop!(parse_comparison, Operator::is_equality);
		let parse_and        = binop!(parse_equality,   |&op| op == Operator::And);
		let parse_or         = binop!(parse_and,        |&op| op == Operator::Or);

		parse_or(self)
	}


	/// Parse a higher precedence expression, optionally ending as a logical OR.
	fn parse_binop<P, F>(
		&mut self,
		mut parse_higher_prec_op: P,
		mut check: F,
	) -> Result<ast::Expr, Error>
	where
		P: FnMut(&mut Self) -> Result<ast::Expr, Error>,
		F: FnMut(&Operator) -> bool,
	{
		let mut expr = parse_higher_prec_op(self)?;

		loop {
			match self.token.take() {
				Some(Token { token: TokenKind::Operator(op), pos }) if check(&op) => {
					self.step();

					let right = parse_higher_prec_op(self)?;

					expr = ast::Expr::BinaryOp {
						left: expr.into(),
						op: op.into(),
						right: right.into(),
						pos,
					};
				}

				token => {
					self.token = token;
					break;
				}
			}
		}

		Ok(expr)
	}


	/// Parse a higher precedence expression, optionally starting with a unary operator.
	fn parse_unop(&mut self) -> Result<ast::Expr, Error> {
		match self.token.take() {
			Some(Token { token: TokenKind::Operator(op), pos }) if op.is_unary() => {
				self.step();

				let operand = self.parse_unop()?;

				Ok(ast::Expr::UnaryOp {
					op: op.into(),
					operand: operand.into(),
					pos,
				})
			}

			token => {
				self.token = token;
				self.parse_postfix()
			}
		}
	}


	fn parse_postfix(&mut self) -> Result<ast::Expr, Error> {
		let mut expr = self.parse_primary()?;

		loop {
			match self.token.take() {
				// Function call.
				Some(Token { token: TokenKind::OpenParens, pos }) => {
					self.step();

					let params = self.comma_sep(Self::parse_expression)?;
					self.expect(TokenKind::CloseParens)?;

					expr = ast::Expr::Call {
						function: expr.into(),
						params: params.into(),
						pos,
					}
				},

				// Subscript operator.
				Some(Token { token: TokenKind::OpenBracket, pos }) => {
					self.step();

					let field = self.parse_expression()?;
					self.expect(TokenKind::CloseBracket)?;

					expr = ast::Expr::Access {
						object: expr.into(),
						field: field.into(),
						pos,
					}
				},

				// Dot access operator.
				Some(Token { token: TokenKind::Operator(Operator::Dot), pos }) => {
					self.step();

					// Here, the identifier is a literal, and not a variable name. Hence, `var.id`
					// is equivalent to `var["id"]`, and not from `var[id]`.
					let (identifier, id_pos) = self.parse_identifier()?;
					let field = ast::Expr::Literal {
						literal: ast::Literal::Identifier(identifier),
						pos: id_pos,
					};

					expr = ast::Expr::Access {
						object: expr.into(),
						field: field.into(),
						pos,
					}
				},

				token => {
					self.token = token;
					break;
				}
			}
		}

		Ok(expr)
	}


	/// Parse a higher precedence expression.
	fn parse_primary(&mut self) -> Result<ast::Expr, Error> {
		match self.token.take() {
			// Identifier.
			Some(Token { token: TokenKind::Identifier(identifier), pos }) => {
				self.step();

				Ok(ast::Expr::Identifier { identifier, pos })
			}

			// Self.
			Some(Token { token: TokenKind::Keyword(Keyword::Self_), pos }) => {
				self.step();

				Ok(ast::Expr::Self_ { pos })
			}

			// Basic literal.
			Some(Token { token: TokenKind::Literal(literal), pos }) => {
				self.step();

				Ok(ast::Expr::Literal { literal: literal.into(), pos })
			}

			// Array literal.
			Some(Token { token: TokenKind::OpenBracket, pos }) => {
				self.step();

				let items = self.comma_sep(Self::parse_expression)?;
				self.expect(TokenKind::CloseBracket)?;

				Ok(ast::Expr::Literal {
					literal: ast::Literal::Array(items.into()),
					pos,
				})
			}

			// Dict literal.
			Some(Token { token: TokenKind::OpenDict, pos }) => {
				self.step();

				let items = self.comma_sep(|parser| {
					let (key, _) = parser.parse_identifier()?;
					parser.expect(TokenKind::Colon)?;
					let value = parser.parse_expression()?;

					Ok((key, value))
				})?;
				self.expect(TokenKind::CloseBracket)?;

				let mut dict = HashMap::new();

				for (id, value) in items.into_vec() { // Use vec's owned iterator.
					if dict.insert(id, value).is_some() { // Key already in dict.
						return Err(Error::duplicate_keys(pos))
					}
				}

				Ok(ast::Expr::Literal { literal: ast::Literal::Dict(dict), pos })
			}

			// Function literal.
			Some(Token { token: TokenKind::Keyword(Keyword::Function), pos }) => {
				self.step();
				let (args, body) = self.parse_function()?;

				Ok(ast::Expr::Literal { literal: ast::Literal::Function { args, body }, pos })
			}

			// Command blocks.
			Some(Token { token, pos }) if CommandBlockKind::from_token(&token).is_some() => {
				self.step();

				let commands = self.parse_command_block()?;

				Ok(
					ast::Expr::CommandBlock {
						// TODO: refactor this expect as a if-let guard when stabilized.
						kind: CommandBlockKind::from_token(&token).expect("invalid command token"),
						commands,
						pos
					}
				)
			}

			// If conditional.
			Some(Token { token: TokenKind::Keyword(Keyword::If), pos }) => {
				self.step();

				let condition = self.parse_expression()?;
				self.expect(TokenKind::Keyword(Keyword::Then))?;
				let then = self.parse_block()?;
				let otherwise = {
					let has_else = self.eat(|token| match token {
						Token { token: TokenKind::Keyword(Keyword::End), .. } => Ok(false),
						Token { token: TokenKind::Keyword(Keyword::Else), .. } => Ok(true),
						token => Err((Error::unexpected_msg(token.clone(), "end or else"), token)),
					})?;

					if has_else {
						let block = self.parse_block()?;
						self.expect(TokenKind::Keyword(Keyword::End))?;
						block
					} else {
						ast::Block::default()
					}
				};

				Ok(ast::Expr::If {
					condition: condition.into(),
					then,
					otherwise,
					pos,
				})
			}

			// Parenthesis.
			Some(Token { token: TokenKind::OpenParens, .. }) => {
				self.step();

				let expr = self.parse_expression()?;
				self.expect(TokenKind::CloseParens)?;

				Ok(expr)
			}

			// Some other unexpected token.
			Some(token) => {
				// We need to restore the token because it may be some delimiter.
				self.token = Some(token.clone());
				Err(Error::unexpected_msg(token, "expression"))
			}

			None => Err(Error::unexpected_eof()),
		}
	}


	/// Parse a identifier.
	fn parse_identifier(&mut self) -> Result<(ast::Symbol, SourcePos), Error> {
		self.eat(|token| match token {
			Token { token: TokenKind::Identifier(symbol), pos } => Ok((symbol, pos)),
			token => Err((Error::unexpected_msg(token.clone(), "identifier"), token)),
		})
	}


	/// Parse a function literal after the function keyword.
	/// Returns a pair of parameters and body.
	fn parse_function(&mut self) -> Result<(Box<[ast::Symbol]>, ast::Block), Error> {
		self.expect(TokenKind::OpenParens)?;
		let args = self.comma_sep(|parser| {
			let (id, _) = parser.parse_identifier()?;
			Ok(id)
		})?;
		self.expect(TokenKind::CloseParens)?;
		let body = self.parse_block()?;
		self.expect(TokenKind::Keyword(Keyword::End))?;

		Ok((args, body))
	}


	/// Comma-separated items.
	fn comma_sep<P, R>(&mut self, parse: P) -> Result<Box<[R]>, Error>
	where
		P: FnMut(&mut Self) -> Result<R, Error>,
	{
		self.sep_by(parse, |token| *token == TokenKind::Comma)
	}


	/// Items divided by a separator.
	/// A ending trailing separator is optional.
	fn sep_by<P, R, S>(&mut self, mut parse: P, mut sep: S) -> Result<Box<[R]>, Error>
	where
		P: FnMut(&mut Self) -> Result<R, Error>,
		S: FnMut(&TokenKind) -> bool,
	{
		let mut items = Vec::new();

		while let Ok(item) = parse(self) {
			items.push(item);

			match &self.token {
				Some(Token { token, .. }) if sep(token) => self.step(),
				_ => break,
			}
		}

		Ok(items.into())
	}
}