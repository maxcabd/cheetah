use crate::layout::TextMeasurer;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) trait Host: Send + Sync {
    fn attach(&self) {}
    fn measure_text(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32);
    fn emit_ops(&self, json: &str);
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
}
