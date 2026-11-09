use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:9001")
        .await
        .expect("bind 0.0.0.0:9001");
    println!("cheetah gateway listening on ws://0.0.0.0:9001");

    let (broadcast_tx, _) = broadcast::channel::<String>(256);
    let live_tx = broadcast_tx.clone();
    tokio::spawn(async move {
        let authors = ["mara", "lena", "dev", "alex"];
        let bodies = [
            "live update from the gateway",
            "this arrived over a websocket",
            "server time is real time",
            "the rust store just persisted this",
        ];
        let mut index = 0usize;

        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let message = json!({
                "type": "message",
                "author": authors[index % authors.len()],
                "body": format!("{} (#live {})", bodies[index % bodies.len()], index + 1),
            })
            .to_string();
            let _ = live_tx.send(message);
            index += 1;
        }
    });

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(value) => value,
            Err(_) => continue,
        };
        let broadcast_tx = broadcast_tx.clone();

        tokio::spawn(async move {
            let ws = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            let (mut write, mut read) = ws.split();
            let mut live_rx = broadcast_tx.subscribe();
            println!("client connected: {peer}");

            loop {
                tokio::select! {
                    incoming = read.next() => {
                        match incoming {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.as_str()) {
                                    if value.get("type").and_then(|value| value.as_str()) == Some("send") {
                                        let id = value.get("id").and_then(|value| value.as_u64()).unwrap_or(0);
                                        let ack = json!({"type": "ack", "id": id}).to_string();
                                        if write.send(Message::Text(ack.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                let _ = write.send(Message::Pong(payload)).await;
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Err(_)) => break,
                            _ => {}
                        }
                    }
                    live = live_rx.recv() => {
                        match live {
                            Ok(message) => {
                                if write.send(Message::Text(message.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }

            println!("client disconnected: {peer}");
        });
    }
}
