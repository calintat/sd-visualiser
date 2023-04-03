use epaint::{emath::RectTransform, Color32, Pos2, Rect, Rounding, Shape, Stroke, Vec2};
use itertools::Itertools;
use sd_core::monoidal::MonoidalGraph;
use thiserror::Error;

use crate::layout::{layout, LayoutError};

pub const SCALE: f32 = 50.0;
pub const STROKE_WIDTH: f32 = 1.0;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error(transparent)]
    LayoutError(#[from] LayoutError),
}

pub fn render(
    graph: MonoidalGraph,
    bounds: Vec2,
    transform: RectTransform,
) -> Result<Vec<Shape>, RenderError> {
    let layout = layout(&graph)?;

    let width = layout.width as f32;
    let height = layout.slices.len() as f32;

    // Scale by a constant and translate to the centre of the bounding box.
    let transform = |pos: Pos2| {
        transform.transform_pos(Pos2 {
            x: pos.x * SCALE + (bounds.x - width * SCALE) / 2.0,
            y: pos.y * SCALE + (bounds.y - height * SCALE) / 2.0,
        })
    };

    let mut shapes: Vec<Shape> = Vec::new();

    for &x in &layout.slices[0] {
        let start = transform(Pos2 {
            x: x as f32,
            y: -0.25,
        });
        let end = transform(Pos2 {
            x: x as f32,
            y: 0.25,
        });
        shapes.push(Shape::line_segment(
            [start, end],
            Stroke::new(STROKE_WIDTH, Color32::BLACK),
        ));
    }

    for (y, slice) in graph.slices.iter().enumerate() {
        let mut offset_i = 0;
        let mut offset_o = 0;
        for op in slice.ops.iter() {
            let ni = op.number_of_inputs();
            let no = op.number_of_outputs();

            let input_wires = &layout.slices[y][offset_i..offset_i + ni];
            let output_wires = &layout.slices[y + 1][offset_o..offset_o + no];

            // Find the horizontal range that this operation covers.
            let (&min_x, &max_x) = input_wires
                .iter()
                .chain(output_wires)
                .minmax_by(|x, y| x.partial_cmp(y).unwrap())
                .into_option()
                .expect("Scalars are not allowed!");

            let min = transform(Pos2 {
                x: min_x as f32 - 0.25,
                y: y as f32 + 0.25,
            });
            let max = transform(Pos2 {
                x: max_x as f32 + 0.25,
                y: y as f32 + 0.75,
            });
            shapes.push(Shape::rect_stroke(
                Rect::from_min_max(min, max),
                Rounding::none(),
                Stroke::new(STROKE_WIDTH, Color32::BLACK),
            ));

            for &x in output_wires {
                let start = transform(Pos2 {
                    x: x as f32,
                    y: y as f32 + 0.75,
                });
                let end = transform(Pos2 {
                    x: x as f32,
                    y: y as f32 + 1.25,
                });
                shapes.push(Shape::line_segment(
                    [start, end],
                    Stroke::new(STROKE_WIDTH, Color32::BLACK),
                ));
            }

            offset_i += ni;
            offset_o += no;
        }
    }

    Ok(shapes)
}