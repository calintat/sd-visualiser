use std::{
    collections::{BTreeSet, HashMap},
    fmt::Display,
};
use thiserror::Error;

use crate::hypergraph::{GraphNode, HyperGraph, HyperGraphError, Port, PortIndex};
use crate::language::{ActiveOp, BindClause, Expr, PassiveOp, Term, Thunk, Value, Variable};

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Passive(PassiveOp),
    Active(ActiveOp),
}

impl Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active(op) => op.fmt(f),
            Self::Passive(op) => op.fmt(f),
        }
    }
}

impl From<PassiveOp> for Op {
    fn from(p: PassiveOp) -> Self {
        Op::Passive(p)
    }
}

impl From<ActiveOp> for Op {
    fn from(a: ActiveOp) -> Self {
        Op::Active(a)
    }
}

pub type HyperGraphOp = HyperGraph<Op>;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("Error constructing hypergraph")]
    HyperGraphError(#[from] HyperGraphError),
    #[error("Couldn't find location of variable `{0}`")]
    VariableError(Variable),
}

impl Expr {
    pub(crate) fn free_variables(
        &self,
        bound: &mut BTreeSet<Variable>,
        vars: &mut BTreeSet<Variable>,
    ) {
        for bc in &self.binds {
            bc.free_variables(bound, vars);
        }
        self.value.free_variables(bound, vars);
    }

    pub fn to_hypergraph(&self) -> Result<HyperGraphOp, ConvertError> {
        let mut vars = BTreeSet::new();

        self.free_variables(&mut BTreeSet::new(), &mut vars);

        self.to_hypergraph_from_inputs(vars.into_iter().collect())
    }

    pub(crate) fn to_hypergraph_from_inputs(
        &self,
        inputs: Vec<Variable>,
    ) -> Result<HyperGraphOp, ConvertError> {
        let mut mapping: HashMap<Variable, Port> = HashMap::new();

        let mut graph = HyperGraphOp::new();

        let input_node = graph.add_node(GraphNode::Input, vec![], inputs.len())?;

        for (i, var) in inputs.into_iter().enumerate() {
            mapping.insert(
                var,
                Port {
                    node: input_node,
                    index: PortIndex(i),
                },
            );
        }

        self.process(&mut graph, &mut mapping)?;

        Ok(graph)
    }

    pub(crate) fn process(
        &self,
        graph: &mut HyperGraphOp,
        mapping: &mut HashMap<Variable, Port>,
    ) -> Result<(), ConvertError> {
        for bc in &self.binds {
            bc.process(graph, mapping)?;
        }
        let port = self.value.process(graph, mapping)?;
        graph.add_node(GraphNode::Output, vec![port], 0)?;
        Ok(())
    }
}

impl BindClause {
    pub(crate) fn free_variables(
        &self,
        bound: &mut BTreeSet<Variable>,
        vars: &mut BTreeSet<Variable>,
    ) {
        self.term.free_variables(bound, vars);
        bound.insert(self.var.clone());
    }

    pub(crate) fn process(
        &self,
        graph: &mut HyperGraphOp,
        mapping: &mut HashMap<Variable, Port>,
    ) -> Result<(), ConvertError> {
        let port = self.term.process(graph, mapping)?;
        mapping.insert(self.var.clone(), port);
        Ok(())
    }
}

impl Term {
    pub(crate) fn free_variables(&self, bound: &BTreeSet<Variable>, vars: &mut BTreeSet<Variable>) {
        match self {
            Term::Value(v) => v.free_variables(bound, vars),
            Term::ActiveOp(_, vs) => {
                for v in vs {
                    v.free_variables(bound, vars);
                }
            }
            Term::Thunk(thunk) => thunk.free_variables(bound, vars),
        }
    }

    pub(crate) fn process(
        &self,
        graph: &mut HyperGraphOp,
        mapping: &mut HashMap<Variable, Port>,
    ) -> Result<Port, ConvertError> {
        match self {
            Term::Value(v) => v.process(graph, mapping),
            Term::ActiveOp(op, vals) => {
                let mut inputs = vec![];
                for v in vals {
                    inputs.push(v.process(graph, mapping)?);
                }
                let node = graph.add_node(GraphNode::w(*op), inputs, 1)?;
                Ok(Port {
                    node,
                    index: PortIndex(0),
                })
            }
            Term::Thunk(thunk) => thunk.process(graph, mapping),
        }
    }
}

impl Value {
    pub(crate) fn free_variables(&self, bound: &BTreeSet<Variable>, vars: &mut BTreeSet<Variable>) {
        match self {
            Value::Var(v) => {
                if !bound.contains(v) {
                    vars.insert(v.clone());
                }
            }
            Value::PassiveOp(_, vs) => {
                for v in vs {
                    v.free_variables(bound, vars)
                }
            }
        }
    }

    pub(crate) fn process(
        &self,
        graph: &mut HyperGraphOp,
        mapping: &mut HashMap<Variable, Port>,
    ) -> Result<Port, ConvertError> {
        match self {
            Value::Var(v) => mapping
                .get(v)
                .copied()
                .ok_or(ConvertError::VariableError(v.clone())),
            Value::PassiveOp(op, vals) => {
                let mut inputs = vec![];
                for v in vals {
                    inputs.push(v.process(graph, mapping)?);
                }
                let node = graph.add_node(GraphNode::w(*op), inputs, 1)?;
                Ok(Port {
                    node,
                    index: PortIndex(0),
                })
            }
        }
    }
}

impl Thunk {
    pub(crate) fn free_variables(&self, bound: &BTreeSet<Variable>, vars: &mut BTreeSet<Variable>) {
        let mut bound = bound.clone();

        for arg in &self.args {
            bound.insert(arg.clone());
        }

        self.body.free_variables(&mut bound, vars);
    }

    pub(crate) fn process(
        &self,
        graph: &mut HyperGraphOp,
        mapping: &mut HashMap<Variable, Port>,
    ) -> Result<Port, ConvertError> {
        let mut vars = BTreeSet::new();

        self.free_variables(&BTreeSet::new(), &mut vars);

        let inputs = vars
            .iter()
            .map(|v| {
                mapping
                    .get(v)
                    .cloned()
                    .ok_or(ConvertError::VariableError(v.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut vars_vec: Vec<_> = vars.into_iter().collect();

        vars_vec.extend(self.args.clone());

        let graph_inner = self.body.to_hypergraph_from_inputs(vars_vec)?;

        let node = graph.add_node(
            GraphNode::Thunk {
                args: self.args.len(),
                body: Box::new(graph_inner),
            },
            inputs,
            1,
        )?;

        Ok(Port {
            node,
            index: PortIndex(0),
        })
    }
}

impl From<&str> for Variable {
    fn from(value: &str) -> Self {
        Variable(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use anyhow::{Context, Result};
    use from_pest::FromPest;
    use insta::assert_debug_snapshot;
    use pest::Parser;
    use rstest::{fixture, rstest};

    use crate::language::{Expr, Rule, SDParser, Variable};

    #[fixture]
    fn basic_program() -> Result<Expr> {
        let mut pairs = SDParser::parse(Rule::program, "bind x = 1() in x")
            .context("Could not parse basic program")?;
        Ok(Expr::from_pest(&mut pairs).unwrap())
    }

    #[fixture]
    fn free_vars() -> Result<Expr> {
        let mut pairs = SDParser::parse(Rule::program, "bind x = y in z")
            .context("Could not parse free variable program")?;
        Ok(Expr::from_pest(&mut pairs).unwrap())
    }

    // Make this something meaningful
    #[fixture]
    fn thunks() -> Result<Expr> {
        let mut pairs = SDParser::parse(
            Rule::program,
            "bind a = x0.1() in bind b = x0.bind z = plus(x0,y) in z in bind x = plus(a,b) in x",
        )
        .context("Could not parse thunk program")?;
        Ok(Expr::from_pest(&mut pairs).unwrap())
    }

    #[rstest]
    fn check_parse(
        basic_program: Result<Expr>,
        free_vars: Result<Expr>,
        thunks: Result<Expr>,
    ) -> Result<()> {
        basic_program?;
        free_vars?;
        thunks?;
        Ok(())
    }

    #[rstest]
    #[case(basic_program(), vec![])]
    #[case(free_vars(), vec!["y".into(), "z".into()])]
    #[case(thunks(), vec!["y".into()])]
    fn free_var_test(#[case] expr: Result<Expr>, #[case] vars: Vec<Variable>) -> Result<()> {
        let expr = expr?;
        let mut free_vars = BTreeSet::new();
        expr.free_variables(&mut BTreeSet::new(), &mut free_vars);

        assert_eq!(free_vars, vars.into_iter().collect());

        Ok(())
    }

    #[rstest]
    fn hypergraph_test_basic(basic_program: Result<Expr>) -> Result<()> {
        let graph = basic_program?.to_hypergraph()?;

        assert_debug_snapshot!(graph);

        Ok(())
    }

    #[rstest]
    fn hypergraph_test_free_var(free_vars: Result<Expr>) -> Result<()> {
        let graph = free_vars?.to_hypergraph()?;

        assert_debug_snapshot!(graph);

        Ok(())
    }

    #[rstest]
    fn hypergraph_test_thunk(thunks: Result<Expr>) -> Result<()> {
        let graph = thunks?.to_hypergraph()?;

        assert_debug_snapshot!(graph);

        Ok(())
    }
}
