use crate::layout::{TextMeasurer, TextRequest};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) trait Host: Send + Sync {
    fn attach(&self) {}
    fn measure_text(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32);

    fn measure_text_batch(&self, requests: &[TextRequest]) -> Vec<(f32, f32)> {
        requests
            .iter()
            .map(|request| {
                self.measure_text(
                    &request.text,
                    request.font_size,
                    request.bold,
                    request.max_width,
                )
            })
            .collect()
    }

    fn emit_ops(&self, bytes: &[u8]);
}

type MeasurementKey = (String, u32, bool, u32);
type Measurement = (f32, f32);

pub(crate) struct CachedTextMeasurer {
    host: Arc<dyn Host>,
    cache: RefCell<HashMap<MeasurementKey, Measurement>>,
}

impl CachedTextMeasurer {
    pub(crate) fn new(host: Arc<dyn Host>) -> Self {
        Self {
            host,
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl TextMeasurer for CachedTextMeasurer {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        let key = (
            text.to_string(),
            font_size.to_bits(),
            bold,
            max_width.to_bits(),
        );

        if let Some(value) = self.cache.borrow().get(&key) {
            return *value;
        }

        let value = self.host.measure_text(text, font_size, bold, max_width);
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, value);
        value
    }

    fn prime(&self, requests: &[TextRequest]) {
        let missing: Vec<TextRequest> = requests
            .iter()
            .filter(|request| {
                !self.cache.borrow().contains_key(&(
                    request.text.clone(),
                    request.font_size.to_bits(),
                    request.bold,
                    request.max_width.to_bits(),
                ))
            })
            .cloned()
            .collect();
        if missing.is_empty() {
            return;
        }

        let measured = self.host.measure_text_batch(&missing);
        let mut cache = self.cache.borrow_mut();
        if cache.len() + measured.len() > 4096 {
            cache.clear();
        }
        for (request, value) in missing.into_iter().zip(measured) {
            cache.insert(
                (
                    request.text,
                    request.font_size.to_bits(),
                    request.bold,
                    request.max_width.to_bits(),
                ),
                value,
            );
        }
    }
}
