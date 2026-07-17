//! The Extensions modal: manage installed plugins (enable/disable, install from
//! the registry, uninstall) and saved macros (export, import, delete). The host
//! plugin/macro operations are synchronous local-file work, so they run inline.

use gpui::prelude::*;
use gpui::{div, px, Context, EventEmitter, PathPromptOptions, SharedString, Window};
use guise::prelude::*;

use crate::state::AppState;
use model::{InstalledPlugin, Macro, PluginManifest};

pub enum ExtensionsEvent {
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Plugins,
    Macros,
}

pub struct ExtensionsModal {
    app: AppState,
    tab: Tab,
    plugins: Vec<InstalledPlugin>,
    registry: Vec<PluginManifest>,
    macros: Vec<Macro>,
}

impl EventEmitter<ExtensionsEvent> for ExtensionsModal {}

impl ExtensionsModal {
    pub fn new(app: AppState, cx: &mut Context<Self>) -> Self {
        let mut modal =
            ExtensionsModal { app, tab: Tab::Plugins, plugins: Vec::new(), registry: Vec::new(), macros: Vec::new() };
        modal.reload();
        let _ = cx;
        modal
    }

    fn reload(&mut self) {
        self.plugins = self.app.host.list_plugins();
        self.registry = self.app.host.plugin_registry();
        self.macros = self.app.host.list_macros();
    }

    fn toggle_plugin(&mut self, name: String, enabled: bool, cx: &mut Context<Self>) {
        self.app.host.toggle_plugin(&name, enabled);
        self.reload();
        cx.notify();
    }

    fn uninstall_plugin(&mut self, name: String, cx: &mut Context<Self>) {
        self.app.host.uninstall_plugin(&name);
        self.app.toasts.success(cx, "Plugin removed", 1500);
        self.reload();
        cx.notify();
    }

    fn install_plugin(&mut self, manifest: PluginManifest, cx: &mut Context<Self>) {
        match self.app.host.install_plugin(&manifest) {
            Ok(_) => self.app.toasts.success(cx, "Plugin installed", 1500),
            Err(e) => self.app.toasts.error(cx, "Install failed", &e),
        }
        self.reload();
        cx.notify();
    }

    fn export_macro(&mut self, id: String, cx: &mut Context<Self>) {
        match self.app.host.export_macro(&id) {
            Ok(json) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
                self.app.toasts.success(cx, "Macro copied as JSON", 1500);
            }
            Err(e) => self.app.toasts.error(cx, "Export failed", &e),
        }
    }

    fn delete_macro(&mut self, id: String, cx: &mut Context<Self>) {
        self.app.host.delete_macro(&id);
        self.app.toasts.success(cx, "Macro deleted", 1500);
        self.reload();
        cx.notify();
    }

    fn import_macro(&self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = cx.update(|cx| {
                let outcome = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
                    .and_then(|v| host.import_macro(&v));
                match outcome {
                    Ok(_) => {
                        toasts.success(cx, "Macro imported", 1500);
                        let _ = this.update(cx, |m, cx| {
                            m.reload();
                            cx.notify();
                        });
                    }
                    Err(e) => toasts.error(cx, "Import failed", &e),
                }
            });
        })
        .detach();
    }

    fn plugins_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::palette(cx);
        let mut stack = Stack::new().gap(Size::Sm);

        stack = stack.child(Text::new("Installed").size(Size::Xs).dimmed());
        if self.plugins.is_empty() {
            stack = stack.child(Text::new("No plugins installed").size(Size::Xs).dimmed());
        } else {
            for p in &self.plugins {
                let name = p.manifest.name.clone();
                let name_rm = name.clone();
                let enabled = p.enabled;
                stack = stack.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .p(px(6.0))
                        .rounded(px(4.0))
                        .bg(colors.bg_muted)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(Text::new(name.clone()).size(Size::Sm).medium())
                                .child(
                                    Text::new(format!(
                                        "{} · {}",
                                        p.manifest.kind,
                                        if p.manifest.version.is_empty() {
                                            "—"
                                        } else {
                                            &p.manifest.version
                                        }
                                    ))
                                    .size(Size::Xs)
                                    .dimmed(),
                                ),
                        )
                        .child(
                            Button::new(
                                SharedString::from(format!("pl-tog-{name}")),
                                if enabled { "On" } else { "Off" },
                            )
                            .size(Size::Xs)
                            .variant(if enabled { Variant::Light } else { Variant::Subtle })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_plugin(name.clone(), !enabled, cx)
                            })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("pl-rm-{name_rm}")), "Remove")
                                .size(Size::Xs)
                                .variant(Variant::Subtle)
                                .color(ColorName::Red)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.uninstall_plugin(name_rm.clone(), cx)
                                })),
                        ),
                );
            }
        }

        // Registry entries that aren't already installed.
        let available: Vec<&PluginManifest> = self
            .registry
            .iter()
            .filter(|m| !self.plugins.iter().any(|p| p.manifest.name == m.name))
            .collect();
        if !available.is_empty() {
            stack = stack.child(Divider::new()).child(Text::new("Available").size(Size::Xs).dimmed());
            for m in available {
                let manifest = m.clone();
                stack = stack.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .p(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(Text::new(m.name.clone()).size(Size::Sm))
                                .child(Text::new(m.description.clone()).size(Size::Xs).dimmed()),
                        )
                        .child(
                            Button::new(SharedString::from(format!("pl-inst-{}", m.name)), "Install")
                                .size(Size::Xs)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.install_plugin(manifest.clone(), cx)
                                })),
                        ),
                );
            }
        }
        stack.into_any_element()
    }

    fn macros_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::palette(cx);
        let mut stack = Stack::new().gap(Size::Sm).child(
            Group::new().justify(Justify::End).child(
                Button::new("mac-import", "Import…")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.import_macro(cx))),
            ),
        );
        if self.macros.is_empty() {
            return stack
                .child(Text::new("No saved macros").size(Size::Xs).dimmed())
                .into_any_element();
        }
        for m in &self.macros {
            let id_exp = m.id.clone();
            let id_del = m.id.clone();
            stack = stack.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .p(px(6.0))
                    .rounded(px(4.0))
                    .bg(colors.bg_muted)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(Text::new(m.name.clone()).size(Size::Sm).medium())
                            .child(
                                Text::new(format!("{} step(s)", m.steps.len()))
                                    .size(Size::Xs)
                                    .dimmed(),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("mac-exp-{}", m.id)), "Export")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.export_macro(id_exp.clone(), cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("mac-del-{}", m.id)), "Delete")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_macro(id_del.clone(), cx)
                            })),
                    ),
            );
        }
        stack.into_any_element()
    }
}

impl Render for ExtensionsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;
        let tab_btn = |id: &'static str, label: &'static str, this_tab: Tab, cx: &mut Context<Self>| {
            Button::new(id, label)
                .size(Size::Xs)
                .variant(if tab == this_tab { Variant::Light } else { Variant::Subtle })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = this_tab;
                    cx.notify();
                }))
        };

        let content = match tab {
            Tab::Plugins => self.plugins_view(cx),
            Tab::Macros => self.macros_view(cx),
        };

        Modal::new()
            .title("Extensions")
            .width(560.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(ExtensionsEvent::Close)))
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .child(tab_btn("ext-plugins", "Plugins", Tab::Plugins, cx))
                    .child(tab_btn("ext-macros", "Macros", Tab::Macros, cx)),
            )
            .child(Divider::new())
            .child(div().id("ext-scroll").max_h(px(420.0)).overflow_y_scroll().child(content))
    }
}
