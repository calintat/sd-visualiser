#![allow(clippy::clone_on_copy)]

use std::{
    fmt::{Display, Write},
    str::FromStr,
};

use from_pest::{ConversionError, FromPest, Void};
use pest::iterators::Pairs;
use pest_ast::FromPest;
use pest_derive::Parser;
#[cfg(test)]
use serde::Serialize;

use super::{span_into_str, Fresh, OpInfo};
use crate::common::{Empty, Matchable, Unit};

pub struct Spartan;

impl super::Language for Spartan {
    type Op = Op;
    type Var = Variable;
    type Addr = Unit;
    type VarDef = Variable;
    type BlockAddr = Empty;
}

pub type Expr = super::Expr<Spartan>;
pub type Bind = super::Bind<Spartan>;
pub type Value = super::Value<Spartan>;
pub type Thunk = super::Thunk<Spartan>;

#[derive(Parser)]
#[grammar = "language/spartan.pest"]
pub struct SpartanParser;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub enum Op {
    Plus,
    Minus,
    Times,
    Div,
    Rem,
    And,
    Or,
    Not,
    If,
    Eq,
    Neq,
    Lt,
    Leq,
    Gt,
    Geq,
    App,
    Lambda,
    Atom,
    Deref,
    Assign,
    Tuple,
    Detuple,
    Bool(bool),
    Number(usize),
}

impl Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plus => f.write_char('+'),
            Self::Minus => f.write_char('-'),
            Self::Times => f.write_char('×'),
            Self::Div => f.write_char('/'),
            Self::Rem => f.write_char('%'),
            Self::And => f.write_char('∧'),
            Self::Or => f.write_char('∨'),
            Self::Not => f.write_char('¬'),
            Self::If => f.write_str("if"),
            Self::Eq => f.write_char('='),
            Self::Neq => f.write_char('≠'),
            Self::Lt => f.write_char('<'),
            Self::Leq => f.write_char('≤'),
            Self::Gt => f.write_char('>'),
            Self::Geq => f.write_char('≥'),
            Self::App => f.write_char('@'),
            Self::Lambda => f.write_char('λ'),
            Self::Atom => f.write_char('&'),
            Self::Deref => f.write_char('!'),
            Self::Assign => f.write_str(":="),
            Self::Tuple => f.write_str("()"),
            Self::Detuple => f.write_str(")("),
            Self::Bool(b) => f.write_str(&b.to_string()),
            Self::Number(n) => f.write_str(&n.to_string()),
        }
    }
}

impl FromStr for Op {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plus" => Ok(Self::Plus),
            "minus" => Ok(Self::Minus),
            "times" => Ok(Self::Times),
            "div" => Ok(Self::Div),
            "rem" => Ok(Self::Rem),
            "and" => Ok(Self::And),
            "or" => Ok(Self::Or),
            "not" => Ok(Self::Not),
            "if" => Ok(Self::If),
            "eq" => Ok(Self::Eq),
            "neq" => Ok(Self::Neq),
            "lt" => Ok(Self::Lt),
            "leq" => Ok(Self::Leq),
            "gt" => Ok(Self::Gt),
            "geq" => Ok(Self::Geq),
            "app" => Ok(Self::App),
            "lambda" => Ok(Self::Lambda),
            "atom" => Ok(Self::Atom),
            "deref" => Ok(Self::Deref),
            "assign" => Ok(Self::Assign),
            "tuple" => Ok(Self::Tuple),
            "detuple" => Ok(Self::Detuple),
            "true" => Ok(Self::Bool(true)),
            "false" => Ok(Self::Bool(false)),
            _ => s.parse().map(Self::Number).map_err(|_err| ()),
        }
    }
}

impl Matchable for Op {
    fn is_match(&self, query: &str) -> bool {
        match self {
            Self::Plus => query == "plus",
            Self::Minus => query == "minus",
            Self::Times => query == "times",
            Self::Div => query == "div",
            Self::Rem => query == "rem",
            Self::And => query == "and",
            Self::Or => query == "or",
            Self::Not => query == "not",
            Self::If => query == "if",
            Self::Eq => query == "eq",
            Self::Neq => query == "neq",
            Self::Lt => query == "lt",
            Self::Leq => query == "leq",
            Self::Gt => query == "gt",
            Self::Geq => query == "geq",
            Self::App => query == "app",
            Self::Lambda => query == "lambda",
            Self::Atom => query == "atom",
            Self::Deref => query == "deref",
            Self::Assign => query == "assign",
            Self::Tuple => query == "tuple",
            Self::Detuple => query == "detuple",
            Self::Bool(b) => b.to_string() == query,
            Self::Number(n) => n.to_string() == query,
        }
    }
}

impl<'pest> FromPest<'pest> for Op {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != Rule::op {
            return Err(ConversionError::NoMatch);
        }
        let op = pair
            .as_str()
            .parse()
            .map_err(|()| ConversionError::NoMatch)?;
        *pest = clone;
        Ok(op)
    }
}

impl OpInfo<Spartan> for Op {}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, FromPest)]
#[cfg_attr(test, derive(Serialize))]
#[pest_ast(rule(Rule::variable))]
pub struct Variable(#[pest_ast(outer(with(span_into_str), with(str::to_string)))] pub String);

impl Matchable for Variable {
    fn is_match(&self, query: &str) -> bool {
        self.0 == query
    }
}

impl Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Fresh for Variable {
    fn fresh(number: usize) -> Self {
        Self(format!("?{number}"))
    }
}

impl<'pest> FromPest<'pest> for Unit {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(_: &mut Pairs<'pest, Rule>) -> Result<Self, ConversionError<Void>> {
        Ok(Unit)
    }
}

// Conversions from pest parse trees

impl<'pest> FromPest<'pest> for Expr {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != Rule::expr {
            return Err(ConversionError::NoMatch);
        }
        let mut inner = pair.into_inner();
        let expr = Expr {
            binds: FromPest::from_pest(&mut inner)?,
            values: FromPest::from_pest(&mut inner)?,
        };
        if inner.clone().next().is_some() {
            return Err(ConversionError::Extraneous {
                current_node: stringify!(Expr),
            });
        }
        *pest = clone;
        Ok(expr)
    }
}

impl<'pest> FromPest<'pest> for Bind {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != Rule::bind {
            return Err(ConversionError::NoMatch);
        }
        let mut inner = pair.into_inner();
        let bind = Bind {
            defs: FromPest::from_pest(&mut inner)?,
            value: FromPest::from_pest(&mut inner)?,
        };
        if inner.next().is_some() {
            return Err(ConversionError::Extraneous {
                current_node: stringify!(Bind),
            });
        }
        *pest = clone;
        Ok(bind)
    }
}

impl<'pest> FromPest<'pest> for Value {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != Rule::value {
            return Err(ConversionError::NoMatch);
        }
        let value = Err(ConversionError::NoMatch)
            .or_else(|_: ConversionError<Void>| {
                let mut inner = pair.clone().into_inner();
                let value = Value::Variable(FromPest::from_pest(&mut inner)?);
                if inner.next().is_some() {
                    return Err(ConversionError::Extraneous {
                        current_node: stringify!(Value),
                    });
                }
                Ok(value)
            })
            .or_else(|_: ConversionError<Void>| {
                let mut inner = pair.clone().into_inner();
                let value = Value::Thunk(FromPest::from_pest(&mut inner)?);
                if inner.next().is_some() {
                    return Err(ConversionError::Extraneous {
                        current_node: stringify!(Value),
                    });
                }
                Ok(value)
            })
            .or_else(|_: ConversionError<Void>| {
                let mut inner = pair.into_inner();
                let value = Value::Op {
                    op: FromPest::from_pest(&mut inner)?,
                    args: FromPest::from_pest(&mut inner)?,
                };
                if inner.next().is_some() {
                    return Err(ConversionError::Extraneous {
                        current_node: stringify!(Value),
                    });
                }
                Ok(value)
            })?;
        *pest = clone;
        Ok(value)
    }
}

impl<'pest> FromPest<'pest> for Thunk {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != Rule::thunk {
            return Err(ConversionError::NoMatch);
        }
        let mut inner = pair.into_inner();
        let thunk = Thunk {
            addr: FromPest::from_pest(&mut inner)?,
            args: FromPest::from_pest(&mut inner)?,
            body: FromPest::from_pest(&mut inner)?,
            blocks: vec![],
        };
        if inner.next().is_some() {
            return Err(ConversionError::Extraneous {
                current_node: stringify!(Thunk),
            });
        }
        *pest = clone;
        Ok(thunk)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use dir_test::{dir_test, Fixture};
    use from_pest::FromPest;
    use pest::Parser;

    use super::{Expr, Rule, SpartanParser};

    pub fn parse_sd(raw_path: &str) -> (&str, Expr) {
        let path = Path::new(raw_path);
        let program = std::fs::read_to_string(path).unwrap();
        let mut pairs = SpartanParser::parse(Rule::program, &program).unwrap_or_else(|err| {
            panic!(
                "could not parse program {:?}\n{err:?}",
                path.file_stem().unwrap()
            )
        });
        let name = path.file_stem().unwrap().to_str().unwrap();
        let expr = Expr::from_pest(&mut pairs).unwrap();
        (name, expr)
    }

    #[allow(clippy::needless_pass_by_value)]
    #[dir_test(dir: "$CARGO_MANIFEST_DIR/../examples", glob: "**/*.sd", loader: crate::language::spartan::tests::parse_sd, postfix: "check_parse")]
    fn check_parse(fixture: Fixture<(&str, Expr)>) {
        let (_name, _expr) = fixture.content();
    }
}
