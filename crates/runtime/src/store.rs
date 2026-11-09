use crate::model::Message;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::Path;

const MESSAGES: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const META: TableDefinition<u8, u8> = TableDefinition::new("meta");
const INITIALIZED: u8 = 1;

pub struct Store {
    db: Database,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::create(path)?;
        Ok(Self { db })
    }

    pub fn load(&self) -> Vec<Message> {
        let mut messages: Vec<Message> = Vec::new();

        if let Ok(read) = self.db.begin_read() {
            if let Ok(table) = read.open_table(MESSAGES) {
                if let Ok(iter) = table.iter() {
                    for item in iter.flatten() {
                        let (_, value) = item;
                        if let Ok(message) = serde_json::from_slice(value.value()) {
                            messages.push(message);
                        }
                    }
                }
            }
        }

        messages.sort_by_key(|message| message.id);
        messages
    }

    pub fn save(&self, message: &Message) {
        if let Ok(write) = self.db.begin_write() {
            if let Ok(mut table) = write.open_table(MESSAGES) {
                if let Ok(bytes) = serde_json::to_vec(message) {
                    let _ = table.insert(message.id as u64, bytes.as_slice());
                }
            }
            let _ = write.commit();
        }
    }

    pub fn save_all(&self, messages: &[Message]) {
        if let Ok(write) = self.db.begin_write() {
            if let Ok(mut table) = write.open_table(MESSAGES) {
                for message in messages {
                    if let Ok(bytes) = serde_json::to_vec(message) {
                        let _ = table.insert(message.id as u64, bytes.as_slice());
                    }
                }
            }
            let _ = write.commit();
        }
    }

    pub fn delete(&self, id: u32) {
        if let Ok(write) = self.db.begin_write() {
            if let Ok(mut table) = write.open_table(MESSAGES) {
                let _ = table.remove(id as u64);
            }
            let _ = write.commit();
        }
    }

    pub fn clear(&self) {
        if let Ok(write) = self.db.begin_write() {
            let _ = write.delete_table(TableDefinition::<u64, &[u8]>::new("messages"));
            let _ = write.commit();
        }
    }

    pub fn initialized(&self) -> bool {
        self.db
            .begin_read()
            .ok()
            .and_then(|read| read.open_table(META).ok())
            .and_then(|table| table.get(INITIALIZED).ok().flatten())
            .is_some()
    }

    pub fn mark_initialized(&self) {
        if let Ok(write) = self.db.begin_write() {
            if let Ok(mut table) = write.open_table(META) {
                let _ = table.insert(INITIALIZED, 1);
            }
            let _ = write.commit();
        }
    }
}
