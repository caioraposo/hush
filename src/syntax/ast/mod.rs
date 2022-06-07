mod command;
pub mod fmt;

use super::{lexer, SourcePos};
pub use crate::symbol::Symbol;
pub use command::{
	ArgPart,
	ArgExpansion,
	ArgUnit,
	Argument,
	BasicCommand,
	Command,
	CommandBlock,
	CommandBlockKind,
	Redirection,
	RedirectionTarget,
};


/// A trait for types that can be produced from ill-formed syntax.
/// The resulting value should not be considered value for any use but a placeholder.
pub trait IllFormed {
	/// Create an ill-formed instance.
	fn ill_formed() -> Self;

	/// Check if the instance is ill-formed.
	/// This defaults to false for convenience in types where the ill-formed instance can
	/// actually be used.
	fn is_ill_formed(&self) -> bool { false }
}


impl IllFormed for () {
	fn ill_formed() -> Self { }
}


impl<A, B> IllFormed for (A, B)
where
	A: IllFormed,
	B: IllFormed,
{
	fn ill_formed() -> Self {
    (
			A::ill_formed(),
			B::ill_formed()
		)
	}

	fn is_ill_formed(&self) -> bool {
		self.0.is_ill_formed() || self.1.is_ill_formed()
	}
}


impl IllFormed for SourcePos {
	fn ill_formed() -> Self {
		Self { line: 0, column: 0, path: Symbol::default() }
	}

	fn is_ill_formed(&self) -> bool {
		*self == Self::ill_formed()
	}
}


impl IllFormed for Symbol {
	fn ill_formed() -> Self {
		Symbol::default()
	}

	fn is_ill_formed(&self) -> bool {
		*self == Self::ill_formed()
	}
}


/// A block is a list of statements, constituting a new scope.
#[derive(Debug)]
pub enum Block {
	IllFormed,
	Block(Box<[Statement]>),
}


impl Block {
	pub fn is_empty(&self) -> bool {
		matches!(self, Self::Block(block) if block.is_empty())
	}
}


impl Default for Block {
	fn default() -> Self {
		Self::Block(Default::default())
	}
}


impl From<Box<[Statement]>> for Block {
	fn from(block: Box<[Statement]>) -> Self {
		Self::Block(block)
	}
}


impl IllFormed for Block {
	fn ill_formed() -> Self {
		Self::IllFormed
	}

	fn is_ill_formed(&self) -> bool {
		matches!(self, Self::IllFormed)
	}
}


/// Literals of all types in the language.
/// Note that there are no literals for the error type.
#[derive(Debug)]
pub enum Literal {
	Nil,
	Bool(bool),
	Int(i64),
	Float(f64),
	Byte(u8),
	String(Box<[u8]>),
	Array(Box<[Expr]>),
	Dict(Box<[((Symbol, SourcePos), Expr)]>),
	Function {
		/// A list of parameters (identifiers).
		params: Box<[(Symbol, SourcePos)]>,
		body: Block,
		is_memoized: bool,
	},
	/// For the dot access operator, we want to be able to have identifiers as literal
	/// strings instead of names for variables. This variant should only be used in such
	/// case.
	Identifier(Symbol),
}


impl Default for Literal {
	fn default() -> Self {
		Self::Nil
	}
}


impl From<lexer::Literal> for Literal {
	fn from(lit: lexer::Literal) -> Self {
		match lit {
			lexer::Literal::Nil => Literal::Nil,
			lexer::Literal::True => Literal::Bool(true),
			lexer::Literal::False => Literal::Bool(false),
			lexer::Literal::Int(int) => Literal::Int(int),
			lexer::Literal::Float(float) => Literal::Float(float),
			lexer::Literal::Byte(byte) => Literal::Byte(byte),
			lexer::Literal::String(string) => Literal::String(string),
		}
	}
}


/// Unary operators.
#[derive(Debug)]
pub enum UnaryOp {
	Minus, // -
	Not,   // not
	Try,   // ?
}


impl UnaryOp {
	pub fn is_postfix(&self) -> bool {
		matches!(self, Self::Try)
	}
}


/// Warning, the following instance may panic if used with unmapped operators.
impl From<lexer::Operator> for UnaryOp {
	fn from(op: lexer::Operator) -> Self {
		match op {
			lexer::Operator::Minus => UnaryOp::Minus,
			lexer::Operator::Not => UnaryOp::Not,
			lexer::Operator::Try => UnaryOp::Try,
			_ => panic!("invalid operator"),
		}
	}
}


/// Binary operators.
/// Assignment/Access are not represented as operators, but directly as
/// statements/expressions instead.
#[derive(Debug)]
pub enum BinaryOp {
	Plus,  // +
	Minus, // -
	Times, // *
	Div,   // /
	Mod,   // %

	Equals,        // ==
	NotEquals,     // !=
	Greater,       // >
	GreaterEquals, // >=
	Lower,         // <
	LowerEquals,   // <=

	And, // and
	Or,  // or

	Concat, // ++
}


/// Warning, the following instance may panic if used with unmapped operators.
impl From<lexer::Operator> for BinaryOp {
	fn from(op: lexer::Operator) -> Self {
		match op {
			lexer::Operator::Plus => BinaryOp::Plus,
			lexer::Operator::Minus => BinaryOp::Minus,
			lexer::Operator::Times => BinaryOp::Times,
			lexer::Operator::Div => BinaryOp::Div,
			lexer::Operator::Mod => BinaryOp::Mod,
			lexer::Operator::Equals => BinaryOp::Equals,
			lexer::Operator::NotEquals => BinaryOp::NotEquals,
			lexer::Operator::Greater => BinaryOp::Greater,
			lexer::Operator::GreaterEquals => BinaryOp::GreaterEquals,
			lexer::Operator::Lower => BinaryOp::Lower,
			lexer::Operator::LowerEquals => BinaryOp::LowerEquals,
			lexer::Operator::And => BinaryOp::And,
			lexer::Operator::Or => BinaryOp::Or,
			lexer::Operator::Concat => BinaryOp::Concat,
			_ => panic!("invalid operator"),
		}
	}
}


/// Expressions of all kinds in the language.
#[derive(Debug)]
pub enum Expr {
	/// An ill-formed expr, produced by a parse error.
	IllFormed,
	/// The `self` keyword.
	Self_ {
		pos: SourcePos,
	},
	Identifier {
		identifier: Symbol,
		pos: SourcePos,
	},
	Literal {
		literal: Literal,
		pos: SourcePos,
	},
	UnaryOp {
		op: UnaryOp,
		operand: Box<Expr>,
		pos: SourcePos,
	},
	BinaryOp {
		left: Box<Expr>,
		op: BinaryOp,
		right: Box<Expr>,
		pos: SourcePos,
	},
	/// If-else expression.
	If {
		condition: Box<Expr>,
		then: Block,
		otherwise: Block,
		pos: SourcePos,
	},
	/// Field access ([]) operator.
	Access {
		object: Box<Expr>,
		field: Box<Expr>,
		pos: SourcePos,
	},
	/// Function call (()) operator.
	Call {
		function: Box<Expr>,
		args: Box<[Expr]>,
		pos: SourcePos,
	},
	CommandBlock {
		block: CommandBlock,
		pos: SourcePos,
	},
}


impl IllFormed for Expr {
	fn ill_formed() -> Self {
		Self::IllFormed
	}

	fn is_ill_formed(&self) -> bool {
		matches!(self, Self::IllFormed)
	}
}


/// Statements of all kinds in the language.
#[derive(Debug)]
pub enum Statement {
	/// An ill-formed statement, produced by a parse error.
	IllFormed,
	/// Introduces an identifier.
	Let {
		identifier: Symbol,
		init: Expr,
		pos: SourcePos,
	},
	Assign {
		left: Expr,
		right: Expr,
		pos: SourcePos,
	},
	Return {
		expr: Expr,
		pos: SourcePos,
	},
	Break {
		pos: SourcePos,
	},
	/// While loop.
	While {
		condition: Expr,
		block: Block,
		pos: SourcePos,
	},
	/// For loop. Also introduces an identifier.
	For {
		identifier: Symbol,
		expr: Expr,
		block: Block,
		pos: SourcePos,
	},
	Expr(Expr),
}


impl IllFormed for Statement {
	fn ill_formed() -> Self {
		Self::IllFormed
	}

	fn is_ill_formed(&self) -> bool {
		matches!(self, Self::IllFormed)
	}
}


/// The abstract syntax tree for a source file.
#[derive(Debug)]
pub struct Ast {
	/// The source path. May be something fictional, like "<stdin>".
	pub source: Symbol,
	/// The program.
	pub statements: Block,
}
