//! The chart modal. Plots the current query result: pick a label column and a
//! numeric value column, then render a bar (divs), line, or pie chart (canvas
//! paths).

use std::f32::consts::PI;

use gpui::prelude::*;
use gpui::{canvas, div, point, px, Context, Entity, EventEmitter, Hsla, PathBuilder, Window};
use guise::prelude::*;
use serde_json::Value;

use crate::sheet::Sheet;
use model::Row;

pub enum ChartEvent {
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bar,
    Line,
    Pie,
}

const MAX_POINTS: usize = 40;
const CHART_H: f32 = 280.0;

pub struct ChartModal {
    columns: Vec<String>,
    rows: Vec<Row>,
    label_col: Entity<Select>,
    value_col: Entity<Select>,
    kind: Signal<Kind>,
}

fn num(v: Option<&Value>) -> f32 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) as f32,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn is_numeric_col(rows: &[Row], col: &str) -> bool {
    rows.iter()
        .filter_map(|r| r.get(col))
        .find(|v| !v.is_null())
        .map(|v| matches!(v, Value::Number(_)) || matches!(v, Value::String(s) if s.trim().parse::<f64>().is_ok()))
        .unwrap_or(false)
}

impl ChartModal {
    pub fn new(columns: Vec<String>, rows: Vec<Row>, cx: &mut Context<Self>) -> Self {
        let label_idx = columns.iter().position(|c| !is_numeric_col(&rows, c)).unwrap_or(0);
        let value_idx = columns
            .iter()
            .position(|c| is_numeric_col(&rows, c))
            .unwrap_or(columns.len().saturating_sub(1));

        let label_col = cx.new({
            let cols = columns.clone();
            move |cx| Select::new(cx).label("Labels").data(cols).selected(label_idx).size(Size::Xs)
        });
        let value_col = cx.new({
            let cols = columns.clone();
            move |cx| Select::new(cx).label("Values").data(cols).selected(value_idx).size(Size::Xs)
        });
        let kind = Signal::new(cx, Kind::Bar);
        watch(cx, &kind);

        // Redraw when the Labels/Values column selection changes.
        cx.subscribe(&label_col, |_this, _sel, _e: &SelectEvent, cx| cx.notify()).detach();
        cx.subscribe(&value_col, |_this, _sel, _e: &SelectEvent, cx| cx.notify()).detach();

        ChartModal { columns, rows, label_col, value_col, kind }
    }

    /// (label, value) pairs from the selected columns, capped for readability.
    fn data(&self, cx: &Context<Self>) -> Vec<(String, f32)> {
        let label = self
            .label_col
            .read(cx)
            .selected_index()
            .and_then(|i| self.columns.get(i))
            .cloned()
            .unwrap_or_default();
        let value = self
            .value_col
            .read(cx)
            .selected_index()
            .and_then(|i| self.columns.get(i))
            .cloned()
            .unwrap_or_default();
        self.rows
            .iter()
            .take(MAX_POINTS)
            .map(|row| {
                let l = match row.get(&label) {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) if !v.is_null() => v.to_string(),
                    _ => String::new(),
                };
                (l, num(row.get(&value)))
            })
            .collect()
    }
}

fn palette(theme: &guise::Theme) -> [Hsla; 8] {
    [
        theme.color(ColorName::Blue, 5).hsla(),
        theme.color(ColorName::Teal, 5).hsla(),
        theme.color(ColorName::Orange, 5).hsla(),
        theme.color(ColorName::Grape, 5).hsla(),
        theme.color(ColorName::Green, 5).hsla(),
        theme.color(ColorName::Red, 5).hsla(),
        theme.color(ColorName::Cyan, 5).hsla(),
        theme.color(ColorName::Yellow, 5).hsla(),
    ]
}

impl EventEmitter<ChartEvent> for ChartModal {}

impl ChartModal {
    fn bar_chart(&self, data: &[(String, f32)], colors: [Hsla; 8], muted: Hsla) -> gpui::AnyElement {
        let max = data.iter().map(|(_, v)| *v).fold(0.0_f32, f32::max).max(1.0);
        let mut bars = div().flex().items_end().gap_1().h(px(CHART_H));
        let mut labels = div().flex().gap_1();
        for (i, (label, value)) in data.iter().enumerate() {
            let h = (value / max) * (CHART_H - 10.0);
            bars = bars.child(
                div()
                    .flex_1()
                    .h(px(h.max(1.0)))
                    .bg(colors[i % colors.len()])
                    .rounded_t(px(2.0)),
            );
            labels = labels.child(
                div()
                    .flex_1()
                    .text_size(px(9.0))
                    .text_color(muted)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(gpui::SharedString::from(label.clone())),
            );
        }
        div().flex().flex_col().gap_1().child(bars).child(labels).into_any_element()
    }

    fn line_chart(&self, data: Vec<(String, f32)>, color: Hsla) -> gpui::AnyElement {
        let max = data.iter().map(|(_, v)| v).fold(0.0_f32, |a, b| a.max(*b)).max(1.0);
        canvas(
            move |_bounds, _w, _cx| {},
            move |bounds, _, window, _cx| {
                let o = bounds.origin;
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let n = data.len();
                if n == 0 {
                    return;
                }
                let mut pb = PathBuilder::stroke(px(2.0));
                for (i, (_, v)) in data.iter().enumerate() {
                    let fx = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
                    let x = o.x + px(w * fx);
                    let y = o.y + px(h - h * (v / max));
                    if i == 0 {
                        pb.move_to(point(x, y));
                    } else {
                        pb.line_to(point(x, y));
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, color);
                }
            },
        )
        .h(px(CHART_H))
        .w_full()
        .into_any_element()
    }

    fn pie_chart(&self, data: Vec<(String, f32)>, colors: [Hsla; 8]) -> gpui::AnyElement {
        let total: f32 = data.iter().map(|(_, v)| v).sum::<f32>().max(1.0);
        canvas(
            move |_bounds, _w, _cx| {},
            move |bounds, _, window, _cx| {
                let o = bounds.origin;
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let cx0 = w / 2.0;
                let cy = h / 2.0;
                let r = (w.min(h) / 2.0) - 10.0;
                let mut start = -PI / 2.0;
                for (i, (_, v)) in data.iter().enumerate() {
                    let sweep = (v / total) * 2.0 * PI;
                    let steps = ((sweep.abs() / (PI / 24.0)).ceil() as usize).max(2);
                    let mut pb = PathBuilder::fill();
                    pb.move_to(point(o.x + px(cx0), o.y + px(cy)));
                    for s in 0..=steps {
                        let a = start + sweep * (s as f32 / steps as f32);
                        pb.line_to(point(
                            o.x + px(cx0 + r * a.cos()),
                            o.y + px(cy + r * a.sin()),
                        ));
                    }
                    pb.close();
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, colors[i % colors.len()]);
                    }
                    start += sweep;
                }
            },
        )
        .h(px(CHART_H))
        .w_full()
        .into_any_element()
    }
}

impl Render for ChartModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = guise::theme::theme(cx);
        let colors = palette(theme);
        let muted = theme.dimmed().hsla();
        let kind = *self.kind.read(cx);
        let data = self.data(cx);

        let type_button = |id: &'static str, label: &'static str, this_kind: Kind| {
            Button::new(id, label)
                .size(Size::Xs)
                .variant(if kind == this_kind { Variant::Light } else { Variant::Subtle })
                .on_click(cx.listener(move |this, _, _, cx| this.kind.set(cx, this_kind)))
        };

        let controls = Group::new()
            .gap(Size::Xs)
            .align(Align::End)
            .child(type_button("chart-bar", "Bar", Kind::Bar))
            .child(type_button("chart-line", "Line", Kind::Line))
            .child(type_button("chart-pie", "Pie", Kind::Pie))
            .child(Divider::vertical())
            .child(div().w(px(160.0)).child(self.label_col.clone()))
            .child(div().w(px(160.0)).child(self.value_col.clone()));

        let body = if data.is_empty() {
            div().p(px(20.0)).child(Text::new("No data to chart").size(Size::Sm).dimmed()).into_any_element()
        } else {
            match kind {
                Kind::Bar => self.bar_chart(&data, colors, muted),
                Kind::Line => self.line_chart(data, colors[0]),
                Kind::Pie => {
                    let legend_data = data.clone();
                    let mut legend = Group::new().gap(Size::Sm);
                    for (i, (label, _)) in legend_data.iter().enumerate().take(8) {
                        legend = legend.child(
                            Group::new()
                                .gap(Size::Xs)
                                .align(Align::Center)
                                .child(div().w(px(10.0)).h(px(10.0)).rounded(px(2.0)).bg(colors[i % colors.len()]))
                                .child(Text::new(label.clone()).size(Size::Xs).dimmed()),
                        );
                    }
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(self.pie_chart(data, colors))
                        .child(legend)
                        .into_any_element()
                }
            }
        };

        Sheet::new()
            .title("Chart")
            .width(680.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(ChartEvent::Close)))
            .child(controls)
            .child(Divider::new())
            .child(div().p(px(8.0)).child(body))
    }
}
