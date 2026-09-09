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
        let loading = self.busy.get(cx) || self.state.rows_loading.get(cx);
        let locked = loading || self.committing.get(cx);
        let selected = !self.state.selection.read(cx).is_empty();
        let has_table = self.state.active_table.read(cx).is_some();
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(40.0))
            .px(px(8.0))
            .gap(px(4.0))
            .bg(crate::theme::palette(cx).bg_subtle)
            .border_b_1()
            .border_color(border)
            .child(
                ActionIcon::new("data-refresh", IconName::RefreshCw)
                    .size(Size::Sm)
                    .label("Refresh rows")
                    .disabled(loading)
                    .on_click(cx.listener(|this, _, _, cx| this.state.bump_rows(cx))),
            )
            .child(
                ActionIcon::new("data-insert", IconName::Plus)
                    .size(Size::Sm)
                    .label("Insert row")
                    .disabled(locked || !has_table)
                    .on_click(cx.listener(|this, _, _, cx| this.open_insert(cx))),
            )
            .child(div().w(px(1.0)).h(px(18.0)).mx(px(4.0)).bg(border))
            .child(
                ActionIcon::new("data-filter", IconName::ListFilter)
                    .size(Size::Sm)
                    .label("Filters")
                    .variant(if self.state.filter_panel_open.get(cx) {
                        Variant::Light
                    } else {
                        Variant::Subtle
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.filter_panel_open.update(cx, |o| *o = !*o)
                    })),
            )
            .child(
                ActionIcon::new("data-inspect", IconName::PanelRight)
                    .size(Size::Sm)
                    .label("Inspect selected row")
                    .variant(if self.state.inspector_open.get(cx) {
                        Variant::Light
                    } else {
                        Variant::Subtle
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.inspector_open.update(cx, |o| *o = !*o)
                    })),
            )
            .child(
                ActionIcon::new("data-copy", IconName::Copy)
                    .size(Size::Sm)
                    .label("Copy selected rows…")
                    .disabled(locked || !selected)
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                        this.open_copy(event.position(), window, cx);
                    })),
            )
            .child(
                ActionIcon::new("data-delete", IconName::Trash2)
                    .size(Size::Sm)
                    .label("Stage deletion of selected rows")
                    .disabled(locked || !selected)
                    .on_click(cx.listener(|this, _, _, cx| this.delete_selected(cx))),
            )
            .child(div().w(px(1.0)).h(px(18.0)).mx(px(4.0)).bg(border))
            .child(
                ActionIcon::new("data-import", IconName::Upload)
                    .size(Size::Sm)
                    .label("Import CSV…")
                    .disabled(locked || !has_table)
                    .on_click(cx.listener(|this, _, _, cx| this.import_csv(cx))),
            )
            .child(
                ActionIcon::new("data-export", IconName::Download)
                    .size(Size::Sm)
                    .label("Export table…")
                    .disabled(locked || !has_table)
                    .on_click(cx.listener(|this, _, _, cx| this.export_table(cx))),
            )
            .child(
                ActionIcon::new("data-generate", IconName::WandSparkles)
                    .size(Size::Sm)
                    .label("Generate 50 sample rows")
                    .disabled(locked || !has_table)
                    .on_click(cx.listener(|this, _, _, cx| this.generate_data(cx))),
            )
            .child(div().flex_1())
            .when(loading, |d| d.child(Loader::new().size(Size::Xs)))
            .child(super::super::views::selector(&self.state, cx))
    }

    pub(super) fn pending_bar(&self, cx: &mut Context<Self>, count: usize) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(42.0))
            .px(px(12.0))
            .gap(px(12.0))
            .bg(colors.bg_surface)
            .border_t_1()
            .border_color(colors.border)
            .child(Icon::new(IconName::Pencil).size(Size::Xs))
            .child(Text::new(format!("{count} uncommitted change(s)")).size(Size::Xs))
            .child(div().flex_1())
            .child(
                Button::new("data-discard", "Discard")
                    .size(Size::Xs)
                    .color(ColorName::Gray)
                    .variant(Variant::Subtle)
                    .disabled(self.committing.get(cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_discard = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("data-review", "Review changes")
                    .size(Size::Xs)
                    .disabled(self.committing.get(cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_review = true;
                        cx.notify();
                    })),
            )
    }

    pub(super) fn review_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let changes = self.state.pending.read(cx);
        let committing = *self.committing.read(cx);
        let count = changes.len();
        let updates = changes
            .iter()
            .filter(|c| matches!(c, PendingChange::Update { .. }))
            .count();
        let inserts = changes
            .iter()
            .filter(|c| matches!(c, PendingChange::Insert { .. }))
            .count();
        let deletes = changes
            .iter()
            .filter(|c| matches!(c, PendingChange::Delete { .. }))
            .count();

        let mut list = Stack::new().gap(Size::Xs);
        for change in changes {
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
                    .child(
                        Badge::new(format!("{updates} updates"))
                            .variant(Variant::Light)
                            .color(ColorName::Blue),
                    )
                    .child(
                        Badge::new(format!("{inserts} inserts"))
                            .variant(Variant::Light)
                            .color(ColorName::Teal),
                    )
                    .child(
                        Badge::new(format!("{deletes} deletes"))
                            .variant(Variant::Light)
                            .color(ColorName::Red),
                    ),
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
        // Borrow rather than clone the whole page/selection to show one row.
        let selection = self.state.selection.read(cx);
        let rows_binding = self.state.rows.read(cx);
        let response = rows_binding.as_ref();

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
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                cx.write_to_clipboard(
                                                    gpui::ClipboardItem::new_string(
                                                        copy_value.clone(),
                                                    ),
                                                );
                                                this.app.toasts.success(cx, "Copied", 1000);
                                            }),
                                        ),
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
            _ => Text::new("Select a row to inspect")
                .size(Size::Xs)
                .dimmed()
                .into_any_element(),
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
