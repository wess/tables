//! One connection card. Rendered by `Home` so its action buttons can dispatch
//! back through `Home`'s listeners.

use gpui::prelude::*;
use gpui::{div, px, Context, SharedString};
use guise::prelude::*;

use crate::home::Home;
use crate::theme;
use model::StoredConnection;

impl Home {
    pub(super) fn card(&self, conn: &StoredConnection, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = theme::type_color(&conn.kind);
        let connecting = self.connecting.read(cx).as_deref() == Some(conn.id.as_str());

        let subtitle = if conn.kind == "sqlite" {
            conn.filepath
                .clone()
                .unwrap_or_else(|| conn.database.clone())
        } else {
            format!("{}:{}", conn.host, conn.port)
        };

        let id_connect = conn.id.clone();
        let conn_edit = conn.clone();
        let conn_delete = conn.clone();

        let edit = ActionIcon::new(
            SharedString::from(format!("edit-{}", conn.id)),
            IconName::Pencil,
        )
        .label("Edit connection")
        .variant(Variant::Subtle)
        .size(Size::Sm)
        .on_click(cx.listener(move |this, _, _, cx| this.open_form(Some(conn_edit.clone()), cx)));

        let delete = ActionIcon::new(
            SharedString::from(format!("delete-{}", conn.id)),
            IconName::Trash2,
        )
        .label("Delete connection")
        .variant(Variant::Subtle)
        .color(ColorName::Red)
        .size(Size::Sm)
        .on_click(cx.listener(move |this, _, _, cx| this.request_delete(conn_delete.clone(), cx)));

        let colors = theme::palette(cx);
        div()
            .flex()
            .items_center()
            .w_full()
            .min_w(px(0.0))
            .gap(px(14.0))
            .px(px(16.0))
            .py(px(16.0))
            .border_b_1()
            .border_color(colors.border)
            .child(Icon::new(IconName::Database).size(Size::Lg).color(accent))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(4.0))
                    .child(
                        div()
                            .truncate()
                            .child(Text::new(conn.name.clone()).size(Size::Sm).medium()),
                    )
                    .child(
                        div().truncate().child(
                            Text::new(format!("{} · {subtitle}", theme::type_label(&conn.kind)))
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                    ),
            )
            .child(edit)
            .child(delete)
            .child(
                Button::new(
                    SharedString::from(format!("connect-{}", conn.id)),
                    if connecting {
                        "Connecting…"
                    } else {
                        "Connect"
                    },
                )
                .size(Size::Sm)
                .color(ColorName::Gray)
                .variant(Variant::Default)
                .disabled(connecting)
                .on_click(cx.listener(move |this, _, _, cx| this.connect(id_connect.clone(), cx))),
            )
    }
}
