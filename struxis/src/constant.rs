use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FractalType {
    Top,
    Bottom,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Range,
    None,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    SBarCreated,
    CBarChanged,
    SwingChanged,
    TrendChanged,
    MtcNewBar,
    TimeframeEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M1,
    M5,
    M15,
    H1,
    D1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyZoneOrigin {
    Swing,
    Trend,
    Channel,
    Ema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyZoneOrientation {
    Horizontal,
    Trendline,
    Channel,
}

impl Timeframe {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::H1 => "1h",
            Self::D1 => "1d",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DataError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1m" => Ok(Self::M1),
            "5m" => Ok(Self::M5),
            "15m" => Ok(Self::M15),
            "1h" => Ok(Self::H1),
            "1d" => Ok(Self::D1),
            _ => Err(DataError::InvalidTimeframe(value.to_string())),
        }
    }
}

#[derive(Debug)]
pub enum DataError {
    InvalidTimeframe(String),
    InvalidDatetime(String),
    Io(std::io::Error),
    Csv(csv::Error),
    Polars(polars::error::PolarsError),
}

pub struct Const;

impl Const {
    pub const DEBUG: bool = true;
    pub const LOOKBACK_LIMIT: usize = 300;
}

impl Display for DataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTimeframe(v) => write!(f, "invalid timeframe: {v}"),
            Self::InvalidDatetime(v) => write!(f, "invalid datetime: {v}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Csv(e) => write!(f, "csv error: {e}"),
            Self::Polars(e) => write!(f, "polars error: {e}"),
        }
    }
}

impl std::error::Error for DataError {}

impl From<std::io::Error> for DataError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<csv::Error> for DataError {
    fn from(value: csv::Error) -> Self {
        Self::Csv(value)
    }
}

impl From<polars::error::PolarsError> for DataError {
    fn from(value: polars::error::PolarsError) -> Self {
        Self::Polars(value)
    }
}
