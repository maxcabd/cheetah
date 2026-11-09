use crate::actor::Command;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy)]
struct Lifecycle {
    active: bool,
    revision: u64,
}

pub(crate) struct Gateway {
    outbound: UnboundedSender<String>,
    lifecycle: watch::Sender<Lifecycle>,
}

impl Gateway {
    pub(crate) fn send(&self, payload: String) -> Result<(), ()> {
        self.outbound.send(payload).map_err(|_| ())
    }

    pub(crate) fn set_active(&self, active: bool) {
        self.lifecycle.send_modify(|state| {
            state.active = active;
            state.revision = state.revision.wrapping_add(1);
        });
    }
}

pub(crate) fn spawn(url: String, commands: UnboundedSender<Command>) -> Gateway {
    let (outbound, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (lifecycle, mut lifecycle_rx) = watch::channel(Lifecycle {
        active: true,
        revision: 0,
    });

    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);

        loop {
            if !lifecycle_rx.borrow().active {
                let _ = commands.send(Command::Connection(false));
                if !wait_active(&mut lifecycle_rx).await {
                    return;
                }
                backoff = Duration::from_secs(1);
                continue;
            }

            match connect_async(url.as_str()).await {
                Ok((ws, _)) => {
                    if !lifecycle_rx.borrow().active {
                        let _ = commands.send(Command::Connection(false));
                        while receiver.try_recv().is_ok() {}
                        if !wait_active(&mut lifecycle_rx).await {
                            return;
                        }
                        backoff = Duration::from_secs(1);
                        continue;
                    }

                    backoff = Duration::from_secs(1);
                    let _ = commands.send(Command::Connection(true));
                    let (mut write, mut read) = ws.split();
                    let mut paused = false;

                    loop {
                        tokio::select! {
                            changed = lifecycle_rx.changed() => {
                                if changed.is_err() {
                                    return;
                                }
                                let state = *lifecycle_rx.borrow();
                                let _ = state.revision;
                                if !state.active {
                                    paused = true;
                                    break;
                                }
                                // Reactivated while the socket was still open:
                                // let the actor flush its outbox again.
                                let _ = commands.send(Command::Connection(true));
                            }
                            outgoing = receiver.recv() => {
                                match outgoing {
                                    Some(payload) => {
                                        if write.send(Message::Text(payload.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    None => return,
                                }
                            }
                            incoming = read.next() => {
                                match incoming {
                                    Some(Ok(Message::Text(text))) => {
                                        handle_incoming(text.as_str(), &commands);
                                    }
                                    Some(Ok(Message::Ping(payload))) => {
                                        let _ = write.send(Message::Pong(payload)).await;
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    Some(Err(_)) => break,
                                    _ => {}
                                }
                            }
                        }
                    }

                    drop(read);
                    drop(write);
                    let _ = commands.send(Command::Connection(false));
                    while receiver.try_recv().is_ok() {}

                    if paused {
                        if !wait_active(&mut lifecycle_rx).await {
                            return;
                        }
                        backoff = Duration::from_secs(1);
                        continue;
                    }
                }
                Err(_) => {
                    let _ = commands.send(Command::Connection(false));
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                changed = lifecycle_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    if !lifecycle_rx.borrow().active {
                        if !wait_active(&mut lifecycle_rx).await {
                            return;
                        }
                        backoff = Duration::from_secs(1);
                        continue;
                    }
                }
            }
            backoff = (backoff * 2).min(Duration::from_secs(8));
        }
    });

    Gateway {
        outbound,
        lifecycle,
    }
}

async fn wait_active(lifecycle: &mut watch::Receiver<Lifecycle>) -> bool {
    loop {
        if lifecycle.borrow().active {
            return true;
        }
        if lifecycle.changed().await.is_err() {
            return false;
        }
    }
}

fn handle_incoming(text: &str, commands: &UnboundedSender<Command>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };

    match value.get("type").and_then(|value| value.as_str()) {
        Some("message") => {
            let author = value
                .get("author")
                .and_then(|value| value.as_str())
                .unwrap_or("server")
                .to_string();
            let body = value
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();

            if !body.is_empty() {
                let _ = commands.send(Command::Incoming { author, body });
            }
        }
        Some("ack") => {
            if let Some(id) = value.get("id").and_then(|value| value.as_u64()) {
                let _ = commands.send(Command::Ack { id: id as u32 });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::spawn;
    use crate::actor::Command;
    use futures_util::StreamExt;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::sync::mpsc::UnboundedReceiver;
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::accept_async;

    async fn expect_connection(receiver: &mut UnboundedReceiver<Command>, online: bool) {
        loop {
            if let Some(Command::Connection(value)) = receiver.recv().await {
                if value == online {
                    return;
                }
            }
        }
    }

    #[tokio::test]
    async fn pauses_and_reconnects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("first connection");
            let mut first = accept_async(stream).await.expect("first handshake");
            while first.next().await.is_some() {}

            let (stream, _) = listener.accept().await.expect("second connection");
            let _second = accept_async(stream).await.expect("second handshake");
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        let (commands, mut receiver) = unbounded_channel();
        let gateway = spawn(format!("ws://{address}"), commands);

        timeout(
            Duration::from_secs(5),
            expect_connection(&mut receiver, true),
        )
        .await
        .expect("initial connection");

        gateway.set_active(false);
        timeout(
            Duration::from_secs(5),
            expect_connection(&mut receiver, false),
        )
        .await
        .expect("pause should drop the socket");

        gateway.set_active(true);
        timeout(
            Duration::from_secs(5),
            expect_connection(&mut receiver, true),
        )
        .await
        .expect("resume should reconnect");

        server.abort();
    }
}
