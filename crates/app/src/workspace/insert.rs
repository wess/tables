//! The insert-row modal. One field per column; an empty field is omitted so the
//! column takes its database default. Emits the assembled row for the Data
//! panel to persist. Stays open across a submit — the parent drives the async
//! insert and calls back in, so a failure keeps the typed values on screen and
//! a success clears the form for the next row.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, FocusHandle, Window};
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
    busy: bool,
    error: Option<String>,
    /// Cleared to re-focus the first field on open and after each saved row,
    /// then set once focus lands — so it never steals focus mid-tabbing.
    focused: bool,
}

impl EventEmitter<InsertEvent> for InsertModal {}

impl InsertModal {
    pub fn new(table: String, columns: Vec<String>, cx: &mut Context<Self>) -> Self {
        let inputs: Vec<Entity<TextInput>> = columns
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
        // Enter in any field submits the row, matching the primary button.
        for input in &inputs {
            cx.subscribe(input, |this, _input, event: &TextInputEvent, cx| {
                if let TextInputEvent::Submit(_) = event {
                    this.submit(cx);
                }
            })
            .detach();
        }
        InsertModal { table, columns, inputs, busy: false, error: None, focused: false }
    }

    fn first_focus(&self, cx: &Context<Self>) -> Option<FocusHandle> {
        self.inputs.first().map(|i| i.read(cx).focus_handle())
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

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let row = self.build_row(cx);
        self.busy = true;
        self.error = None;
        cx.emit(InsertEvent::Submit(row));
        cx.notify();
    }

    /// The parent's insert failed — surface the error and let the user retry
    /// without losing what they typed.
    pub fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = Some(error);
        cx.notify();
    }

    /// The parent's insert succeeded — clear the fields and refocus so the user
    /// can enter another row.
    pub fn succeed(&mut self, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = None;
        self.focused = false; // re-focus the first field on the next render
        for input in &self.inputs {
            input.update(cx, |i, cx| i.set_text("", cx));
        }
        cx.notify();
    }
}

impl Render for InsertModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus the first field once on open (and once after each saved row),
        // never on later renders — so tabbing between fields isn't hijacked.
        if !self.focused {
            if let Some(handle) = self.first_focus(cx) {
                handle.focus(window);
            }
            self.focused = true;
        }

        let mut fields = Stack::new().gap(Size::Xs);
        for input in &self.inputs {
            fields = fields.child(input.clone());
        }
        if let Some(error) = &self.error {
            fields = fields.child(Alert::new(error.clone()).color(ColorName::Red).icon("✕"));
        }

        let busy = self.busy;
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
                        Button::new("insert-submit", if busy { "Inserting…" } else { "Insert" })
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
