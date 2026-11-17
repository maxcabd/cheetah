use crate::app::{Action, Event, Runtime, Viewport};
use crate::bridge::{CachedTextMeasurer, Host};
use crate::demo::ChatApp;
use crate::gateway::{self, Gateway};
#[cfg(not(feature = "devserver"))]
use crate::nightshade::NightshadeApp;
use crate::Op;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub(crate) enum Command {
    Resize(Viewport),
    Event {
        id: u32,
        kind: String,
        payload: String,
    },
    Incoming {
        author: String,
        body: String,
    },
    Ack {
        id: u32,
    },
    SendTimeout {
        id: u32,
    },
    Retry,
    Connection(bool),
    Lifecycle(bool),
    /// App code was hot-patched: re-render with the new code.
    #[cfg_attr(not(feature = "devserver"), allow(dead_code))]
    Rerender,
    /// Live literal edits from the dev server: apply and re-render.
    #[cfg(feature = "devserver")]
    Literals(crate::live::LiteralOverrides),
    /// New `.view` templates from the dev server: swap and re-render.
    #[cfg(feature = "devserver")]
    Views(crate::views::Views),
    Shutdown,
}

pub(crate) struct ActorHandle {
    pub(crate) sender: UnboundedSender<Command>,
    pub(crate) join: Option<JoinHandle<()>>,
}

pub(crate) fn spawn(
    host: Arc<dyn Host>,
    store_path: String,
    gateway_url: String,
    app: String,
) -> ActorHandle {
    spawn_with(host, gateway_url, move || {
        // Dev-server builds run Nightshade from apps/nightshade instead.
        #[cfg(not(feature = "devserver"))]
        if app == "nightshade" || app == "nightshade-demo" {
            let state = std::path::Path::new(&store_path).with_file_name("nightshade-state.redb");
            return Runtime::new(NightshadeApp::new(app == "nightshade-demo").with_state_file(state));
        }
        let _ = &app;
        Runtime::new(ChatApp::new(&store_path))
    })
}

/// Start the UI actor with a caller-built runtime. `make` runs on the actor
/// thread, so the app itself need not be `Send`.
pub(crate) fn spawn_with(
    host: Arc<dyn Host>,
    gateway_url: String,
    make: impl FnOnce() -> Runtime + Send + 'static,
) -> ActorHandle {
    let (sender, mut receiver) = unbounded_channel();
    let actor_sender = sender.clone();

    let join = std::thread::Builder::new()
        .name("cheetah-ui".into())
        .spawn(move || {
            host.attach();

            let mut runtime = make();
            runtime.set_text_measurer(Rc::new(CachedTextMeasurer::new(Arc::clone(&host))));

            let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");

            let gateway = {
                let _guard = tokio_runtime.enter();
                gateway::spawn(gateway_url, actor_sender.clone())
            };

            let mut paused = false;
            let mut pending: VecDeque<Command> = VecDeque::new();

            while let Some(command) = pending.pop_front().or_else(|| receiver.blocking_recv()) {
                match command {
                    Command::Resize(viewport) => {
                        let ops = runtime.render(viewport);
                        emit(host.as_ref(), ops);
                    }
                    Command::Event { id, kind, payload } => {
                        runtime.apply_event(id, &kind, &payload);
                        while let Ok(next) = receiver.try_recv() {
                            match next {
                                Command::Event { id, kind, payload } => {
                                    runtime.apply_event(id, &kind, &payload);
                                }
                                other => {
                                    pending.push_back(other);
                                    break;
                                }
                            }
                        }
                        let ops = runtime.render_current();
                        emit(host.as_ref(), ops);
                        process_actions(
                            &mut runtime,
                            host.as_ref(),
                            &tokio_runtime,
                            &gateway,
                            &actor_sender,
                        );
                    }
                    Command::Incoming { author, body } => {
                        if paused {
                            continue;
                        }
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "incoming".into(),
                            payload: format!("{author}\u{1f}{body}"),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);
                    }
                    Command::Ack { id } => {
                        if paused {
                            continue;
                        }
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "ack".into(),
                            payload: id.to_string(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);
                    }
                    Command::SendTimeout { id } => {
                        if paused {
                            continue;
                        }
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "fail".into(),
                            payload: id.to_string(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);

                        if runtime.has_queued() {
                            spawn_retry(&tokio_runtime, actor_sender.clone());
                        }
                    }
                    Command::Retry => {
                        if paused {
                            continue;
                        }
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "flush".into(),
                            payload: String::new(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);
                        process_actions(
                            &mut runtime,
                            host.as_ref(),
                            &tokio_runtime,
                            &gateway,
                            &actor_sender,
                        );
                    }
                    Command::Connection(online) => {
                        if paused {
                            continue;
                        }
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "connection".into(),
                            payload: if online { "online" } else { "offline" }.into(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);
                        process_actions(
                            &mut runtime,
                            host.as_ref(),
                            &tokio_runtime,
                            &gateway,
                            &actor_sender,
                        );
                    }
                    Command::Lifecycle(active) => {
                        if active == !paused {
                            continue;
                        }
                        paused = !active;
                        gateway.set_active(active);
                        // Apps save state here: a backgrounded app may be
                        // ended without another event.
                        let lifecycle = Event {
                            id: 0,
                            key: None,
                            kind: "lifecycle".into(),
                            payload: if active { "active" } else { "background" }.into(),
                        };
                        let ops = runtime.dispatch(lifecycle);
                        emit(host.as_ref(), ops);
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "connection".into(),
                            payload: if active { "connecting" } else { "offline" }.into(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(host.as_ref(), ops);
                    }
                    Command::Rerender => {
                        let ops = runtime.rerender();
                        emit(host.as_ref(), ops);
                    }
                    #[cfg(feature = "devserver")]
                    Command::Literals(literals) => {
                        let ops = runtime.set_literals(literals);
                        emit(host.as_ref(), ops);
                    }
                    #[cfg(feature = "devserver")]
                    Command::Views(views) => {
                        let ops = runtime.set_views(views);
                        emit(host.as_ref(), ops);
                    }
                    Command::Shutdown => break,
                }
            }
        })
        .expect("cheetah ui thread");

    ActorHandle {
        sender,
        join: Some(join),
    }
}

fn emit(host: &dyn Host, ops: Vec<Op>) {
    if ops.is_empty() {
        return;
    }
    host.emit_ops(&crate::wire::encode(&ops));
}

fn process_actions(
    runtime: &mut Runtime,
    host: &dyn Host,
    tokio_runtime: &tokio::runtime::Runtime,
    gateway: &Gateway,
    sender: &UnboundedSender<Command>,
) {
    for action in runtime.take_actions() {
        match action {
            Action::SendMessage { id, body } => {
                let payload = serde_json::json!({
                    "type": "send",
                    "id": id,
                    "body": body,
                })
                .to_string();

                if gateway.send(payload).is_err() {
                    let event = Event {
                        id: 0,
                        key: None,
                        kind: "fail".into(),
                        payload: id.to_string(),
                    };
                    let ops = runtime.dispatch(event);
                    emit(host, ops);

                    if runtime.has_queued() {
                        spawn_retry(tokio_runtime, sender.clone());
                    }
                } else {
                    spawn_send_timeout(tokio_runtime, sender.clone(), id);
                }
            }
            Action::Timer { name, ms } => {
                let sender = sender.clone();
                tokio_runtime.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    let _ = sender.send(Command::Event {
                        id: 0,
                        kind: "timer".into(),
                        payload: name,
                    });
                });
            }
        }
    }
}

fn spawn_send_timeout(
    runtime: &tokio::runtime::Runtime,
    sender: UnboundedSender<Command>,
    id: u32,
) {
    runtime.spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        let _ = sender.send(Command::SendTimeout { id });
    });
}

fn spawn_retry(runtime: &tokio::runtime::Runtime, sender: UnboundedSender<Command>) {
    runtime.spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = sender.send(Command::Retry);
    });
}
