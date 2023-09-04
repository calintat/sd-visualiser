use std::{
    fmt::{Debug, Display},
    ops::Bound,
};

use egui::Vec2;
use good_lp::{variable, Expression, ResolutionError, Solution, Variable};
use itertools::Itertools;
use ordered_float::OrderedFloat;
use sd_core::{
    common::InOut,
    hypergraph::{
        generic::{Ctx, OperationWeight},
        traits::WithWeight,
    },
    lp::LpProblem,
    monoidal::graph::{MonoidalGraph, MonoidalOp},
    weak_map::WeakMap,
};
#[cfg(test)]
use serde::Serialize;
use store_interval_tree::{Interval, IntervalTree};
use thiserror::Error;

use crate::common::RADIUS_OPERATION;

#[derive(Clone, Debug, Error)]
pub enum LayoutError {
    #[error("An error occurred when solving the problem: {0}")]
    ResolutionError(#[from] ResolutionError),
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub struct NodeOffset<H, V> {
    pub(crate) node: Node<H, V>,
    input_offset: usize,
    inputs: usize,
    output_offset: usize,
    outputs: usize,
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub enum AtomType {
    Cup,
    Cap,
    Other,
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub enum Node<H, V> {
    Atom {
        h_pos: H,
        v_pos: V,
        extra_size: f32,
        atype: AtomType,
    },
    Swap {
        h_pos: H,
        v_top: V,
        v_bot: V,
        out_to_in: Vec<usize>,
    },
    Thunk {
        layout: LayoutInternal<H, V>,
    },
}

impl<V> Node<Variable, V> {
    #[must_use]
    pub fn h_min(&self) -> Expression {
        match self {
            Self::Atom {
                h_pos: pos,
                extra_size,
                ..
            } => *pos - *extra_size,
            Self::Swap { h_pos: pos, .. } => (*pos).into(),
            Self::Thunk { layout, .. } => layout.h_min.into(),
        }
    }

    #[must_use]
    pub fn h_max(&self) -> Expression {
        match self {
            Self::Atom {
                h_pos: pos,
                extra_size,
                ..
            } => *pos + *extra_size,
            Self::Swap { h_pos: pos, .. } => (*pos).into(),
            Self::Thunk { layout, .. } => layout.h_max.into(),
        }
    }
}

impl<H, V> Node<H, V> {
    pub fn unwrap_atom(&self) -> &H {
        match self {
            Self::Atom { h_pos, .. } | Self::Swap { h_pos, .. } => h_pos,
            Self::Thunk { .. } => panic!(),
        }
    }

    pub fn unwrap_thunk(&self) -> &LayoutInternal<H, V> {
        match self {
            Self::Atom { .. } | Self::Swap { .. } => panic!(),
            Self::Thunk { layout, .. } => layout,
        }
    }
}

impl<H, V> InOut for NodeOffset<H, V> {
    fn number_of_inputs(&self) -> usize {
        self.inputs
    }

    fn number_of_outputs(&self) -> usize {
        self.outputs
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub struct WireData<H, V> {
    pub h: H,
    pub v_top: V,
    pub v_bot: V,
}

impl<H, V: Default> From<H> for WireData<H, V> {
    fn from(value: H) -> Self {
        WireData {
            h: value,
            v_top: Default::default(),
            v_bot: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Serialize))]
pub struct LayoutInternal<H, V> {
    pub h_min: H,
    pub h_max: H,
    pub v_min: V,
    pub v_max: V,
    pub nodes: Vec<Vec<NodeOffset<H, V>>>,
    pub wires: Vec<Vec<WireData<H, V>>>,
}

impl<H, V> LayoutInternal<H, V> {
    pub fn inputs(&self) -> impl DoubleEndedIterator<Item = &H> {
        self.wires.first().unwrap().iter().map(|x| &x.h)
    }

    pub fn outputs(&self) -> impl DoubleEndedIterator<Item = &H> {
        self.wires.last().unwrap().iter().map(|x| &x.h)
    }
}

pub type HLayout<T> = LayoutInternal<f32, T>;
pub type Layout = LayoutInternal<f32, f32>;

impl<T> HLayout<T> {
    #[must_use]
    pub fn width(&self) -> f32 {
        self.h_max - self.h_min
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        (0..self.nodes.len())
            .map(|j| self.slice_height(j))
            .sum::<f32>()
            + 1.0
    }

    #[must_use]
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width(), self.height())
    }

    #[must_use]
    pub fn slice_height(&self, j: usize) -> f32 {
        self.nodes[j]
            .iter()
            .map(|n| match &n.node {
                Node::Atom { .. } => {
                    let in_width = if n.inputs < 2 {
                        1.0
                    } else {
                        self.wires[j][n.input_offset + n.inputs - 1].h
                            - self.wires[j][n.input_offset].h
                    };

                    let out_width = if n.outputs < 2 {
                        1.0
                    } else {
                        self.wires[j + 1][n.output_offset + n.outputs - 1].h
                            - self.wires[j + 1][n.output_offset].h
                    };

                    f32::sqrt((in_width + out_width) / 2.0) - 1.0
                }
                Node::Swap { out_to_in, .. } => out_to_in
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        (f32::sqrt((self.wires[j + 1][i].h - self.wires[j][*x].h).abs()) - 1.0)
                            .clamp(0.0, f32::INFINITY)
                    })
                    .max_by(|x, y| x.partial_cmp(y).unwrap())
                    .unwrap_or_default(),
                Node::Thunk { layout } => {
                    let height_above = self.wires[j][n.input_offset..]
                        .iter()
                        .zip(layout.inputs())
                        .map(|(x, y)| (f32::sqrt((x.h - y).abs()) - 1.0).clamp(0.0, f32::INFINITY))
                        .max_by(|x, y| x.partial_cmp(y).unwrap())
                        .unwrap_or_default();

                    let height_below = self.wires[j + 1][n.output_offset..]
                        .iter()
                        .zip(layout.outputs())
                        .map(|(x, y)| (f32::sqrt((x.h - y).abs()) - 1.0).clamp(0.0, f32::INFINITY))
                        .max_by(|x, y| x.partial_cmp(y).unwrap())
                        .unwrap_or_default();

                    layout.height() + height_above + height_below
                }
            })
            .max_by(|x, y| x.partial_cmp(y).unwrap())
            .unwrap_or_default()
            + 1.0
    }

    #[allow(clippy::cast_possible_truncation)]
    fn from_solution_h(layout: LayoutInternal<Variable, T>, solution: &impl Solution) -> Self {
        HLayout {
            h_min: solution.value(layout.h_min) as f32,
            h_max: solution.value(layout.h_max) as f32,
            v_min: layout.v_min,
            v_max: layout.v_max,
            nodes: layout
                .nodes
                .into_iter()
                .map(|ns| {
                    ns.into_iter()
                        .map(|n| NodeOffset {
                            node: match n.node {
                                Node::Atom {
                                    h_pos,
                                    v_pos,
                                    extra_size,
                                    atype,
                                } => Node::Atom {
                                    h_pos: (solution.value(h_pos) as f32),
                                    v_pos,
                                    extra_size,
                                    atype,
                                },
                                Node::Swap {
                                    h_pos,
                                    v_top,
                                    v_bot,
                                    out_to_in,
                                } => Node::Swap {
                                    h_pos: solution.value(h_pos) as f32,
                                    v_top,
                                    v_bot,
                                    out_to_in,
                                },
                                Node::Thunk { layout } => Node::Thunk {
                                    layout: Self::from_solution_h(layout, solution),
                                },
                            },
                            input_offset: n.input_offset,
                            inputs: n.inputs,
                            output_offset: n.output_offset,
                            outputs: n.outputs,
                        })
                        .collect()
                })
                .collect(),
            wires: layout
                .wires
                .into_iter()
                .map(|vs| {
                    vs.into_iter()
                        .map(|WireData { h, v_top, v_bot }| WireData {
                            h: solution.value(h) as f32,
                            v_top,
                            v_bot,
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

impl Layout {
    #[allow(clippy::cast_possible_truncation)]
    fn from_solution_v(layout: LayoutInternal<f32, Variable>, solution: &impl Solution) -> Self {
        Layout {
            h_min: layout.h_min,
            h_max: layout.h_max,
            v_min: solution.value(layout.v_min) as f32,
            v_max: solution.value(layout.v_max) as f32,
            nodes: layout
                .nodes
                .into_iter()
                .map(|ns| {
                    ns.into_iter()
                        .map(|n| NodeOffset {
                            node: match n.node {
                                Node::Atom {
                                    h_pos,
                                    v_pos,
                                    extra_size,
                                    atype,
                                } => Node::Atom {
                                    h_pos,
                                    v_pos: solution.value(v_pos) as f32,
                                    extra_size,
                                    atype,
                                },
                                Node::Swap {
                                    h_pos,
                                    v_top,
                                    v_bot,
                                    out_to_in,
                                } => Node::Swap {
                                    h_pos,
                                    v_top: solution.value(v_top) as f32,
                                    v_bot: solution.value(v_bot) as f32,
                                    out_to_in,
                                },
                                Node::Thunk { layout } => Node::Thunk {
                                    layout: Layout::from_solution_v(layout, solution),
                                },
                            },
                            input_offset: n.input_offset,
                            inputs: n.inputs,
                            output_offset: n.output_offset,
                            outputs: n.outputs,
                        })
                        .collect()
                })
                .collect(),
            wires: layout
                .wires
                .into_iter()
                .map(|vs| {
                    vs.into_iter()
                        .map(|WireData { h, v_top, v_bot }| WireData {
                            h,
                            v_top: solution.value(v_top) as f32,
                            v_bot: solution.value(v_bot) as f32,
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn h_layout_internal<T: Ctx>(
    graph: &MonoidalGraph<T>,
    expanded: &WeakMap<T::Thunk, bool>,
    problem: &mut LpProblem,
) -> LayoutInternal<Variable, ()>
where
    OperationWeight<T>: Display,
{
    // STEP 1. Generate variables for each layer.
    let min = problem.add_variable(variable().min(0.0));
    let max = problem.add_variable(variable().min(0.0));

    let mut nodes = Vec::default();
    let mut wires: Vec<Vec<WireData<Variable, ()>>> = Vec::default();

    let add_constraints_wires = |problem: &mut LpProblem, vs: &Vec<Variable>| {
        if let Some(x) = vs.first().copied() {
            problem.add_constraint((x - min).geq(0.5));
        }
        if let Some(x) = vs.last().copied() {
            problem.add_constraint((max - x).geq(0.5));
        }
        for (x, y) in vs.iter().copied().tuple_windows() {
            problem.add_constraint((y - x).geq(1.0));
        }
    };

    let add_constraints_nodes = |problem: &mut LpProblem, ns: &Vec<NodeOffset<Variable, ()>>| {
        if let Some(x) = ns.first() {
            problem.add_constraint((x.node.h_min() - min).geq(0.5));
        }
        if let Some(x) = ns.last() {
            problem.add_constraint((max - x.node.h_max()).geq(0.5));
        }
        for (x, y) in ns.iter().tuple_windows() {
            problem.add_constraint((y.node.h_min() - x.node.h_max()).geq(1.0));
        }
    };

    let inputs = problem.add_variables(
        variable().min(0.0),
        graph.free_inputs.len() + graph.bound_inputs.len(),
    );
    add_constraints_wires(problem, &inputs);
    wires.push(inputs.into_iter().map(Into::into).collect());
    for slice in &graph.slices {
        let outputs = problem.add_variables(variable().min(0.0), slice.number_of_outputs());
        add_constraints_wires(problem, &outputs);
        wires.push(outputs.into_iter().map(Into::into).collect());

        let mut input_offset = 0;
        let mut output_offset = 0;

        let ns = slice
            .ops
            .iter()
            .map(|op| {
                let node = match op {
                    MonoidalOp::Thunk { body, addr, .. } if expanded[addr] => Node::Thunk {
                        layout: h_layout_internal(body, expanded, problem),
                    },
                    MonoidalOp::Swap { out_to_in, .. } => Node::Swap {
                        h_pos: problem.add_variable(variable().min(0.0)),
                        v_top: (),
                        v_bot: (),
                        out_to_in: out_to_in.clone(),
                    },
                    MonoidalOp::Cup { .. } => Node::Atom {
                        h_pos: problem.add_variable(variable().min(0.0)),
                        v_pos: (),
                        extra_size: 0.0,
                        atype: AtomType::Cup,
                    },
                    MonoidalOp::Cap { .. } => Node::Atom {
                        h_pos: problem.add_variable(variable().min(0.0)),
                        v_pos: (),
                        extra_size: 0.0,
                        atype: AtomType::Cap,
                    },
                    MonoidalOp::Operation { addr } => Node::Atom {
                        h_pos: problem.add_variable(variable().min(0.0)),
                        v_pos: (),
                        extra_size: (addr.weight().to_string().chars().count().saturating_sub(1)
                            as f32
                            / 2.0)
                            * RADIUS_OPERATION,
                        atype: AtomType::Other,
                    },
                    _ => Node::Atom {
                        h_pos: problem.add_variable(variable().min(0.0)),
                        v_pos: (),
                        extra_size: 0.0,
                        atype: AtomType::Other,
                    },
                };
                let node_offset = NodeOffset {
                    node,
                    input_offset,
                    output_offset,
                    inputs: op.number_of_inputs(),
                    outputs: op.number_of_outputs(),
                };
                input_offset += op.number_of_inputs();
                output_offset += op.number_of_outputs();
                node_offset
            })
            .collect_vec();
        add_constraints_nodes(problem, &ns);
        nodes.push(ns);
    }

    // STEP 2. Add constraints between layers.
    for (nodes, (wires_i, wires_o)) in nodes.iter().zip(wires.iter().tuple_windows()) {
        let mut prev_op = None;
        for node in nodes {
            let ni = node.number_of_inputs();
            let no = node.number_of_outputs();

            assert_ne!(ni + no, 0, "Scalars are not allowed!");

            let ins = &wires_i[node.input_offset..node.input_offset + ni];
            let outs = &wires_o[node.output_offset..node.output_offset + no];

            let prev_in: Option<Expression> = if node.input_offset == 0 {
                None
            } else {
                wires_i
                    .get(node.input_offset - 1)
                    .copied()
                    .map(|x| x.h.into())
            };
            let prev_out: Option<Expression> = if node.output_offset == 0 {
                None
            } else {
                wires_o
                    .get(node.output_offset - 1)
                    .copied()
                    .map(|x| x.h.into())
            };

            // Distance constraints
            let constraints = [
                (prev_in.clone(), Some(node.node.h_min())),
                (prev_out.clone(), Some(node.node.h_min())),
                (prev_op.clone(), ins.first().copied().map(|x| x.h.into())),
                (prev_op, outs.first().copied().map(|x| x.h.into())),
                (prev_in, outs.first().copied().map(|x| x.h.into())),
                (prev_out, ins.first().copied().map(|x| x.h.into())),
            ];
            for (x, y) in constraints.into_iter().filter_map(|(x, y)| x.zip(y)) {
                problem.add_constraint((y - x).geq(1.0));
            }

            match &node.node {
                Node::Atom {
                    h_pos: pos, atype, ..
                } => {
                    match atype {
                        AtomType::Cup => {
                            for (x, y) in ins[1..].iter().copied().zip(outs) {
                                problem.add_constraint(Expression::eq(x.h.into(), y.h));
                            }
                            problem.add_constraint((*pos * 2.0).eq(ins[ni - 1].h + ins[0].h));
                            problem.add_objective(ins[ni - 1].h - ins[0].h);
                        }
                        AtomType::Cap => {
                            for (x, y) in outs[1..].iter().copied().zip(ins) {
                                problem.add_constraint(Expression::eq(x.h.into(), y.h));
                            }
                            problem.add_constraint((*pos * 2.0).eq(outs[no - 1].h + outs[0].h));
                            problem.add_objective(outs[no - 1].h - outs[0].h);
                        }
                        AtomType::Other => {
                            // Try to "squish" inputs and outputs
                            if ni >= 2 {
                                problem.add_objective((ins[ni - 1].h - ins[0].h) * ni as f32);
                            }

                            if no >= 2 {
                                problem.add_objective((outs[no - 1].h - outs[0].h) * no as f32);
                            }

                            // Fair averaging constraints
                            if ni > 0 {
                                let sum_ins: Expression = ins.iter().map(|x| x.h).sum();
                                problem.add_constraint((*pos * ni as f64).eq(sum_ins));
                            }
                            if no > 0 {
                                let sum_outs: Expression = outs.iter().map(|x| x.h).sum();
                                problem.add_constraint((*pos * no as f64).eq(sum_outs));
                            }
                        }
                    }
                }
                Node::Swap {
                    h_pos: pos,
                    out_to_in,
                    ..
                } => {
                    let in_outs: Expression = ins.iter().chain(outs.iter()).map(|x| x.h).sum();
                    problem.add_constraint((*pos * (ni + no) as f64).eq(in_outs));

                    for (i, j) in out_to_in.iter().copied().enumerate() {
                        let distance = problem.add_variable(variable().min(0.0));
                        problem.add_constraint((ins[j].h - outs[i].h).leq(distance));
                        problem.add_constraint((outs[i].h - ins[j].h).leq(distance));
                        problem.add_objective(distance);
                    }
                }
                Node::Thunk { layout, .. } => {
                    // Align internal wires with the external ones.
                    for (&x, &y) in ins.iter().zip(layout.inputs()) {
                        let distance = problem.add_variable(variable().min(0.0));
                        problem.add_constraint((x.h - y).leq(distance));
                        problem.add_constraint((y - x.h).leq(distance));
                        problem.add_objective(distance * 1.5);
                    }
                    for (&x, &y) in outs.iter().zip(layout.outputs()) {
                        let distance = problem.add_variable(variable().min(0.0));
                        problem.add_constraint((x.h - y).leq(distance));
                        problem.add_constraint((y - x.h).leq(distance));
                        problem.add_objective(distance * 1.5);
                    }

                    problem.add_objective(layout.h_max - layout.h_min);
                }
            }

            prev_op = Some(node.node.h_max());
        }
    }

    LayoutInternal {
        h_min: min,
        h_max: max,
        v_min: (),
        v_max: (),
        nodes,
        wires,
    }
}

#[allow(clippy::too_many_lines)]
fn v_layout_internal(
    problem: &mut LpProblem,
    h_layout: HLayout<()>,
) -> LayoutInternal<f32, Variable> {
    // Set up wires

    let wires: Vec<Vec<WireData<f32, Variable>>> = h_layout
        .wires
        .into_iter()
        .map(|vs| {
            vs.into_iter()
                .map(|v| {
                    let v_top = problem.add_variable(variable().min(0.0));
                    let v_bot = problem.add_variable(variable().min(0.0));

                    problem.add_constraint(Expression::leq(v_top.into(), v_bot));
                    problem.add_objective(v_bot - v_top);

                    WireData {
                        h: v.h,
                        v_top,
                        v_bot,
                    }
                })
                .collect()
        })
        .collect();

    // Set up min

    let v_min = problem.add_variable(variable().min(0.0));

    for x in wires.first().unwrap() {
        problem.add_constraint(Expression::eq(v_min.into(), x.v_top));
    }

    // Set up max

    let v_max = problem.add_variable(variable().min(0.0));

    for x in wires.last().unwrap() {
        problem.add_constraint(Expression::eq(v_max.into(), x.v_bot));
    }

    // Set up nodes

    let mut interval_tree: IntervalTree<OrderedFloat<f32>, Variable> = IntervalTree::new();

    let nodes = h_layout
        .nodes
        .into_iter()
        .zip(wires.iter().tuple_windows())
        .map(|(ns, (before, after))| {
            ns.into_iter()
                .map(|n| {
                    let (node, top, bottom, interval) = match n.node {
                        Node::Atom {
                            h_pos,
                            extra_size,
                            atype,
                            ..
                        } => {
                            let v_pos = problem.add_variable(variable().min(0.0));

                            let in_gap = if n.inputs < 2 {
                                1.0
                            } else {
                                f32::sqrt(
                                    before[n.input_offset + n.inputs - 1].h
                                        - before[n.input_offset].h,
                                )
                            };

                            let interval = Interval::new(
                                Bound::Included(OrderedFloat(h_pos - extra_size)),
                                Bound::Included(OrderedFloat(h_pos + extra_size)),
                            );

                            let out_gap = if n.outputs < 2 {
                                1.0
                            } else {
                                after[n.output_offset + n.outputs - 1].h - after[n.output_offset].h
                            };

                            let start = problem.add_variable(variable().min(0.0));
                            problem.add_constraint(Expression::eq(v_pos - in_gap, start));
                            let end = problem.add_variable(variable().min(0.0));
                            problem.add_constraint(Expression::eq(v_pos + out_gap, end));

                            (
                                Node::Atom {
                                    h_pos,
                                    v_pos,
                                    extra_size,
                                    atype,
                                },
                                start,
                                end,
                                interval,
                            )
                        }
                        Node::Swap {
                            h_pos, out_to_in, ..
                        } => {
                            let height = out_to_in
                                .iter()
                                .enumerate()
                                .map(|(i, x)| f32::sqrt((after[i].h - before[*x].h).abs()))
                                .max_by(|x, y| x.partial_cmp(y).unwrap())
                                .unwrap_or_default();
                            let v_top = problem.add_variable(variable().min(0.0));
                            let v_bot = problem.add_variable(variable().min(0.0));

                            problem.add_constraint(Expression::eq(v_top + height, v_bot));

                            let interval = Interval::point(OrderedFloat(h_pos));

                            (
                                Node::Swap {
                                    h_pos,
                                    v_top,
                                    v_bot,
                                    out_to_in,
                                },
                                v_top,
                                v_bot,
                                interval,
                            )
                        }
                        Node::Thunk { layout } => {
                            let layout = v_layout_internal(problem, layout);

                            let height_above = before[n.input_offset..]
                                .iter()
                                .zip(layout.inputs())
                                .map(|(x, y)| f32::sqrt((x.h - y).abs()))
                                .max_by(|x, y| x.partial_cmp(y).unwrap())
                                .unwrap_or_default();

                            let height_below = after[n.output_offset..]
                                .iter()
                                .zip(layout.outputs())
                                .map(|(x, y)| f32::sqrt((x.h - y).abs()))
                                .max_by(|x, y| x.partial_cmp(y).unwrap())
                                .unwrap_or_default();

                            let start = problem.add_variable(variable().min(0.0));
                            problem
                                .add_constraint(Expression::eq(layout.v_min - height_above, start));
                            let end = problem.add_variable(variable().min(0.0));
                            problem
                                .add_constraint(Expression::eq(layout.v_max - height_below, end));

                            let interval = Interval::new(
                                Bound::Included(OrderedFloat(layout.h_min)),
                                Bound::Included(OrderedFloat(layout.h_max)),
                            );

                            (Node::Thunk { layout }, start, end, interval)
                        }
                    };

                    for x in &before[n.input_offset..n.input_offset + n.inputs] {
                        problem.add_constraint(Expression::eq(top.into(), x.v_bot));
                    }

                    for x in &after[n.output_offset..n.output_offset + n.outputs] {
                        problem.add_constraint(Expression::eq(bottom.into(), x.v_top));
                    }

                    for x in interval_tree.query(&interval) {
                        problem.add_constraint(Expression::leq((*x.value()).into(), top));
                    }

                    interval_tree.insert(interval, bottom);

                    NodeOffset {
                        node,
                        input_offset: n.input_offset,
                        inputs: n.inputs,
                        output_offset: n.output_offset,
                        outputs: n.outputs,
                    }
                })
                .collect()
        })
        .collect();

    // Minimise entire graph
    // problem.add_objective((v_max - v_min) * 5.0);

    LayoutInternal {
        h_min: h_layout.h_min,
        h_max: h_layout.h_max,
        v_min,
        v_max,
        nodes,
        wires,
    }
}

pub fn layout<T: Ctx>(
    graph: &MonoidalGraph<T>,
    expanded: &WeakMap<T::Thunk, bool>,
) -> Result<Layout, LayoutError>
where
    OperationWeight<T>: Display,
{
    let mut problem = LpProblem::default();

    let layout = h_layout_internal(graph, expanded, &mut problem);
    problem.add_objective(layout.h_max);
    let h_solution = problem.minimise(good_lp::default_solver)?;

    problem = LpProblem::default();

    let v_layout = v_layout_internal(&mut problem, HLayout::from_solution_h(layout, &h_solution));

    let v_solution = problem.minimise(good_lp::default_solver)?;

    Ok(Layout::from_solution_v(v_layout, &v_solution))
}

#[cfg(test)]
mod tests {
    use sd_core::{examples, weak_map::WeakMap};

    use super::layout;

    #[test]
    fn int() {
        insta::with_settings!({sort_maps => true}, {
            insta::assert_ron_snapshot!(layout(&examples::int(), &WeakMap::default()).expect("Layout failed"));
        });
    }

    #[test]
    fn copy() {
        insta::with_settings!({sort_maps => true}, {
            insta::assert_ron_snapshot!(layout(&examples::copy(), &WeakMap::default()).expect("Layout failed"));
        });
    }
}
