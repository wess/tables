//! The insert-row modal. One field per column; an empty field is omitted so the
//! column takes its database default. Emits the assembled row for the Data
//! panel to persist.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, Window};
use guise::prelude::*;
use serde_json::Value;

use model::Row;

pub enum InsertEvent {
    Submit(Row),
    Cancel,
}

pub struct InsertModal {
    table: String,
    columns: Vec<String>,
    inputs: Vec<Entity<TextInput>>,
}

impl EventEmitter<InsertEvent> for InsertModal {}

impl InsertModal {
    pub fn new(table: String, columns: Vec<String>, cx: &mut Context<Self>) -> Self {
        let inputs = columns
            .iter()
            .map(|column| {
                let column = column.clone();
                cx.new(move |cx| {
                    TextInput::new(cx)
                        .label(column)
                        .placeholder("(default)")
                        .size(Size::Xs)
                })
            })
            .collect();
        InsertModal { table, columns, inputs }
    }

    fn build_row(&self, cx: &Context<Self>) -> Row {
        let mut row = Row::new();
        for (column, input) in self.columns.iter().zip(&self.inputs) {
            let text = input.read(cx).text();
            if !text.is_empty() {
                row.insert(column.clone(), Value::String(text));
            }
        }
        row
    }
}

impl Render for InsertModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = Stack::new().gap(Size::Xs);
        for input in &self.inputs {
            fields = fields.child(input.clone());
        }

        Modal::new()
            .title(format!("Insert into {}", self.table))
            .width(460.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(InsertEvent::Cancel)))
            .child(
                div()
                    .id("insert-fields")
                    .max_h(px(420.0))
                    .overflow_y_scroll()
                    .child(fields),
            )
            .child(Divider::new())
            .child(
                Group::new()
                    .justify(Justify::End)
                    .gap(Size::Xs)
                    .child(
                        Button::new("insert-cancel", "Cancel")
                            .variant(Variant::Subtle)
                            .color(ColorName::Gray)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(InsertEvent::Cancel))),
                    )
                    .child(
                        Button::new("insert-submit", "Insert").on_click(cx.listener(
                            |this, _, _, cx| {
                                let row = this.build_row(cx);
                                cx.emit(InsertEvent::Submit(row));
                            },
                        )),
                    ),
            )
    }
}
