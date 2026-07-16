//! The notification (toast) facade — top-right stack, per-toast color and
//! auto-close duration.

use std::time::Duration;

use gpui::{App, AppContext, Entity};
use guise::prelude::*;

/// Provided as context by `Root`; any view can toast.
#[derive(Clone)]
pub struct Toasts {
    stack: Entity<ToastStack>,
}

impl Toasts {
    pub fn new(cx: &mut App) -> Self {
        Toasts {
            stack: cx.new(|_| ToastStack::new()),
        }
    }

    pub fn stack(&self) -> Entity<ToastStack> {
        self.stack.clone()
    }

    /// The general form: optional title, message, color, auto-close ms.
    pub fn show(&self, cx: &mut App, title: Option<&str>, message: &str, color: ColorName, ms: u64) {
        let title = title.map(str::to_string);
        let message = message.to_string();
        self.stack.update(cx, |stack, cx| {
            stack.set_duration(Some(Duration::from_millis(ms)));
            match title {
                Some(title) => stack.push_titled(title, message, color, cx),
                None => stack.push(message, cx),
            };
        });
    }

    /// Success (teal), message only.
    pub fn success(&self, cx: &mut App, message: &str, ms: u64) {
        self.show(cx, None, message, ColorName::Teal, ms);
    }

    /// Error (red) with a title; default 4000 ms.
    pub fn error(&self, cx: &mut App, title: &str, message: &str) {
        self.show(cx, Some(title), message, ColorName::Red, 4000);
    }

    /// Partial result (orange) with a title.
    pub fn warn(&self, cx: &mut App, title: &str, message: &str) {
        self.show(cx, Some(title), message, ColorName::Orange, 4000);
    }

    /// Neutral info (default color), message only.
    pub fn info(&self, cx: &mut App, message: &str, ms: u64) {
        self.show(cx, None, message, ColorName::Gray, ms);
    }
}
