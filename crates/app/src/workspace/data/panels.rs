//! Data-tab render helpers: the toolbar, the pending-changes review modal, and
//! the row inspector.

use gpui::prelude::*;
use gpui::{div, px, Context, Hsla};
use guise::prelude::*;

use super::DataPanel;
use crate::state::PendingChange;
use crate::workspace::review::generate_sql;

impl DataPanel {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>, border: Hsla) -> impl IntoElement {
        let has_selection = !self.state.selection.read(cx).is_empty();
        let pending = self.state.pending.read(cx).len();

        let mut actions = Group::new()
            .gap(Size::Xs)
            .align(Align::Center)
            .child(
                Button::new("data-refresh", "Refresh")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.state.bump_rows(cx))),
            )
            .child(
                Button::new("data-insert", "Insert")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.open_insert(cx))),
            )
            .child(
                Button::new("data-delete", "Delete")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .color(ColorName::Red)
                    .disabled(!has_selection)
                    .on_click(cx.listener(|this, _, _, cx| this.delete_selected(cx))),
            )
            .child(
                Button::new("data-mock", "Generate")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.generate_data(cx))),
            )
            .child(Divider::vertical())
            .child(
                Button::new("data-filter", "Filter")
                    .size(Size::Xs)
                    .variant(if self.state.filter_panel_open.read(cx).to_owned() {
                        Variant::Light
                    } else {
                        Variant::Subtle
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.filter_panel_open.update(cx, |open| *open = !*open);
                    })),
            )
            .child(
                Button::new("data-inspect", "Inspect")
                    .size(Size::Xs)
                    .variant(if self.state.inspector_open.read(cx).to_owned() {
                        Variant::Light
                    } else {
                        Variant::Subtle
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.inspector_open.update(cx, |open| *open = !*open);
                    })),
            )
            .child(
                Button::new("data-copy", "Copy")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .disabled(!has_selection)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_selection(cx))),
            )
            .child(Divider::vertical())
            .child(
                Button::new("data-import", "Import")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.import_csv(cx))),
            )
            .child(
                Button::new("data-export", "Export")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.export_table(cx))),
            );

        if pending > 0 {
            actions = actions
                .child(Divider::vertical())
                .child(
                    Badge::new(format!("{pending} pending"))
                        .variant(Variant::Light)
                        .color(ColorName::Orange)
                        .size(Size::Sm),
                )
                .child(
                    Button::new("data-review", "Review")
                        .size(Size::Xs)
                        .variant(Variant::Light)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_review = true;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("data-discard", "Discard")
                        .size(Size::Xs)
                        .variant(Variant::Subtle)
                        .color(ColorName::Red)
                        .on_click(cx.listener(|this, _, _, cx| this.discard(cx))),
                );
        }

        div()
            .flex()
            .items_center()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(border)
            .child(actions)
    }

    pub(super) fn review_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let changes = self.state.pending.get(cx);
        let committing = *self.committing.read(cx);
        let count = changes.len();
        let updates = changes.iter().filter(|c| matches!(c, PendingChange::Update { .. })).count();
        let inserts = changes.iter().filter(|c| matches!(c, PendingChange::Insert { .. })).count();
        let deletes = changes.iter().filter(|c| matches!(c, PendingChange::Delete { .. })).count();

        let mut list = Stack::new().gap(Size::Xs);
        for change in &changes {
            let accent = match change {
                PendingChange::Update { .. } => ColorName::Blue,
                PendingChange::Insert { .. } => ColorName::Teal,
                PendingChange::Delete { .. } => ColorName::Red,
            };
            let stripe = guise::theme::theme(cx).color(accent, 6).hsla();
            list = list.child(
                div()
                    .p(px(8.0))
                    .bg(colors.bg_surface)
                    .border_l_2()
                    .border_color(stripe)
                    .rounded(px(4.0))
                    .font_family(crate::theme::MONO_FAMILY)
                    .text_size(px(11.0))
                    .child(gpui::SharedString::from(generate_sql(change))),
            );
        }

        Modal::new()
            .title("Review Changes")
            .width(640.0)
            .on_close(cx.listener(|this, _, _, cx| {
                this.show_review = false;
                cx.notify();
            }))
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .child(Badge::new(format!("{updates} updates")).variant(Variant::Light).color(ColorName::Blue))
                    .child(Badge::new(format!("{inserts} inserts")).variant(Variant::Light).color(ColorName::Teal))
                    .child(Badge::new(format!("{deletes} deletes")).variant(Variant::Light).color(ColorName::Red)),
            )
            .child(
                div()
                    .id("review-scroll")
                    .max_h(px(380.0))
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(Divider::new())
            .child(
                Group::new()
                    .justify(Justify::Between)
                    .child(
                        Button::new("review-discard", "Discard All")
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .on_click(cx.listener(|this, _, _, cx| this.discard(cx))),
                    )
                    .child(
                        Group::new()
                            .gap(Size::Xs)
                            .child(
                                Button::new("review-cancel", "Cancel")
                                    .variant(Variant::Default)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_review = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(
                                    "review-commit",
                                    if committing {
                                        "Committing…".to_string()
                                    } else {
                                        format!("Commit {count}")
                                    },
                                )
                                .disabled(committing)
                                .on_click(cx.listener(|this, _, _, cx| this.commit(cx))),
                            ),
                    ),
            )
    }

    /// The right-side inspector: the last-selected row as column/value pairs.
    pub(super) fn inspector_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let null_display = self.app.settings.read(cx).null_display.clone();
        let selection = self.state.selection.get(cx);
        let response = self.state.rows.get(cx);

        let body = match (selection.iter().max().copied(), response) {
            (Some(idx), Some(resp)) if resp.rows.get(idx).is_some() => {
                let row = &resp.rows[idx];
                let mut list = Stack::new().gap(Size::Sm);
                for col in &resp.columns {
                    let value = crate::workspace::cell_text(row.get(col), &null_display);
                    let copy_value = value.clone();
                    list = list.child(
                        Stack::new()
                            .gap(Size::Xs)
                            .child(
                                Group::new()
                                    .justify(Justify::Between)
                                    .align(Align::Center)
                                    .child(Text::new(col.clone()).size(Size::Xs).dimmed())
                                    .child(
                                        ActionIcon::new(
                                            gpui::SharedString::from(format!("cp-{col}")),
                                            "⧉",
                                        )
                                        .variant(Variant::Subtle)
                                        .size(Size::Xs)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                copy_value.clone(),
                                            ));
                                            this.app.toasts.success(cx, "Copied", 1000);
                                        })),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(crate::theme::MONO_FAMILY)
                                    .text_size(px(12.0))
                                    .child(gpui::SharedString::from(value)),
                            ),
                    );
                }
                list.into_any_element()
            }
            _ => Text::new("Select a row to inspect").size(Size::Xs).dimmed().into_any_element(),
        };

        div()
            .id("inspector")
            .w(px(280.0))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .overflow_y_scroll()
            .p(px(8.0))
            .child(
                Stack::new()
                    .gap(Size::Sm)
                    .child(Text::new("Inspector").size(Size::Sm).medium())
                    .child(body),
            )
    }
}
