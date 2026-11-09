use crate::actor::Command;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub(crate) struct Gateway {
    outbound: UnboundedSender<String>,
}

impl Gateway {
    pub(crate) fn send(&self, payload: String) -> Result<(), ()> {
        self.outbound.send(payload).map_err(|_| ())
    }
}

pub(crate) fn spawn(url: String, commands: UnboundedSender<Command>) -> Gateway {
    let (outbound, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);

        loop {
            match connect_async(url.as_str()).await {
                Ok((ws, _)) => {
                    backoff = Duration::from_secs(1);
                    let _ = commands.send(Command::Connection(true));
                    let (mut write, mut read) = ws.split();

                    loop {
                        tokio::select! {
                            outgoing = receiver.recv() => {
                                match outgoing {
                                    Some(payload) => {
                                        if write.send(Message::Text(payload.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    None => break,
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

                    let _ = commands.send(Command::Connection(false));
                    while receiver.try_recv().is_ok() {}
                }
                Err(_) => {
                    let _ = commands.send(Command::Connection(false));
                }
            }

            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(8));
        }
    });

    Gateway { outbound }
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
