//! Query-tab render helpers: the history/favorites side panel and the
//! per-statement result block, plus small text formatters.

use gpui::prelude::*;
use gpui::{div, px, Context};
use guise::prelude::*;

use super::{QueryPanel, Side};
use crate::workspace::cell_text;
use model::QueryResult;

fn short_time(iso: &str) -> String {
    // "2026-07-15T19:34:07.123Z" -> "2026-07-15 19:34:07"
    iso.get(..19).unwrap_or(iso).replace('T', " ")
}

fn preview(sql: &str) -> String {
    let one_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.len() > 120 {
        format!("{}…", &one_line[..120])
    } else {
        one_line
    }
}

impl QueryPanel {
    pub(super) fn side_panel(&self, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let error_color = guise::theme::theme(cx).color(ColorName::Red, 6).hsla();

        let body = match side {
            Side::History => {
                let entries = self.history.get(cx);
                if entries.is_empty() {
                    Stack::new()
                        .child(Text::new("No history yet").size(Size::Xs).dimmed())
                        .into_any_element()
                } else {
                    let mut list = Stack::new().gap(Size::Xs);
                    for entry in &entries {
                        let sql = entry.sql.clone();
                        let is_error = entry.error.is_some();
                        let meta = if is_error {
                            "error".to_string()
                        } else {
                            format!("{} ms", entry.execution_time)
                        };
                        list = list.child(
                            div()
                                .id(gpui::SharedString::from(format!("h-{}", entry.id)))
                                .p(px(6.0))
                                .rounded(px(4.0))
                                .bg(colors.bg_muted)
                                .cursor_pointer()
                                .child(
                                    Group::new()
                                        .justify(Justify::Between)
                                        .child(Text::new(short_time(&entry.executed_at)).size(Size::Xs).dimmed())
                                        .child(Text::new(meta).size(Size::Xs).dimmed()),
                                )
                                .child(
                                    div()
                                        .font_family(crate::theme::MONO_FAMILY)
                                        .text_size(px(11.0))
                                        .when(is_error, |d| d.text_color(error_color))
                                        .child(gpui::SharedString::from(preview(&entry.sql))),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| this.load_sql(&sql, cx))),
                        );
                    }
                    list.into_any_element()
                }
            }
            Side::Favorites => {
                let favorites = self.favorites.get(cx);
                let mut container = Stack::new().gap(Size::Sm).child(
                    Group::new()
                        .gap(Size::Xs)
                        .child(self.fav_name.clone())
                        .child(
                            Button::new("fav-save", "Save")
                                .size(Size::Xs)
                                .on_click(cx.listener(|this, _, _, cx| this.save_current_favorite(cx))),
                        ),
                );
                if favorites.is_empty() {
                    container = container.child(Text::new("No favorites").size(Size::Xs).dimmed());
                } else {
                    let mut list = Stack::new().gap(Size::Xs);
                    for fav in &favorites {
                        let sql = fav.sql.clone();
                        let id = fav.id.clone();
                        list = list.child(
                            div()
                                .id(gpui::SharedString::from(format!("f-{}", fav.id)))
                                .p(px(6.0))
                                .rounded(px(4.0))
                                .bg(colors.bg_muted)
                                .child(
                                    Group::new()
                                        .justify(Justify::Between)
                                        .align(Align::Center)
                                        .child(
                                            div()
                                                .id(gpui::SharedString::from(format!("favname-{}", fav.id)))
                                                .flex_1()
                                                .cursor_pointer()
                                                .child(Text::new(fav.name.clone()).size(Size::Xs).medium())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.load_sql(&sql, cx)
                                                })),
                                        )
                                        .child(
                                            ActionIcon::new(
                                                gpui::SharedString::from(format!("del-{}", fav.id)),
                                                "🗑",
                                            )
                                            .variant(Variant::Subtle)
                                            .color(ColorName::Red)
                                            .size(Size::Xs)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.delete_favorite(id.clone(), cx)
                                            })),
                                        ),
                                )
                                .child(
                                    div()
                                        .font_family(crate::theme::MONO_FAMILY)
                                        .text_size(px(11.0))
                                        .child(gpui::SharedString::from(preview(&fav.sql))),
                                ),
                        );
                    }
                    container = container.child(list);
                }
                container.into_any_element()
            }
        };

        div()
            .id("query-side")
            .w(px(260.0))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .overflow_y_scroll()
            .p(px(8.0))
            .child(body)
    }

    pub(super) fn result_block(
        &self,
        idx: usize,
        result: &QueryResult,
        multi: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let error_color = guise::theme::theme(cx).color(ColorName::Red, 6);
        let mut block = Stack::new().gap(Size::Xs);
        if multi {
            let meta = if result.error.is_some() {
                "error".to_string()
            } else {
                format!("{} rows · {} ms", result.rows.len(), result.execution_time)
            };
            block = block.child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(Text::new(format!("Statement {}", idx + 1)).size(Size::Xs).medium())
                    .child(Text::new(meta).size(Size::Xs).dimmed()),
            );
            if let Some(sql) = &result.sql {
                block = block.child(
                    div()
                        .font_family(crate::theme::MONO_FAMILY)
                        .text_size(px(11.0))
                        .child(gpui::SharedString::from(preview(sql))),
                );
            }
        }
        let inner = if let Some(err) = &result.error {
            Text::new(err.clone()).size(Size::Sm).color(error_color).into_any_element()
        } else if !result.columns.is_empty() {
            let mut table = Table::new()
                .with_border(true)
                .striped(true)
                .highlight_on_hover(true)
                .head(result.columns.clone());
            for row in &result.rows {
                let cells: Vec<String> = result
                    .columns
                    .iter()
                    .map(|c| cell_text(row.get(c), "NULL"))
                    .collect();
                table = table.row(cells);
            }
            table.into_any_element()
        } else {
            Text::new(format!(
                "{} row(s) affected · {} ms",
                result.rows_affected, result.execution_time
            ))
            .size(Size::Sm)
            .dimmed()
            .into_any_element()
        };
        block.child(inner).into_any_element()
    }
}
