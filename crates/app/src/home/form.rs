//! The new/edit connection form.
//!
//! A modal over the host's General settings. SSL and SSH are round-tripped from
//! the edited connection untouched (their editors land in a later pass).

use gpui::prelude::*;
use gpui::{Context, Entity, EventEmitter, Window};
use guise::prelude::*;

use crate::bridge;
use crate::sheet::Sheet;
use crate::state::AppState;
use model::{ConnectionTestResult, SshConfig, SslConfig, StoredConnection};

/// What the form asks `Home` to do; the actual persistence lives there.
pub enum ConnectionFormEvent {
    Save(Box<StoredConnection>),
    Cancel,
}

const DEFAULT_COLOR: &str = "#339AF0";
const KINDS: [&str; 3] = ["PostgreSQL", "MySQL / MariaDB", "SQLite"];

pub struct ConnectionForm {
    state: AppState,
    initial: Option<StoredConnection>,
    name: Entity<TextInput>,
    kind: Entity<Select>,
    host: Entity<TextInput>,
    port: Entity<NumberInput>,
    database: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    filepath: Entity<TextInput>,
    color: Entity<TextInput>,
    group: Entity<TextInput>,
    tags: Entity<TextInput>,
    startup: Entity<TextArea>,
    safe_mode: Entity<Select>,
    // SSL.
    ssl_mode: Entity<Select>,
    ssl_ca: Entity<TextInput>,
    ssl_cert: Entity<TextInput>,
    ssl_key: Entity<TextInput>,
    // SSH tunnel.
    ssh_enabled: bool,
    ssh_host: Entity<TextInput>,
    ssh_port: Entity<NumberInput>,
    ssh_username: Entity<TextInput>,
    ssh_auth: Entity<Select>,
    ssh_password: Entity<TextInput>,
    ssh_key: Entity<TextInput>,
    test_result: Signal<Option<ConnectionTestResult>>,
    testing: Signal<bool>,
}

impl EventEmitter<ConnectionFormEvent> for ConnectionForm {}

impl ConnectionForm {
    pub fn new(initial: Option<StoredConnection>, cx: &mut Context<Self>) -> Self {
        let state = AppState::get(cx);
        let field = |value: String| value;

        let kind_index = match initial.as_ref().map(|c| c.kind.as_str()) {
            Some("mysql") => 1,
            Some("sqlite") => 2,
            _ => 0,
        };
        let safe_index = match initial.as_ref().and_then(|c| c.safe_mode.as_deref()) {
            Some("confirm") => 1,
            Some("readonly") => 2,
            _ => 0,
        };
        let port_value = initial.as_ref().map(|c| c.port as f64).unwrap_or(5432.0);

        let text = |cx: &mut Context<Self>, label: &str, placeholder: &str, value: String| {
            let label = label.to_string();
            let placeholder = placeholder.to_string();
            cx.new(move |cx| {
                TextInput::new(cx)
                    .label(label)
                    .placeholder(placeholder)
                    .value(&value)
            })
        };

        let g = |get: fn(&StoredConnection) -> String| initial.as_ref().map(get).unwrap_or_default();

        let name = text(cx, "Name", "My Database", g(|c| c.name.clone()));
        let kind = cx.new(move |cx| Select::new(cx).label("Type").data(KINDS).selected(kind_index));
        let host = text(cx, "Host", "localhost", g(|c| c.host.clone()));
        let port = cx.new(move |cx| NumberInput::new(cx).label("Port").value(port_value));
        let database = text(cx, "Database", "mydb", g(|c| c.database.clone()));
        let username = text(cx, "Username", "postgres", g(|c| c.username.clone()));
        let password = cx.new({
            let value = g(|c| c.password.clone());
            move |cx| {
                TextInput::new(cx)
                    .label("Password")
                    .placeholder("••••••••")
                    .password(true)
                    .value(&value)
            }
        });
        let filepath = text(
            cx,
            "File Path",
            "/path/to/database.db",
            g(|c| c.filepath.clone().unwrap_or_default()),
        );
        let color = text(
            cx,
            "Accent Color",
            DEFAULT_COLOR,
            field(
                initial
                    .as_ref()
                    .map(|c| c.color.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            ),
        );
        let group = text(cx, "Group", "Production, Staging, Local…", g(|c| c.group.clone().unwrap_or_default()));
        let tags = text(
            cx,
            "Tags",
            "backend, analytics (comma separated)",
            initial
                .as_ref()
                .and_then(|c| c.tags.clone())
                .map(|t| t.join(", "))
                .unwrap_or_default(),
        );
        let startup = cx.new({
            let value = g(|c| c.startup_commands.clone().unwrap_or_default());
            move |cx| {
                TextArea::new(cx)
                    .label("Startup Commands")
                    .placeholder("SET timezone = 'UTC';")
                    .rows(2)
                    .value(&value)
            }
        });
        let safe_mode = cx.new(move |cx| {
            Select::new(cx)
                .label("Safe Mode")
                .data([
                    "Off — no restrictions",
                    "Confirm — prompt before writes",
                    "Read-only — block all writes",
                ])
                .selected(safe_index)
        });

        let ssl = initial.as_ref().and_then(|c| c.ssl.clone());
        let ssl_index = match ssl.as_ref().map(|s| s.mode.as_str()) {
            Some("required") => 1,
            Some("verify-ca") => 2,
            Some("verify-identity") => 3,
            _ => 0,
        };
        let ssl_mode = cx.new(move |cx| {
            Select::new(cx)
                .label("SSL Mode")
                .data(["Disabled", "Required", "Verify CA", "Verify Identity"])
                .selected(ssl_index)
        });
        let ssl_ca = text(cx, "CA Certificate", "/path/to/ca.pem", ssl.as_ref().and_then(|s| s.ca.clone()).unwrap_or_default());
        let ssl_cert = text(cx, "Client Certificate", "/path/to/cert.pem", ssl.as_ref().and_then(|s| s.cert.clone()).unwrap_or_default());
        let ssl_key = text(cx, "Client Key", "/path/to/key.pem", ssl.as_ref().and_then(|s| s.key.clone()).unwrap_or_default());

        let ssh = initial.as_ref().and_then(|c| c.ssh.clone());
        let ssh_enabled = ssh.as_ref().map(|s| s.enabled).unwrap_or(false);
        let ssh_port_value = ssh.as_ref().map(|s| s.port as f64).filter(|p| *p > 0.0).unwrap_or(22.0);
        let ssh_host = text(cx, "SSH Host", "bastion.example.com", ssh.as_ref().map(|s| s.host.clone()).unwrap_or_default());
        let ssh_port = cx.new(move |cx| NumberInput::new(cx).label("SSH Port").value(ssh_port_value));
        let ssh_username = text(cx, "SSH User", "ubuntu", ssh.as_ref().map(|s| s.username.clone()).unwrap_or_default());
        let ssh_auth_index = match ssh.as_ref().map(|s| s.auth_method.as_str()) {
            Some("key") => 1,
            _ => 0,
        };
        let ssh_auth = cx.new(move |cx| {
            Select::new(cx).label("SSH Auth").data(["Password", "Key"]).selected(ssh_auth_index)
        });
        let ssh_password = cx.new({
            let value = ssh.as_ref().and_then(|s| s.password.clone()).unwrap_or_default();
            move |cx| TextInput::new(cx).label("SSH Password").password(true).value(&value)
        });
        let ssh_key = text(cx, "SSH Key Path", "~/.ssh/id_rsa", ssh.as_ref().and_then(|s| s.key_path.clone()).unwrap_or_default());

        let test_result = Signal::new(cx, None::<ConnectionTestResult>);
        watch(cx, &test_result);
        let testing = Signal::new(cx, false);
        watch(cx, &testing);
        // Re-render the form when the type / SSL / SSH-auth changes so dependent
        // fields swap.
        cx.subscribe(&kind, |_this, _select, _event: &SelectEvent, cx| cx.notify())
            .detach();
        cx.subscribe(&ssl_mode, |_this, _select, _event: &SelectEvent, cx| cx.notify())
            .detach();
        cx.subscribe(&ssh_auth, |_this, _select, _event: &SelectEvent, cx| cx.notify())
            .detach();

        ConnectionForm {
            state,
            initial,
            name,
            kind,
            host,
            port,
            database,
            username,
            password,
            filepath,
            color,
            group,
            tags,
            startup,
            safe_mode,
            ssl_mode,
            ssl_ca,
            ssl_cert,
            ssl_key,
            ssh_enabled,
            ssh_host,
            ssh_port,
            ssh_username,
            ssh_auth,
            ssh_password,
            ssh_key,
            test_result,
            testing,
        }
    }

    fn is_sqlite(&self, cx: &Context<Self>) -> bool {
        self.kind.read(cx).selected_index() == Some(2)
    }

    /// Assemble the edited fields into a `StoredConnection`, preserving the id,
    /// SSL/SSH, and any unknown fields from the connection being edited.
    fn build(&self, cx: &Context<Self>) -> StoredConnection {
        let opt = |value: String| (!value.is_empty()).then_some(value);
        let kind = match self.kind.read(cx).selected_index().unwrap_or(0) {
            1 => "mysql",
            2 => "sqlite",
            _ => "postgres",
        }
        .to_string();
        let safe_mode = match self.safe_mode.read(cx).selected_index().unwrap_or(0) {
            1 => Some("confirm".to_string()),
            2 => Some("readonly".to_string()),
            _ => None,
        };
        let tags: Vec<String> = self
            .tags
            .read(cx)
            .text()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        let ssl_mode = match self.ssl_mode.read(cx).selected_index().unwrap_or(0) {
            1 => "required",
            2 => "verify-ca",
            3 => "verify-identity",
            _ => "disabled",
        };
        let ssl = (ssl_mode != "disabled").then(|| SslConfig {
            mode: ssl_mode.to_string(),
            ca: opt(self.ssl_ca.read(cx).text()),
            cert: opt(self.ssl_cert.read(cx).text()),
            key: opt(self.ssl_key.read(cx).text()),
        });
        let ssh = self.ssh_enabled.then(|| {
            let auth = match self.ssh_auth.read(cx).selected_index().unwrap_or(0) {
                1 => "key",
                _ => "password",
            };
            SshConfig {
                enabled: true,
                host: self.ssh_host.read(cx).text(),
                port: self.ssh_port.read(cx).value_f64().unwrap_or(22.0) as u16,
                username: self.ssh_username.read(cx).text(),
                auth_method: auth.to_string(),
                password: opt(self.ssh_password.read(cx).text()),
                key_path: opt(self.ssh_key.read(cx).text()),
            }
        });
        let base = self.initial.clone();

        StoredConnection {
            id: base.as_ref().map(|c| c.id.clone()).unwrap_or_default(),
            name: self.name.read(cx).text(),
            kind,
            host: self.host.read(cx).text(),
            port: self.port.read(cx).value_f64().unwrap_or(0.0) as u16,
            database: self.database.read(cx).text(),
            username: self.username.read(cx).text(),
            password: self.password.read(cx).text(),
            color: self.color.read(cx).text(),
            filepath: opt(self.filepath.read(cx).text()),
            ssl,
            ssh,
            startup_commands: opt(self.startup.read(cx).text()),
            safe_mode,
            group: opt(self.group.read(cx).text()),
            tags: (!tags.is_empty()).then_some(tags),
            extra: base.map(|c| c.extra).unwrap_or_default(),
        }
    }

    fn test(&self, cx: &mut Context<Self>) {
        let conn = self.build(cx);
        let host = self.state.host.clone();
        let result = self.test_result.clone();
        let testing = self.testing.clone();
        self.testing.set(cx, true);
        self.test_result.set(cx, None); // clear any stale result while testing
        bridge::run(
            cx,
            async move { host.test_connection(&conn).await },
            move |outcome, cx| {
                testing.set(cx, false);
                result.set(cx, Some(outcome));
            },
        );
    }
}

impl Render for ConnectionForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_sqlite = self.is_sqlite(cx);
        let editing = self.initial.is_some();
        let testing = *self.testing.read(cx);

        let mut fields = Stack::new().gap(Size::Sm).child(
            Group::new()
                .grow(true)
                .gap(Size::Sm)
                .child(self.name.clone())
                .child(self.kind.clone()),
        );

        if is_sqlite {
            fields = fields.child(self.filepath.clone());
        } else {
            fields = fields
                .child(
                    Group::new()
                        .grow(true)
                        .gap(Size::Sm)
                        .child(self.host.clone())
                        .child(self.port.clone()),
                )
                .child(self.database.clone())
                .child(
                    Group::new()
                        .grow(true)
                        .gap(Size::Sm)
                        .child(self.username.clone())
                        .child(self.password.clone()),
                );
        }

        fields = fields
            .child(self.color.clone())
            .child(self.safe_mode.clone())
            .child(
                Group::new()
                    .grow(true)
                    .gap(Size::Sm)
                    .child(self.group.clone())
                    .child(self.tags.clone()),
            )
            .child(self.startup.clone());

        // SSL / SSH — only meaningful for server connections.
        if !is_sqlite {
            let ssl_on = self.ssl_mode.read(cx).selected_index().unwrap_or(0) != 0;
            let ssh_on = self.ssh_enabled;
            let ssh_key_auth = self.ssh_auth.read(cx).selected_index() == Some(1);

            fields = fields
                .child(Divider::new())
                .child(Text::new("SSL").size(Size::Xs).dimmed())
                .child(self.ssl_mode.clone());
            if ssl_on {
                fields = fields
                    .child(self.ssl_ca.clone())
                    .child(
                        Group::new()
                            .grow(true)
                            .gap(Size::Sm)
                            .child(self.ssl_cert.clone())
                            .child(self.ssl_key.clone()),
                    );
            }

            fields = fields.child(Divider::new()).child(
                Group::new()
                    .justify(Justify::Between)
                    .align(Align::Center)
                    .child(Text::new("SSH Tunnel").size(Size::Xs).dimmed())
                    .child(
                        Button::new("form-ssh-toggle", if ssh_on { "On" } else { "Off" })
                            .size(Size::Xs)
                            .variant(if ssh_on { Variant::Light } else { Variant::Subtle })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ssh_enabled = !this.ssh_enabled;
                                cx.notify();
                            })),
                    ),
            );
            if ssh_on {
                fields = fields
                    .child(
                        Group::new()
                            .grow(true)
                            .gap(Size::Sm)
                            .child(self.ssh_host.clone())
                            .child(self.ssh_port.clone()),
                    )
                    .child(
                        Group::new()
                            .grow(true)
                            .gap(Size::Sm)
                            .child(self.ssh_username.clone())
                            .child(self.ssh_auth.clone()),
                    )
                    .child(if ssh_key_auth {
                        self.ssh_key.clone().into_any_element()
                    } else {
                        self.ssh_password.clone().into_any_element()
                    });
            }
        }

        if let Some(result) = self.test_result.read(cx).clone() {
            let alert = if result.ok {
                Alert::new(format!("Connected — {}", result.version.unwrap_or_default()))
                    .color(ColorName::Teal)
                    .icon("✓")
            } else {
                Alert::new(result.error.unwrap_or_default())
                    .color(ColorName::Red)
                    .icon("✕")
            };
            fields = fields.child(alert);
        }

        let actions = Group::new()
            .justify(Justify::Between)
            .child(
                Button::new("form-test", if testing { "Testing…" } else { "Test" })
                    .variant(Variant::Default)
                    .disabled(testing)
                    .on_click(cx.listener(|this, _, _, cx| this.test(cx))),
            )
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .child(
                        Button::new("form-cancel", "Cancel")
                            .variant(Variant::Subtle)
                            .color(ColorName::Gray)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ConnectionFormEvent::Cancel))),
                    )
                    .child(
                        Button::new("form-save", if editing { "Save" } else { "Create" }).on_click(
                            cx.listener(|this, _, _, cx| {
                                let conn = this.build(cx);
                                cx.emit(ConnectionFormEvent::Save(Box::new(conn)));
                            }),
                        ),
                    ),
            );

        Sheet::new()
            .title(if editing { "Edit Connection" } else { "New Connection" })
            .width(560.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(ConnectionFormEvent::Cancel)))
            .child(fields)
            .child(Divider::new())
            .child(actions)
    }
}
