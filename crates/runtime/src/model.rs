use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    #[default]
    Sent,
    Queued,
    Sending,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: u32,
    pub author: String,
    pub body: String,
    pub time: String,
    pub color: String,
    pub likes: u32,
    pub liked: bool,
    #[serde(default)]
    pub status: MessageStatus,
    #[serde(default)]
    pub attempts: u32,
}
