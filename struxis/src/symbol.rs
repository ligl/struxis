use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradingSession {
    pub sections: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub asset_type: String,
    pub code: String,
    pub name: String,
    pub exchange: String,
    pub tick_size: f64,
    pub price_precision: u32,
    pub currency: String,
    #[serde(default)]
    pub sessions: TradingSession,
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub multiplier: Option<f64>,
    #[serde(default)]
    pub margin_rate: Option<f64>,
    #[serde(default)]
    pub fee_rate: Option<f64>,
}

impl Symbol {
    pub fn round_price(&self, price: f64) -> f64 {
        let rounded = (price / self.tick_size).round() * self.tick_size;
        let scale = 10f64.powi(self.price_precision as i32);
        (rounded * scale).round() / scale
    }
}

static SYMBOLS: OnceLock<Mutex<HashMap<String, Symbol>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, Symbol>> {
    SYMBOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct SymbolRegistry;

impl SymbolRegistry {
    pub fn register(symbol: Symbol) {
        let mut guard = registry().lock().expect("symbol registry poisoned");
        guard.insert(symbol.code.clone(), symbol);
    }

    pub fn get(code: &str) -> Option<Symbol> {
        let guard = registry().lock().expect("symbol registry poisoned");
        guard.get(code).cloned()
    }

    pub fn exists(code: &str) -> bool {
        let guard = registry().lock().expect("symbol registry poisoned");
        guard.contains_key(code)
    }

    pub fn all() -> Vec<Symbol> {
        let guard = registry().lock().expect("symbol registry poisoned");
        guard.values().cloned().collect()
    }

    pub fn clear() {
        let mut guard = registry().lock().expect("symbol registry poisoned");
        guard.clear();
    }
}

pub struct SymbolLoader;

impl SymbolLoader {
    pub fn load(path: impl AsRef<Path>) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;

        let items: HashMap<String, Symbol> = match path.extension().and_then(|x| x.to_str()) {
            Some("json") => {
                let value: JsonValue = serde_json::from_str(&text)?;
                serde_json::from_value(value)?
            }
            Some("yaml") | Some("yml") => {
                let value: YamlValue = serde_yaml::from_str(&text)?;
                serde_yaml::from_value(value)?
            }
            _ => return Err("unsupported symbol file format".into()),
        };

        for (_, symbol) in items {
            SymbolRegistry::register(symbol);
        }
        Ok(SymbolRegistry::all())
    }
}
