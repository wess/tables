//! The schema-comparison modal. Picks another saved connection as the target,
//! connects it, and diffs its tables against the current connection's — the
//! generated SQL migrates the target toward the source.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, Window};
use guise::prelude::*;

use crate::bridge;
use crate::sheet::Sheet;
use crate::state::AppState;
use model::{SchemaDiff, StoredConnection};

pub enum SchemaCompareEvent {
    Close,
}

pub struct SchemaCompareModal {
    app: AppState,
    source_id: String,
    targets: Vec<StoredConnection>,
    target: Entity<Select>,
    diffs: Signal<Option<Vec<SchemaDiff>>>,
    comparing: Signal<bool>,
}

impl EventEmitter<SchemaCompareEvent> for SchemaCompareModal {}

impl SchemaCompareModal {
    pub fn new(source_id: String, cx: &mut Context<Self>) -> Self {
        let app = AppState::get(cx);
        let targets: Vec<StoredConnection> = app
            .host
            .list_connections()
            .into_iter()
            .filter(|c| c.id != source_id)
            .collect();
        let labels: Vec<String> = targets
            .iter()
            .map(|c| if c.name.is_empty() { c.id.clone() } else { c.name.clone() })
            .collect();
        let target = cx.new(move |cx| Select::new(cx).placeholder("Target connection").data(labels));
        let diffs = Signal::new(cx, None);
        let comparing = Signal::new(cx, false);
        watch(cx, &diffs);
        watch(cx, &comparing);
        SchemaCompareModal { app, source_id, targets, target, diffs, comparing }
    }

    fn compare(&self, cx: &mut gpui::App) {
        let Some(idx) = self.target.read(cx).selected_index() else {
            return;
        };
        let Some(target) = self.targets.get(idx) else {
            return;
        };
        let target_id = target.id.clone();
        let source_id = self.source_id.clone();
        let host = self.app.host.clone();
        let diffs = self.diffs.clone();
        let comparing = self.comparing.clone();
        let toasts = self.app.toasts.clone();
        self.comparing.set(cx, true);
        bridge::run(
            cx,
            async move {
                host.connect(&target_id).await?;
                host.compare_schemas(&source_id, &target_id).await
            },
            move |result: Result<Vec<SchemaDiff>, String>, cx| {
                comparing.set(cx, false);
                match result {
                    Ok(list) => diffs.set(cx, Some(list)),
                    Err(error) => toasts.error(cx, "Compare failed", &error),
                }
            },
        );
    }
}

impl Render for SchemaCompareModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let comparing = *self.comparing.read(cx);

        let controls = Group::new()
            .gap(Size::Xs)
            .align(Align::End)
            .child(div().flex_1().child(self.target.clone()))
            .child(
                Button::new("compare-run", if comparing { "Comparing…" } else { "Compare" })
                    .disabled(comparing)
                    .on_click(cx.listener(|this, _, _, cx| this.compare(cx))),
            );

        let results = match self.diffs.get(cx) {
            None => Text::new("Pick a target connection and compare.").size(Size::Sm).dimmed().into_any_element(),
            Some(diffs) if diffs.is_empty() => {
                Text::new("Schemas are identical.").size(Size::Sm).dimmed().into_any_element()
            }
            Some(diffs) => {
                let mut list = Stack::new().gap(Size::Sm);
                for diff in &diffs {
                    let accent = match diff.kind.as_str() {
                        "added" => ColorName::Teal,
                        "removed" => ColorName::Red,
                        _ => ColorName::Blue,
                    };
                    let stripe = guise::theme::theme(cx).color(accent, 6).hsla();
                    list = list.child(
                        div()
                            .p(px(8.0))
                            .bg(colors.bg_surface)
                            .border_l_2()
                            .border_color(stripe)
                            .rounded(px(4.0))
                            .child(
                                Stack::new()
                                    .gap(Size::Xs)
                                    .child(
                                        Group::new()
                                            .gap(Size::Xs)
                                            .align(Align::Center)
                                            .child(Badge::new(diff.kind.clone()).variant(Variant::Light).color(accent).size(Size::Xs))
                                            .child(Text::new(diff.table.clone()).size(Size::Sm).medium()),
                                    )
                                    .child(Text::new(diff.details.clone()).size(Size::Xs).dimmed())
                                    .child(
                                        div()
                                            .font_family(crate::theme::MONO_FAMILY)
                                            .text_size(px(11.0))
                                            .child(gpui::SharedString::from(diff.sql.clone())),
                                    ),
                            ),
                    );
                }
                list.into_any_element()
            }
        };

        Sheet::new()
            .title("Schema Comparison")
            .width(680.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(SchemaCompareEvent::Close)))
            .child(controls)
            .child(Divider::new())
            .child(
                div()
                    .id("compare-scroll")
                    .max_h(px(420.0))
                    .overflow_y_scroll()
                    .child(results),
            )
    }
}
