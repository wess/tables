//! One connection card. Rendered by `Home` so its action buttons can dispatch
//! back through `Home`'s listeners.

use gpui::prelude::*;
use gpui::{Context, SharedString};
use guise::prelude::*;

use crate::home::Home;
use crate::theme;
use model::StoredConnection;

impl Home {
    pub(super) fn card(&self, conn: &StoredConnection, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = theme::type_color(&conn.kind);
        let connecting = self.connecting.read(cx).as_deref() == Some(conn.id.as_str());

        let subtitle = if conn.kind == "sqlite" {
            conn.filepath.clone().unwrap_or_else(|| conn.database.clone())
        } else {
            format!("{}:{}", conn.host, conn.port)
        };

        let id_connect = conn.id.clone();
        let conn_edit = conn.clone();
        let conn_delete = conn.clone();

        let edit = ActionIcon::new(SharedString::from(format!("edit-{}", conn.id)), "✎")
            .variant(Variant::Subtle)
            .size(Size::Sm)
            .on_click(cx.listener(move |this, _, _, cx| this.open_form(Some(conn_edit.clone()), cx)));

        let delete = ActionIcon::new(SharedString::from(format!("delete-{}", conn.id)), "🗑")
            .variant(Variant::Subtle)
            .color(ColorName::Red)
            .size(Size::Sm)
            .on_click(cx.listener(move |this, _, _, cx| this.request_delete(conn_delete.clone(), cx)));

        let head = Group::new()
            .justify(Justify::Between)
            .align(Align::Start)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(ThemeIcon::new("▦").color(accent))
                    .child(
                        Stack::new()
                            .gap(Size::Xs)
                            .child(Text::new(conn.name.clone()).size(Size::Sm).medium())
                            .child(Text::new(subtitle).size(Size::Xs).dimmed()),
                    ),
            )
            .child(Group::new().gap(Size::Xs).child(edit).child(delete));

        let mut badges = Group::new().gap(Size::Xs).child(
            Badge::new(theme::type_label(&conn.kind))
                .variant(Variant::Light)
                .color(accent)
                .size(Size::Xs),
        );
        if conn.kind != "sqlite" && !conn.database.is_empty() {
            badges = badges.child(
                Badge::new(conn.database.clone())
                    .variant(Variant::Light)
                    .color(ColorName::Gray)
                    .size(Size::Xs),
            );
        }

        let connect = Button::new(
            SharedString::from(format!("connect-{}", conn.id)),
            if connecting { "Connecting…" } else { "Connect" },
        )
        .full_width(true)
        .disabled(connecting)
        .on_click(cx.listener(move |this, _, _, cx| this.connect(id_connect.clone(), cx)));

        Card::new()
            .with_border(true)
            .padding(Size::Md)
            .radius(Size::Md)
            .child(
                Stack::new()
                    .gap(Size::Sm)
                    .child(head)
                    .child(badges)
                    .child(connect),
            )
    }
}
