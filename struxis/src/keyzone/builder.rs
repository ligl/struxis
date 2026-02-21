mod channel;
mod swing;
mod trend;

pub use channel::ChannelKeyZoneBuilder;
pub use swing::SwingKeyZoneBuilder;
pub use trend::TrendKeyZoneBuilder;

use crate::constant::{KeyZoneOrigin, Timeframe};
use crate::keyzone::KeyZone;
use crate::mtc::MultiTimeframeContext;

pub trait KeyZoneBuilder {
    fn origin_type(&self) -> KeyZoneOrigin;
    fn build(&self, mtc: &MultiTimeframeContext, timeframe: Timeframe) -> Vec<KeyZone>;
}
