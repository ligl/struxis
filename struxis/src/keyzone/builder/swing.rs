use crate::constant::{KeyZoneOrientation, KeyZoneOrigin, Timeframe};
use crate::keyzone::builder::KeyZoneBuilder;
use crate::keyzone::KeyZone;
use crate::mtc::MultiTimeframeContext;

pub struct SwingKeyZoneBuilder;

impl KeyZoneBuilder for SwingKeyZoneBuilder {
    fn origin_type(&self) -> KeyZoneOrigin {
        KeyZoneOrigin::Swing
    }

    fn build(&self, mtc: &MultiTimeframeContext, timeframe: Timeframe) -> Vec<KeyZone> {
        mtc.get_swing_window(timeframe, 5)
            .into_iter()
            .map(|swing| KeyZone {
                id: None,
                timeframe,
                origin_type: KeyZoneOrigin::Swing,
                orientation: KeyZoneOrientation::Horizontal,
                upper: swing.high_price,
                lower: swing.low_price,
                touch_count: 0,
                last_touch_id: None,
                sbar_start_id: swing.sbar_start_id,
                sbar_end_id: swing.sbar_end_id,
                direction_hint: swing.direction,
                reactions: vec![],
            })
            .collect()
    }
}
