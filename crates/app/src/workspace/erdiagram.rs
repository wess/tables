//! The ER-diagram modal. Fetches every table's structure, lays the tables out
//! in a grid of node cards, and draws foreign-key relationships as lines on a
//! canvas layer behind the cards.

use gpui::prelude::*;
use gpui::{canvas, div, point, px, Context, EventEmitter, PathBuilder, Point, Window};
use guise::prelude::*;

use crate::bridge;
use crate::state::AppState;
use model::TableStructure;

pub enum ErDiagramEvent {
    Close,
}

const NODE_W: f32 = 190.0;
const ROW_H: f32 = 18.0;
const HEADER_H: f32 = 26.0;
const MAX_ROWS: usize = 10;
const CELL_W: f32 = 250.0;
const CELL_H: f32 = 240.0;
const MARGIN: f32 = 20.0;

type Fetched = Vec<(String, TableStructure)>;

pub struct ErDiagramModal {
    #[allow(dead_code)]
    app: AppState,
    data: Signal<Option<Fetched>>,
}

struct Node {
    name: String,
    x: f32,
    y: f32,
    h: f32,
    columns: Vec<(String, String, bool)>, // name, type, is_pk
}

impl ErDiagramModal {
    pub fn new(connection_id: String, cx: &mut Context<Self>) -> Self {
        let app = AppState::get(cx);
        let data = Signal::new(cx, None);
        watch(cx, &data);

        let host = app.host.clone();
        let out = data.clone();
        let toasts = app.toasts.clone();
        bridge::run(
            cx,
            async move {
                let tables = host.list_tables(&connection_id).await?;
                let mut fetched = Vec::new();
                for table in &tables {
                    if table.kind != "table" {
                        continue;
                    }
                    let structure = host.table_structure(&table.name).await?;
                    fetched.push((table.name.clone(), structure));
                }
                Ok::<Fetched, String>(fetched)
            },
            move |result, cx| match result {
                Ok(fetched) => out.set(cx, Some(fetched)),
                Err(error) => toasts.error(cx, "Diagram failed", &error),
            },
        );

        ErDiagramModal { app, data }
    }
}

/// Grid layout of the fetched tables into positioned nodes plus FK edges.
fn layout(fetched: &Fetched) -> (Vec<Node>, Vec<(usize, usize)>) {
    let n = fetched.len();
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let mut nodes = Vec::with_capacity(n);
    for (i, (name, structure)) in fetched.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let columns: Vec<(String, String, bool)> = structure
            .columns
            .iter()
            .take(MAX_ROWS)
            .map(|c| (c.name.clone(), c.data_type.clone(), c.is_primary_key))
            .collect();
        let h = HEADER_H + columns.len() as f32 * ROW_H + 8.0;
        nodes.push(Node {
            name: name.clone(),
            x: MARGIN + col as f32 * CELL_W,
            y: MARGIN + row as f32 * CELL_H,
            h,
            columns,
        });
    }

    let index_of = |name: &str| fetched.iter().position(|(n, _)| n == name);
    let mut edges = Vec::new();
    for (i, (_, structure)) in fetched.iter().enumerate() {
        for fk in &structure.foreign_keys {
            if let Some(j) = index_of(&fk.referenced_table) {
                if i != j {
                    edges.push((i, j));
                }
            }
        }
    }
    (nodes, edges)
}

impl EventEmitter<ErDiagramEvent> for ErDiagramModal {}

impl Render for ErDiagramModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let theme = guise::theme::theme(cx);
        let header_bg = theme.color(ColorName::Blue, 7).hsla();
        let edge_color = theme.color(ColorName::Gray, 6).hsla();
        let pk_color = theme.color(ColorName::Yellow, 4).hsla();
        let text_color = theme.text().hsla();

        let body = match self.data.get(cx) {
            None => div()
                .flex()
                .h(px(400.0))
                .child(Center::new().child(Loader::new().size(Size::Sm)))
                .into_any_element(),
            Some(fetched) if fetched.is_empty() => div()
                .p(px(20.0))
                .child(Text::new("No tables to diagram").size(Size::Sm).dimmed())
                .into_any_element(),
            Some(fetched) => {
                let (nodes, edges) = layout(&fetched);
                let cols = (fetched.len() as f64).sqrt().ceil().max(1.0) as usize;
                let rows = fetched.len().div_ceil(cols);
                let total_w = MARGIN * 2.0 + cols as f32 * CELL_W;
                let total_h = MARGIN * 2.0 + rows as f32 * CELL_H;

                // Edge endpoints (node centers), captured for the canvas paint.
                let centers: Vec<Point<f32>> =
                    nodes.iter().map(|n| point(n.x + NODE_W / 2.0, n.y + n.h / 2.0)).collect();
                let edge_lines: Vec<(Point<f32>, Point<f32>)> =
                    edges.iter().map(|(a, b)| (centers[*a], centers[*b])).collect();

                let edges_canvas = canvas(
                    move |_bounds, _w, _cx| {},
                    move |bounds, _, window, _cx| {
                        let o = bounds.origin;
                        for (from, to) in &edge_lines {
                            let mut pb = PathBuilder::stroke(px(1.5));
                            pb.move_to(point(o.x + px(from.x), o.y + px(from.y)));
                            pb.line_to(point(o.x + px(to.x), o.y + px(to.y)));
                            if let Ok(path) = pb.build() {
                                window.paint_path(path, edge_color);
                            }
                        }
                    },
                )
                .absolute()
                .size_full();

                let mut canvas_layer = div()
                    .relative()
                    .w(px(total_w))
                    .h(px(total_h))
                    .child(edges_canvas);

                for node in &nodes {
                    let mut card = div()
                        .absolute()
                        .left(px(node.x))
                        .top(px(node.y))
                        .w(px(NODE_W))
                        .bg(colors.bg_surface)
                        .border_1()
                        .border_color(colors.border)
                        .rounded(px(6.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(4.0))
                                .bg(header_bg)
                                .text_color(gpui::hsla(0.0, 0.0, 1.0, 1.0))
                                .text_size(px(12.0))
                                .child(gpui::SharedString::from(node.name.clone())),
                        );
                    for (cname, ctype, is_pk) in &node.columns {
                        card = card.child(
                            div()
                                .flex()
                                .justify_between()
                                .px(px(8.0))
                                .py(px(2.0))
                                .text_size(px(11.0))
                                .font_family(crate::theme::MONO_FAMILY)
                                .text_color(if *is_pk { pk_color } else { text_color })
                                .child(gpui::SharedString::from(cname.clone()))
                                .child(
                                    div()
                                        .text_color(colors.text_muted)
                                        .child(gpui::SharedString::from(ctype.clone())),
                                ),
                        );
                    }
                    canvas_layer = canvas_layer.child(card);
                }

                div()
                    .id("er-scroll")
                    .max_h(px(560.0))
                    .max_w(px(900.0))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .child(canvas_layer)
                    .into_any_element()
            }
        };

        Modal::new()
            .title("ER Diagram")
            .width(940.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(ErDiagramEvent::Close)))
            .child(body)
    }
}
