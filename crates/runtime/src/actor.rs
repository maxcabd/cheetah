use crate::app::{Action, Event, Runtime, Viewport};
use crate::demo::ChatApp;
use crate::ffi::{CachedTextMeasurer, JniHost};
use crate::gateway;
use crate::Op;
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
    Connection(bool),
    Shutdown,
}

pub(crate) struct ActorHandle {
    pub(crate) sender: UnboundedSender<Command>,
    pub(crate) join: Option<JoinHandle<()>>,
}

pub(crate) fn spawn(host: Arc<JniHost>, store_path: String, gateway_url: String) -> ActorHandle {
    let (sender, mut receiver) = unbounded_channel();
    let actor_sender = sender.clone();

    let join = std::thread::Builder::new()
        .name("cheetah-ui".into())
        .spawn(move || {
            host.attach_current_thread();

            let mut runtime = Runtime::new(ChatApp::new(&store_path));
            runtime.set_text_measurer(Rc::new(CachedTextMeasurer::new(Arc::clone(&host))));

            let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");

            let gateway = {
                let _guard = tokio_runtime.enter();
                gateway::spawn(gateway_url, actor_sender.clone())
            };

            while let Some(command) = receiver.blocking_recv() {
                match command {
                    Command::Resize(viewport) => {
                        let ops = runtime.render(viewport);
                        emit(&host, ops);
                    }
                    Command::Event { id, kind, payload } => {
                        let ops = runtime.event(id, &kind, &payload);
                        emit(&host, ops);

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
                                        dispatch_fail(&mut runtime, &host, id);
                                    } else {
                                        spawn_send_timeout(
                                            &tokio_runtime,
                                            actor_sender.clone(),
                                            id,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Command::Incoming { author, body } => {
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "incoming".into(),
                            payload: format!("{author}\u{1f}{body}"),
                        };
                        let ops = runtime.dispatch(event);
                        emit(&host, ops);
                    }
                    Command::Ack { id } => {
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "ack".into(),
                            payload: id.to_string(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(&host, ops);
                    }
                    Command::SendTimeout { id } => {
                        dispatch_fail(&mut runtime, &host, id);
                    }
                    Command::Connection(online) => {
                        let event = Event {
                            id: 0,
                            key: None,
                            kind: "connection".into(),
                            payload: if online { "online" } else { "offline" }.into(),
                        };
                        let ops = runtime.dispatch(event);
                        emit(&host, ops);
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

fn emit(host: &JniHost, ops: Vec<Op>) {
    if ops.is_empty() {
        return;
    }
    if let Ok(json) = serde_json::to_string(&ops) {
        host.emit_ops(&json);
    }
}

fn dispatch_fail(runtime: &mut Runtime, host: &JniHost, id: u32) {
    let event = Event {
        id: 0,
        key: None,
        kind: "fail".into(),
        payload: id.to_string(),
    };
    let ops = runtime.dispatch(event);
    emit(host, ops);
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
