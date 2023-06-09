use std::{
    collections::HashSet,
    fmt::Display,
    hash::{BuildHasher, Hash},
};

use egui::{
    emath::RectTransform,
    epaint::{CircleShape, CubicBezierShape, RectShape},
    show_tooltip_at_pointer, vec2, Align2, Color32, Pos2, Rect, Response, Rounding, Sense, Shape,
    Vec2,
};
use indexmap::IndexSet;
use pretty::RcDoc;
use sd_core::{
    common::{InOut, InOutIter},
    hypergraph::{Graph, Node, Operation, OutPort},
    monoidal::{MonoidalGraph, MonoidalOp},
    prettyprinter::PrettyPrint,
};

use crate::layout::Layout;

const TOLERANCE: f32 = 0.1;

const TEXT_SIZE: f32 = 0.28;

const BOX_SIZE: Vec2 = vec2(0.4, 0.4);
const RADIUS_ARG: f32 = 0.05;
const RADIUS_COPY: f32 = 0.1;
const RADIUS_OPERATION: f32 = 0.2;

// Specifies how to transform a layout position to a screen position.
struct Transform {
    scale: f32,
    layout_bounds: Vec2,
    bounds: Rect,
    to_screen: RectTransform,
}

impl Transform {
    fn apply(&self, x: f32, y: f32) -> Pos2 {
        // Scale by a constant and translate to the centre of the bounding box.
        self.to_screen.transform_pos(
            Pos2::new(x * self.scale, y * self.scale)
                + (self.bounds.size() - self.layout_bounds * self.scale) / 2.0,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<V, E, S>(
    ui: &egui::Ui,
    response: &Response,
    layout: &Layout,
    scale: f32,
    graph: &mut MonoidalGraph<(V, Option<E>)>,
    selections: &mut HashSet<Operation<V, Option<E>>, S>,
    bounds: Rect,
    to_screen: RectTransform,
) -> Vec<Shape>
where
    V: Clone + Eq + PartialEq + Hash + Display + PrettyPrint,
    E: Clone + Eq + PartialEq + Hash + PrettyPrint,
    S: BuildHasher,
{
    let transform = Transform {
        scale,
        bounds,
        to_screen,
        layout_bounds: vec2(layout.width(), layout.height()),
    };

    let mut shapes = Vec::default();
    generate_shapes(
        ui,
        response,
        &mut shapes,
        0.0,
        layout,
        graph,
        selections,
        &transform,
    );
    shapes
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn generate_shapes<V, E, S>(
    ui: &egui::Ui,
    response: &Response,
    shapes: &mut Vec<Shape>,
    mut y_offset: f32,
    layout: &Layout,
    graph: &mut MonoidalGraph<(V, Option<E>)>,
    selections: &mut HashSet<Operation<V, Option<E>>, S>,
    transform: &Transform,
) where
    V: Clone + Eq + PartialEq + Hash + Display + PrettyPrint,
    E: Clone + Eq + PartialEq + Hash + PrettyPrint,
    S: BuildHasher,
{
    let default_stroke = ui.visuals().noninteractive().fg_stroke;
    let default_color = default_stroke.color;

    let mut hover_points = IndexSet::new();
    macro_rules! check_hover {
        ($path:expr, $port:expr) => {
            if let Some(hover_pos) = response.hover_pos() {
                if $path.contains_point(hover_pos, TOLERANCE * transform.scale) {
                    hover_points.insert(DummyValue::from_port($port));
                }
            }
        };
    }

    // Source
    for (&x, port) in layout.inputs().iter().zip(&graph.ordered_inputs) {
        let start = transform.apply(x, y_offset);
        let end = transform.apply(x, y_offset + 0.5);
        check_hover!([start, end], port);
        shapes.push(Shape::line_segment([start, end], default_stroke));
    }

    y_offset += 0.5;

    for (j, slice) in graph.slices.iter_mut().enumerate() {
        let slice_height = layout.slice_height(j);
        let y_input = y_offset;
        let y_output = y_offset + slice_height;

        y_offset = y_output;

        // If the slice is out of view, do not render anything.
        let top = transform.apply(0.0, y_input).y;
        let bottom = transform.apply(0.0, y_output).y;
        let range = transform.bounds.y_range();
        if bottom < *range.start() || top > *range.end() {
            continue;
        }

        let mut offset_i = 0;
        let mut offset_o = 0;
        for (i, op) in slice.ops.iter_mut().enumerate() {
            let ni = op.number_of_inputs();
            let no = op.number_of_outputs();

            let x_op = &layout.nodes[j][i];
            let x_ins = &layout.wires[j][offset_i..offset_i + ni];
            let x_outs = &layout.wires[j + 1][offset_o..offset_o + no];

            let id = response.id.with((j, i));

            match op {
                MonoidalOp::Swap { addr_1, addr_2 } => {
                    let in1 = transform.apply(x_ins[0], y_input);
                    let in2 = transform.apply(x_ins[1], y_input);
                    let out1 = transform.apply(x_outs[0], y_output);
                    let out2 = transform.apply(x_outs[1], y_output);

                    let bezier = CubicBezierShape::from_points_stroke(
                        vertical_out_vertical_in(in1, out2),
                        false,
                        Color32::TRANSPARENT,
                        default_stroke,
                    );
                    check_hover!(bezier, &addr_1.0);
                    shapes.push(Shape::CubicBezier(bezier));

                    let bezier = CubicBezierShape::from_points_stroke(
                        vertical_out_vertical_in(in2, out1),
                        false,
                        Color32::TRANSPARENT,
                        default_stroke,
                    );
                    check_hover!(bezier, &addr_2.0);
                    shapes.push(Shape::CubicBezier(bezier));
                }
                MonoidalOp::Thunk {
                    addr,
                    body,
                    expanded,
                    ..
                } if *expanded => {
                    let x_op = x_op.unwrap_thunk();
                    let diff = (slice_height - x_op.height()) / 2.0;
                    let y_min = y_input + diff;
                    let y_max = y_output - diff;
                    for (&x, port) in x_ins.iter().zip(&body.ordered_inputs) {
                        let thunk = transform.apply(x, y_min);
                        let input = transform.apply(x, y_input);
                        check_hover!([input, thunk], port);
                        shapes.push(Shape::line_segment([input, thunk], default_stroke));
                    }
                    for (&x, port) in x_outs.iter().zip(addr.outputs()) {
                        let thunk = transform.apply(x, y_max);
                        let output = transform.apply(x, y_output);
                        check_hover!([thunk, output], &port);
                        shapes.push(Shape::line_segment([thunk, output], default_stroke));
                    }
                    let thunk_rect = Rect::from_min_max(
                        transform.apply(x_op.min, y_min),
                        transform.apply(x_op.max, y_max),
                    );
                    let thunk_response =
                        ui.interact(thunk_rect.intersect(transform.bounds), id, Sense::click());
                    if thunk_response.clicked() {
                        *expanded = false;
                    }
                    shapes.push(Shape::rect_stroke(
                        thunk_rect,
                        Rounding::none(),
                        ui.style().interact(&thunk_response).fg_stroke,
                    ));
                    for &x in x_op
                        .inputs()
                        .iter()
                        .rev()
                        .take(addr.bound_graph_inputs().count())
                    {
                        let dot = transform.apply(x, y_min);
                        shapes.push(Shape::circle_filled(
                            dot,
                            RADIUS_ARG * transform.scale,
                            default_color,
                        ));
                    }
                    generate_shapes(
                        ui,
                        &thunk_response,
                        shapes,
                        y_min,
                        x_op,
                        body,
                        selections,
                        transform,
                    );
                }
                _ => {
                    let x_op = *x_op.unwrap_atom();
                    let y_op = (y_input + y_output) / 2.0;
                    let center = transform.apply(x_op, y_op);

                    let (x_ins_rem, x_outs_rem) = match op {
                        MonoidalOp::Cap { addr, intermediate } => {
                            for (&x, (port, _)) in x_ins.iter().zip(intermediate) {
                                let input = transform.apply(x, y_input);
                                let output = transform.apply(x, y_output);
                                check_hover!([input, output], port);
                                shapes.push(Shape::LineSegment {
                                    points: [input, output],
                                    stroke: default_stroke,
                                });
                            }
                            (
                                vec![],
                                vec![
                                    (x_outs[0], addr.0.clone()),
                                    (*x_outs.last().unwrap(), addr.0.clone()),
                                ],
                            )
                        }
                        MonoidalOp::Cup { addr, intermediate } => {
                            for (&x, (port, _)) in x_outs.iter().zip(intermediate) {
                                let input = transform.apply(x, y_input);
                                let output = transform.apply(x, y_output);
                                check_hover!([input, output], port);
                                shapes.push(Shape::LineSegment {
                                    points: [input, output],
                                    stroke: default_stroke,
                                });
                            }
                            (
                                vec![
                                    (x_ins[0], addr.0.clone()),
                                    (*x_ins.last().unwrap(), addr.0.clone()),
                                ],
                                vec![],
                            )
                        }
                        _ => (
                            x_ins
                                .iter()
                                .copied()
                                .zip(op.inputs().map(|(port, _)| port))
                                .collect::<Vec<_>>(),
                            x_outs
                                .iter()
                                .copied()
                                .zip(op.outputs().map(|(port, _)| port))
                                .collect::<Vec<_>>(),
                        ),
                    };

                    for (x, port) in x_ins_rem {
                        let input = transform.apply(x, y_input);
                        let bezier = CubicBezierShape::from_points_stroke(
                            vertical_out_horizontal_in(input, center),
                            false,
                            Color32::TRANSPARENT,
                            default_stroke,
                        );
                        check_hover!(bezier, &port);
                        shapes.push(bezier.into());
                    }

                    for (x, port) in x_outs_rem {
                        let output = transform.apply(x, y_output);
                        let bezier = CubicBezierShape::from_points_stroke(
                            horizontal_out_vertical_in(center, output),
                            false,
                            Color32::TRANSPARENT,
                            default_stroke,
                        );
                        check_hover!(bezier, &port);
                        shapes.push(bezier.into());
                    }

                    match op {
                        MonoidalOp::Copy { copies, .. } if *copies != 1 => {
                            shapes.push(Shape::circle_filled(
                                center,
                                RADIUS_COPY * transform.scale,
                                default_color,
                            ));
                        }
                        MonoidalOp::Operation { addr } => {
                            let selected = selections.contains(addr);
                            let op_rect =
                                Rect::from_center_size(center, BOX_SIZE * transform.scale);
                            let op_response = ui.interact(
                                op_rect.intersect(transform.bounds),
                                id,
                                Sense::click(),
                            );
                            if op_response.clicked() && !selections.remove(addr) {
                                selections.insert(addr.clone());
                            }
                            shapes.push(Shape::Circle(CircleShape {
                                center,
                                radius: RADIUS_OPERATION * transform.scale,
                                fill: ui
                                    .style()
                                    .interact_selectable(&op_response, selected)
                                    .bg_fill,
                                stroke: ui
                                    .style()
                                    .interact_selectable(&op_response, selected)
                                    .fg_stroke,
                            }));
                            if transform.scale > 10.0 {
                                ui.fonts(|fonts| {
                                    shapes.push(Shape::text(
                                        fonts,
                                        center,
                                        Align2::CENTER_CENTER,
                                        addr.weight(),
                                        egui::FontId::proportional(TEXT_SIZE * transform.scale),
                                        ui.visuals().strong_text_color(),
                                    ));
                                });
                            }
                        }
                        MonoidalOp::Thunk { expanded, .. } => {
                            let thunk_rect =
                                Rect::from_center_size(center, BOX_SIZE * transform.scale);
                            let thunk_response = ui.interact(
                                thunk_rect.intersect(transform.bounds),
                                id,
                                Sense::click(),
                            );
                            if thunk_response.clicked() {
                                *expanded = true;
                            }
                            shapes.push(Shape::Rect(RectShape {
                                rect: thunk_rect,
                                rounding: Rounding::none(),
                                fill: ui.style().interact(&thunk_response).bg_fill,
                                stroke: ui.style().interact(&thunk_response).fg_stroke,
                            }));
                        }
                        _ => (),
                    }
                }
            }

            offset_i += ni;
            offset_o += no;
        }
    }

    // Target
    for (&x, port) in layout.outputs().iter().zip(&graph.outputs) {
        let start = transform.apply(x, y_offset);
        let end = transform.apply(x, y_offset + 0.5);
        check_hover!([start, end], &port.link());
        shapes.push(Shape::line_segment([start, end], default_stroke));
    }

    // Show hover tooltips
    for e in hover_points {
        show_tooltip_at_pointer(ui.ctx(), egui::Id::new("hover_tooltip"), |ui| {
            ui.label(e.to_pretty())
        });
    }
}

fn vertical_out_horizontal_in(start: Pos2, end: Pos2) -> [Pos2; 4] {
    [
        start,
        Pos2::new(start.x, 0.2 * start.y + 0.8 * end.y),
        Pos2::new(0.6 * start.x + 0.4 * end.x, end.y),
        end,
    ]
}

fn horizontal_out_vertical_in(start: Pos2, end: Pos2) -> [Pos2; 4] {
    [
        start,
        Pos2::new(0.4 * start.x + 0.6 * end.x, start.y),
        Pos2::new(end.x, 0.8 * start.y + 0.2 * end.y),
        end,
    ]
}

fn vertical_out_vertical_in(start: Pos2, end: Pos2) -> [Pos2; 4] {
    [
        start,
        Pos2::new(start.x, 0.5 * start.y + 0.5 * end.y),
        Pos2::new(end.x, 0.5 * start.y + 0.5 * end.y),
        end,
    ]
}

trait ContainsPoint {
    // Check if a point lies on a line or curve (with the given tolerance).
    fn contains_point(self, point: Pos2, tolerance: f32) -> bool;
}

impl ContainsPoint for [Pos2; 2] {
    fn contains_point(self, point: Pos2, tolerance: f32) -> bool {
        let [from, to] = self;
        let distance = if from == to {
            (from - point).length()
        } else {
            let vec = to - from;
            let t = (point - from).dot(vec) / vec.length_sq();
            let t = t.clamp(0.0, 1.0);
            let projected = from + vec * t;
            (projected - point).length()
        };
        distance < tolerance
    }
}

const SAMPLES: u8 = 100;

impl ContainsPoint for CubicBezierShape {
    fn contains_point(self, point: Pos2, tolerance: f32) -> bool {
        (0..=SAMPLES).any(|t| {
            let t = f32::from(t) / f32::from(SAMPLES);
            let p = self.sample(t);
            p.distance(point) < tolerance
        })
    }
}

/// A dummy value is like a `spartan::Value` but with anonymous thunks and (possibly) free variables.
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum DummyValue<Op, Var> {
    Thunk,
    FreeVar,
    BoundVar(Var),
    Operation(Op, Vec<DummyValue<Op, Var>>),
}

impl<Op: Clone, Var: Clone> DummyValue<Op, Var> {
    fn from_port(out_port: &OutPort<Op, Option<Var>>) -> Self {
        match out_port.weight() {
            Some(var) => Self::BoundVar(var.clone()),
            None => match out_port.node() {
                None => Self::FreeVar, // technically should be unreachable
                Some(Node::Thunk(_)) => Self::Thunk,
                Some(Node::Operation(op)) => Self::Operation(
                    op.weight().clone(),
                    op.inputs()
                        .map(|in_port| Self::from_port(&in_port.link()))
                        .collect(),
                ),
            },
        }
    }
}

impl<Op: PrettyPrint, Var: PrettyPrint> PrettyPrint for DummyValue<Op, Var> {
    fn to_doc(&self) -> RcDoc<'_, ()> {
        match self {
            Self::Thunk => RcDoc::text("<thunk>"),
            Self::FreeVar => RcDoc::text("<free var>"),
            Self::BoundVar(var) => var.to_doc(),
            Self::Operation(op, vs) => {
                if vs.is_empty() {
                    op.to_doc()
                } else {
                    op.to_doc()
                        .append(RcDoc::text("("))
                        .append(RcDoc::intersperse(
                            vs.iter().map(PrettyPrint::to_doc),
                            RcDoc::text(",").append(RcDoc::space()),
                        ))
                        .append(RcDoc::text(")"))
                }
            }
        }
    }
}
