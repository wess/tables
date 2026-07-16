//! `Sheet` — a non-deferred modal overlay, a drop-in for guise's `Modal`.
//!
//! guise's `Modal` is a gpui `deferred` element (it draws its scrim on top of
//! everything). A guise `Select` (and other dropdowns) also defer their menu.
//! gpui forbids calling `defer_draw` while already inside a deferred draw, so a
//! `Select` opened inside a `Modal` aborts with "cannot call defer_draw during
//! deferred drawing". `Sheet` paints inline instead — an absolutely-positioned
//! scrim sized to the viewport, drawn last so it covers the surface — so any
//! dropdown inside it defers its menu at the top level and never nests.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, ClickEvent, FontWeight, IntoElement, SharedString, Window};
use guise::prelude::*;
use guise::theme::theme;

type CloseFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Sheet {
    title: Option<SharedString>,
    width: f32,
    on_close: Option<CloseFn>,
    children: Vec<AnyElement>,
}

impl Sheet {
    pub fn new() -> Self {
        Sheet { title: None, width: 520.0, on_close: None, children: Vec::new() }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Called when the scrim or the × is clicked. Pass a `cx.listener(...)`.
    pub fn on_close(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet::new()
    }
}

impl ParentElement for Sheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Sheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let t = theme(cx);
        let text = t.text().hsla();
        let dimmed = t.dimmed().hsla();
        let scrim = t.black.alpha(0.55);
        let radius = t.radius(Size::Md);
        let viewport = window.viewport_size();
        let close = self.on_close;

        let mut header = div().flex().items_center().justify_between();
        header = header.child(match self.title {
            Some(title) => div()
                .text_size(px(17.0))
                .font_weight(FontWeight::BOLD)
                .text_color(text)
                .child(title),
            None => div(),
        });
        if let Some(handler) = close.clone() {
            header = header.child(
                div()
                    .id("sheet-close")
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .text_size(px(18.0))
                    .text_color(dimmed)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(text))
                    .child(SharedString::new_static("\u{00d7}"))
                    .on_click(move |ev, win, cx| {
                        handler(ev, win, cx);
                        cx.stop_propagation();
                    }),
            );
        }

        let mut body = div()
            .id("sheet-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p(px(16.0));
        for child in self.children {
            body = body.child(child);
        }

        let dialog = div()
            .id("sheet-dialog")
            .occlude()
            .flex()
            .flex_col()
            .w(px(self.width))
            .max_h(px(720.0))
            .bg(colors.bg_surface)
            .border_1()
            .border_color(colors.border)
            .rounded(px(radius))
            .shadow_xl()
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(colors.border)
                    .child(header),
            )
            .child(body);

        let mut scrim_el = div()
            .id("sheet-scrim")
            .occlude()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .items_center()
            .justify_center()
            .bg(scrim);
        if let Some(handler) = close {
            scrim_el = scrim_el.on_click(move |ev, win, cx| handler(ev, win, cx));
        }
        scrim_el.child(dialog)
    }
}
