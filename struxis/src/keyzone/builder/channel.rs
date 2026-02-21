use crate::constant::{Direction, KeyZoneOrientation, KeyZoneOrigin, Timeframe};
use crate::keyzone::builder::KeyZoneBuilder;
use crate::keyzone::KeyZone;
use crate::mtc::MultiTimeframeContext;

pub struct ChannelKeyZoneBuilder;

impl KeyZoneBuilder for ChannelKeyZoneBuilder {
    fn origin_type(&self) -> KeyZoneOrigin {
        KeyZoneOrigin::Channel
    }

    fn build(&self, _mtc: &MultiTimeframeContext, timeframe: Timeframe) -> Vec<KeyZone> {
        vec![KeyZone {
            id: None,
            timeframe,
            origin_type: KeyZoneOrigin::Channel,
            orientation: KeyZoneOrientation::Channel,
            upper: 0.0,
            lower: 0.0,
            touch_count: 0,
            last_touch_id: None,
            sbar_start_id: 0,
            sbar_end_id: 0,
            direction_hint: Direction::None,
            reactions: vec![],
        }]
    }
}
