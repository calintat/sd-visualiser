use super::spartan::{BindClause, Expr, Op, Thunk, Value, Variable};

#[allow(unused_variables)]
pub trait Visitor<'ast> {
    fn visit_variable(&mut self, variable: &'ast Variable) {}
    fn visit_bind_clause(&mut self, bind_clause: &'ast BindClause) {}
    fn visit_expr(&mut self, expr: &'ast Expr) {}
    fn visit_value(&mut self, value: &'ast Value) {}
    fn visit_op(&mut self, op: &'ast Op) {}
    fn visit_thunk(&mut self, thunk: &'ast Thunk) {}
    fn after_variable(&mut self, variable: &'ast Variable) {}
    fn after_bind_clause(&mut self, bind_clause: &'ast BindClause) {}
    fn after_expr(&mut self, expr: &'ast Expr) {}
    fn after_value(&mut self, value: &'ast Value) {}
    fn after_op(&mut self, op: &'ast Op) {}
    fn after_thunk(&mut self, thunk: &'ast Thunk) {}
}

pub trait Visitable {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast;
}

impl Visitable for Variable {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_variable(self);
        visitor.after_variable(self);
    }
}

impl Visitable for BindClause {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_bind_clause(self);
        self.var.walk(visitor);
        self.value.walk(visitor);
        visitor.after_bind_clause(self);
    }
}

impl Visitable for Expr {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_expr(self);
        self.binds.iter().for_each(|bind| bind.walk(visitor));
        self.value.walk(visitor);
        visitor.after_expr(self);
    }
}

impl Visitable for Value {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_value(self);
        match self {
            Value::Variable(var) => {
                var.walk(visitor);
            }
            Value::Op { op, vs, ds } => {
                op.walk(visitor);
                vs.iter().for_each(|v| v.walk(visitor));
                ds.iter().for_each(|d| d.walk(visitor));
            }
        }
        visitor.after_value(self);
    }
}

impl Visitable for Op {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_op(self);
        visitor.after_op(self);
    }
}

impl Visitable for Thunk {
    fn walk<'visitor, 'ast>(&'visitor self, visitor: &mut impl Visitor<'ast>)
    where
        'visitor: 'ast,
    {
        visitor.visit_thunk(self);
        self.args.iter().for_each(|var| var.walk(visitor));
        self.body.walk(visitor);
        visitor.after_thunk(self);
    }
}
