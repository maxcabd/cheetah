use crate::app::{Action, Event, Runtime, Viewport};
use crate::demo::ChatApp;
use crate::ffi::{CachedTextMeasurer, JniHost};
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
    Shutdown,
}

pub(crate) struct ActorHandle {
    pub(crate) sender: UnboundedSender<Command>,
    pub(crate) join: Option<JoinHandle<()>>,
}

pub(crate) fn spawn(host: Arc<JniHost>) -> ActorHandle {
    let (sender, mut receiver) = unbounded_channel();
    let actor_sender = sender.clone();

    let join = std::thread::Builder::new()
        .name("cheetah-ui".into())
        .spawn(move || {
            host.attach_current_thread();

            let mut runtime = Runtime::new(ChatApp::new());
            runtime.set_text_measurer(Rc::new(CachedTextMeasurer::new(Arc::clone(&host))));

            let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");

            let gateway_sender = actor_sender.clone();
            tokio_runtime.spawn(async move {
                fake_gateway(gateway_sender).await;
            });

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
                                Action::SendMessage { id } => {
                                    spawn_ack(&tokio_runtime, actor_sender.clone(), id);
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

fn spawn_ack(runtime: &tokio::runtime::Runtime, sender: UnboundedSender<Command>, id: u32) {
    runtime.spawn(async move {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let _ = sender.send(Command::Ack { id });
    });
}

async fn fake_gateway(sender: UnboundedSender<Command>) {
    let authors = ["mara", "lena", "dev", "alex"];
    let bodies = [
        "live update from the fake gateway",
        "a message arrived off the UI thread",
        "tokio just pushed this into the actor",
        "the UI thread only applies ops",
    ];

    let mut index = 0usize;
    loop {
        tokio::time::sleep(Duration::from_secs(4)).await;
        let author = authors[index % authors.len()].to_string();
        let body = format!("{} (#live {})", bodies[index % bodies.len()], index + 1);

        if sender.send(Command::Incoming { author, body }).is_err() {
            break;
        }
        index += 1;
    }
}
