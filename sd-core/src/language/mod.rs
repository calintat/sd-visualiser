use std::{fmt::Debug, hash::Hash};

use derivative::Derivative;
use from_pest::{ConversionError, FromPest, Void};
use pest::{iterators::Pairs, RuleType};

pub mod chil;
pub mod spartan;

pub(crate) fn span_into_str(span: pest::Span) -> &str {
    span.as_str()
}

pub trait AsVar<V> {
    fn as_var(&self) -> &V;
}

impl<V> AsVar<V> for V {
    fn as_var(&self) -> &V {
        self
    }
}

pub trait Language {
    type Op: Clone + Eq + PartialEq + Hash + Debug + Send + Sync;
    type Var: Clone + Eq + PartialEq + Hash + Debug + Send + Sync;
    type Addr: Clone + Eq + PartialEq + Hash + Debug + Send + Sync;
    type VarDef: Clone + Eq + PartialEq + Hash + Debug + Send + Sync + AsVar<Self::Var>;

    type Rule: RuleType;
    fn expr_rule() -> Self::Rule;
    fn bind_rule() -> Self::Rule;
    fn value_rule() -> Self::Rule;
    fn thunk_rule() -> Self::Rule;
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Eq(bound = ""),
    PartialEq(bound = ""),
    Hash(bound = ""),
    Debug(bound = "")
)]
pub struct Expr<T: Language> {
    pub binds: Vec<Bind<T>>,
    pub values: Vec<Value<T>>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Eq(bound = ""),
    PartialEq(bound = ""),
    Hash(bound = ""),
    Debug(bound = "")
)]
pub struct Bind<T: Language> {
    pub def: T::VarDef,
    pub value: Value<T>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Eq(bound = ""),
    PartialEq(bound = ""),
    Hash(bound = ""),
    Debug(bound = "")
)]
pub enum Value<T: Language> {
    Variable(T::Var),
    Op { op: T::Op, args: Vec<Arg<T>> },
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Eq(bound = ""),
    PartialEq(bound = ""),
    Hash(bound = ""),
    Debug(bound = "")
)]
pub struct Thunk<T: Language> {
    pub addr: T::Addr,
    pub args: Vec<T::VarDef>,
    pub body: Expr<T>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Eq(bound = ""),
    PartialEq(bound = ""),
    Hash(bound = ""),
    Debug(bound = "")
)]
pub enum Arg<T: Language> {
    Value(Value<T>),
    Thunk(Thunk<T>),
}

impl<T: Language> Arg<T> {
    pub const fn value(&self) -> Option<&Value<T>> {
        match self {
            Self::Value(value) => Some(value),
            Self::Thunk(_) => None,
        }
    }

    pub const fn thunk(&self) -> Option<&Thunk<T>> {
        match self {
            Self::Value(_) => None,
            Self::Thunk(thunk) => Some(thunk),
        }
    }
}

// Conversions between languages

impl<T: Language> Expr<T> {
    pub fn into<U: Language>(self) -> Expr<U>
    where
        U::Op: From<T::Op>,
        U::Var: From<T::Var>,
        U::Addr: From<T::Addr>,
        U::VarDef: From<T::VarDef>,
    {
        Expr {
            binds: self.binds.into_iter().map(Bind::into).collect(),
            values: self.values.into_iter().map(Value::into).collect(),
        }
    }
}

impl<T: Language> Bind<T> {
    pub fn into<U: Language>(self) -> Bind<U>
    where
        U::Op: From<T::Op>,
        U::Var: From<T::Var>,
        U::Addr: From<T::Addr>,
        U::VarDef: From<T::VarDef>,
    {
        Bind {
            def: self.def.into(),
            value: self.value.into(),
        }
    }
}

impl<T: Language> Value<T> {
    pub fn into<U: Language>(self) -> Value<U>
    where
        U::Op: From<T::Op>,
        U::Var: From<T::Var>,
        U::Addr: From<T::Addr>,
        U::VarDef: From<T::VarDef>,
    {
        match self {
            Self::Variable(var) => Value::Variable(var.into()),
            Self::Op { op, args } => Value::Op {
                op: op.into(),
                args: args.into_iter().map(Arg::into).collect(),
            },
        }
    }
}

impl<T: Language> Thunk<T> {
    pub fn into<U: Language>(self) -> Thunk<U>
    where
        U::Op: From<T::Op>,
        U::Var: From<T::Var>,
        U::Addr: From<T::Addr>,
        U::VarDef: From<T::VarDef>,
    {
        Thunk {
            addr: self.addr.into(),
            args: self.args.into_iter().map(Into::into).collect(),
            body: self.body.into(),
        }
    }
}

impl<T: Language> Arg<T> {
    pub fn into<U: Language>(self) -> Arg<U>
    where
        U::Op: From<T::Op>,
        U::Var: From<T::Var>,
        U::Addr: From<T::Addr>,
        U::VarDef: From<T::VarDef>,
    {
        match self {
            Self::Value(value) => Arg::Value(value.into()),
            Self::Thunk(thunk) => Arg::Thunk(thunk.into()),
        }
    }
}

// Conversions from pest parse trees

impl<'pest, T> FromPest<'pest> for Expr<T>
where
    T: Language,
    T::Op: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Var: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Addr: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::VarDef: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
{
    type Rule = T::Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != T::expr_rule() {
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

impl<'pest, T> FromPest<'pest> for Bind<T>
where
    T: Language,
    T::Op: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Var: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Addr: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::VarDef: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
{
    type Rule = T::Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != T::bind_rule() {
            return Err(ConversionError::NoMatch);
        }
        let mut inner = pair.into_inner();
        let bind = Bind {
            def: FromPest::from_pest(&mut inner)?,
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

impl<'pest, T> FromPest<'pest> for Value<T>
where
    T: Language,
    T::Op: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Var: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Addr: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::VarDef: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
{
    type Rule = T::Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != T::value_rule() {
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

impl<'pest, T> FromPest<'pest> for Thunk<T>
where
    T: Language,
    T::Op: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Var: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Addr: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::VarDef: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
{
    type Rule = T::Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        let mut clone = pest.clone();
        let pair = clone.next().ok_or(ConversionError::NoMatch)?;
        if pair.as_rule() != T::thunk_rule() {
            return Err(ConversionError::NoMatch);
        }
        let mut inner = pair.into_inner();
        let thunk = Thunk {
            addr: FromPest::from_pest(&mut inner)?,
            args: FromPest::from_pest(&mut inner)?,
            body: FromPest::from_pest(&mut inner)?,
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

impl<'pest, T> FromPest<'pest> for Arg<T>
where
    T: Language,
    T::Op: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Var: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::Addr: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
    T::VarDef: FromPest<'pest, Rule = T::Rule, FatalError = Void>,
{
    type Rule = T::Rule;
    type FatalError = Void;

    fn from_pest(
        pest: &mut Pairs<'pest, Self::Rule>,
    ) -> Result<Self, ConversionError<Self::FatalError>> {
        Err(ConversionError::NoMatch)
            .or_else(|_: ConversionError<Void>| {
                let mut clone = pest.clone();
                let arg = Arg::Value(FromPest::from_pest(&mut clone)?);
                *pest = clone;
                Ok(arg)
            })
            .or_else(|_: ConversionError<Void>| {
                let mut clone = pest.clone();
                let arg = Arg::Thunk(FromPest::from_pest(&mut clone)?);
                *pest = clone;
                Ok(arg)
            })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{ffi::OsStr, path::Path};

    use super::{
        chil::tests::parse_chil,
        spartan::{self, tests::parse_sd},
    };

    pub fn parse(raw_path: &str) -> (&str, &str, spartan::Expr) {
        let path = Path::new(raw_path);
        match path.extension() {
            Some(ext) if ext == OsStr::new("sd") => {
                let (name, expr) = parse_sd(raw_path);
                ("sd", name, expr)
            }
            Some(ext) if ext == OsStr::new("chil") => {
                let (name, expr) = parse_chil(raw_path);
                ("chil", name, expr.into())
            }
            _ => unreachable!(),
        }
    }
}
