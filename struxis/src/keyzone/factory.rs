use std::collections::HashMap;

use crate::constant::KeyZoneOrigin;
use super::builder::KeyZoneBuilder;

pub struct KeyZoneFactory {
    builders: HashMap<KeyZoneOrigin, Box<dyn KeyZoneBuilder + Send + Sync>>,
}

impl Default for KeyZoneFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyZoneFactory {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
        }
    }

    pub fn register(&mut self, builder: Box<dyn KeyZoneBuilder + Send + Sync>) {
        self.builders.insert(builder.origin_type(), builder);
    }

    pub fn create(&self, origin_type: KeyZoneOrigin) -> Option<&(dyn KeyZoneBuilder + Send + Sync)> {
        self.builders.get(&origin_type).map(|x| x.as_ref())
    }
}
