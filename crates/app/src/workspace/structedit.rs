//! The structure-edit modal: add column, rename column, create index, and
//! create table. Booleans are toggle buttons (not a `Select`), so the plain
//! `Modal` overlay is safe. It emits the assembled `EditOp` for the Structure
//! panel (or sidebar) to execute against the host.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, Window};
use guise::prelude::*;

use model::NewColumn;

/// A structure change to run against the active connection.
pub enum EditOp {
    AddColumn { table: String, column: NewColumn },
    RenameColumn { table: String, from: String, to: String },
    CreateIndex { table: String, name: String, columns: Vec<String>, unique: bool },
    CreateTable { name: String, columns: Vec<NewColumn> },
}

pub enum StructEditEvent {
    Submit(EditOp),
    Cancel,
}

/// What the modal is editing; carries the context each mode needs.
enum Mode {
    AddColumn,
    RenameColumn { current: String },
    CreateIndex { available: Vec<String> },
    CreateTable,
}

/// One editable column row (create-table / add-column).
struct ColRow {
    name: Entity<TextInput>,
    ty: Entity<TextInput>,
    nullable: bool,
    primary_key: bool,
}

pub struct StructEditModal {
    table: String,
    mode: Mode,
    // Create-table / add-column: one or more column rows.
    rows: Vec<ColRow>,
    // Rename: the new name. Create-index: the index name.
    text: Entity<TextInput>,
    // Create-index: which columns are selected, and the unique flag.
    picked: Vec<String>,
    unique: bool,
    error: Option<String>,
}

impl EventEmitter<StructEditEvent> for StructEditModal {}

fn column_row(cx: &mut Context<StructEditModal>) -> ColRow {
    ColRow {
        name: cx.new(|cx| TextInput::new(cx).placeholder("column").size(Size::Xs)),
        ty: cx.new(|cx| TextInput::new(cx).placeholder("type e.g. TEXT").size(Size::Xs)),
        nullable: true,
        primary_key: false,
    }
}

impl StructEditModal {
    pub fn add_column(table: String, cx: &mut Context<Self>) -> Self {
        Self::with_mode(table, Mode::AddColumn, vec![column_row(cx)], cx)
    }

    pub fn rename_column(table: String, current: String, cx: &mut Context<Self>) -> Self {
        let text = cx.new({
            let current = current.clone();
            move |cx| TextInput::new(cx).label("New name").value(&current).size(Size::Xs)
        });
        StructEditModal {
            table,
            mode: Mode::RenameColumn { current },
            rows: Vec::new(),
            text,
            picked: Vec::new(),
            unique: false,
            error: None,
        }
    }

    pub fn create_index(table: String, available: Vec<String>, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextInput::new(cx).label("Index name").size(Size::Xs));
        StructEditModal {
            table,
            mode: Mode::CreateIndex { available },
            rows: Vec::new(),
            text,
            picked: Vec::new(),
            unique: false,
            error: None,
        }
    }

    pub fn create_table(cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TextInput::new(cx).label("Table name").size(Size::Xs));
        StructEditModal {
            table: String::new(),
            mode: Mode::CreateTable,
            rows: vec![column_row(cx)],
            text: table,
            picked: Vec::new(),
            unique: false,
            error: None,
        }
    }

    fn with_mode(table: String, mode: Mode, rows: Vec<ColRow>, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextInput::new(cx).size(Size::Xs));
        StructEditModal { table, mode, rows, text, picked: Vec::new(), unique: false, error: None }
    }

    fn title(&self) -> String {
        match &self.mode {
            Mode::AddColumn => format!("Add column to {}", self.table),
            Mode::RenameColumn { .. } => format!("Rename column in {}", self.table),
            Mode::CreateIndex { .. } => format!("Create index on {}", self.table),
            Mode::CreateTable => "Create table".to_string(),
        }
    }

    fn read_rows(&self, cx: &Context<Self>) -> Vec<NewColumn> {
        self.rows
            .iter()
            .filter_map(|r| {
                let name = r.name.read(cx).text().trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let ty = r.ty.read(cx).text().trim().to_string();
                Some(NewColumn {
                    name,
                    data_type: if ty.is_empty() { "TEXT".into() } else { ty },
                    nullable: r.nullable,
                    primary_key: r.primary_key,
                    default_value: None,
                })
            })
            .collect()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let op = match &self.mode {
            Mode::AddColumn => {
                let cols = self.read_rows(cx);
                match cols.into_iter().next() {
                    Some(column) => EditOp::AddColumn { table: self.table.clone(), column },
                    None => return self.fail("A column name is required", cx),
                }
            }
            Mode::RenameColumn { current } => {
                let to = self.text.read(cx).text().trim().to_string();
                if to.is_empty() {
                    return self.fail("A new name is required", cx);
                }
                EditOp::RenameColumn {
                    table: self.table.clone(),
                    from: current.clone(),
                    to,
                }
            }
            Mode::CreateIndex { .. } => {
                let name = self.text.read(cx).text().trim().to_string();
                if name.is_empty() {
                    return self.fail("An index name is required", cx);
                }
                if self.picked.is_empty() {
                    return self.fail("Pick at least one column", cx);
                }
                EditOp::CreateIndex {
                    table: self.table.clone(),
                    name,
                    columns: self.picked.clone(),
                    unique: self.unique,
                }
            }
            Mode::CreateTable => {
                let name = self.text.read(cx).text().trim().to_string();
                if name.is_empty() {
                    return self.fail("A table name is required", cx);
                }
                let columns = self.read_rows(cx);
                if columns.is_empty() {
                    return self.fail("At least one column is required", cx);
                }
                EditOp::CreateTable { name, columns }
            }
        };
        cx.emit(StructEditEvent::Submit(op));
    }

    fn fail(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.error = Some(msg.to_string());
        cx.notify();
    }

    fn toggle_pick(&mut self, column: &str, cx: &mut Context<Self>) {
        if let Some(i) = self.picked.iter().position(|c| c == column) {
            self.picked.remove(i);
        } else {
            self.picked.push(column.to_string());
        }
        cx.notify();
    }

    /// A small on/off pill used for nullable / PK / unique flags.
    fn flag(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        Button::new(id, label)
            .size(Size::Xs)
            .variant(if on { Variant::Light } else { Variant::Subtle })
            .on_click(cx.listener(move |this, _, _, cx| toggle(this, cx)))
    }

    fn column_rows_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut stack = Stack::new().gap(Size::Xs);
        for (i, row) in self.rows.iter().enumerate() {
            let null_on = row.nullable;
            let pk_on = row.primary_key;
            stack = stack.child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(div().flex_1().child(row.name.clone()))
                    .child(div().flex_1().child(row.ty.clone()))
                    .child(self.flag("null", "NULL", null_on, cx, move |this, cx| {
                        if let Some(r) = this.rows.get_mut(i) {
                            r.nullable = !r.nullable;
                        }
                        cx.notify();
                    }))
                    .child(self.flag("pk", "PK", pk_on, cx, move |this, cx| {
                        if let Some(r) = this.rows.get_mut(i) {
                            r.primary_key = !r.primary_key;
                        }
                        cx.notify();
                    }))
                    .child(
                        ActionIcon::new(gpui::SharedString::from(format!("rmrow-{i}")), "✕")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.rows.len() > 1 {
                                    this.rows.remove(i);
                                    cx.notify();
                                }
                            })),
                    ),
            );
        }
        stack.child(
            Button::new("addrow", "＋ Add column")
                .size(Size::Xs)
                .variant(Variant::Subtle)
                .on_click(cx.listener(|this, _, _, cx| {
                    let row = column_row(cx);
                    this.rows.push(row);
                    cx.notify();
                })),
        )
    }

    fn body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match &self.mode {
            Mode::AddColumn => self.column_rows_editor(cx).into_any_element(),
            Mode::RenameColumn { .. } => {
                Stack::new().gap(Size::Xs).child(self.text.clone()).into_any_element()
            }
            Mode::CreateIndex { available } => {
                let mut chips = Group::new().gap(Size::Xs);
                for col in available {
                    let on = self.picked.iter().any(|c| c == col);
                    let name = col.clone();
                    chips = chips.child(
                        Button::new(gpui::SharedString::from(format!("pick-{col}")), col.clone())
                            .size(Size::Xs)
                            .variant(if on { Variant::Light } else { Variant::Subtle })
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle_pick(&name, cx))),
                    );
                }
                let unique_on = self.unique;
                Stack::new()
                    .gap(Size::Sm)
                    .child(self.text.clone())
                    .child(Text::new("Columns").size(Size::Xs).dimmed())
                    .child(chips)
                    .child(self.flag("uniq", "UNIQUE", unique_on, cx, |this, cx| {
                        this.unique = !this.unique;
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Mode::CreateTable => Stack::new()
                .gap(Size::Sm)
                .child(self.text.clone())
                .child(Text::new("Columns").size(Size::Xs).dimmed())
                .child(self.column_rows_editor(cx))
                .into_any_element(),
        }
    }
}

impl Render for StructEditModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = Stack::new().gap(Size::Sm).child(self.body(cx));
        if let Some(error) = &self.error {
            content = content.child(Alert::new(error.clone()).color(ColorName::Red).icon("✕"));
        }

        Modal::new()
            .title(self.title())
            .width(520.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(StructEditEvent::Cancel)))
            .child(
                div().id("structedit-body").max_h(px(440.0)).overflow_y_scroll().child(content),
            )
            .child(Divider::new())
            .child(
                Group::new()
                    .justify(Justify::End)
                    .gap(Size::Xs)
                    .child(
                        Button::new("se-cancel", "Cancel")
                            .variant(Variant::Subtle)
                            .color(ColorName::Gray)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(StructEditEvent::Cancel))),
                    )
                    .child(
                        Button::new("se-submit", "Apply")
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
