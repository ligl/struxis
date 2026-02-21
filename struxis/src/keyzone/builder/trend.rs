use crate::constant::{KeyZoneOrientation, KeyZoneOrigin, Timeframe};
use crate::keyzone::builder::KeyZoneBuilder;
use crate::keyzone::KeyZone;
use crate::mtc::MultiTimeframeContext;

pub struct TrendKeyZoneBuilder;

impl KeyZoneBuilder for TrendKeyZoneBuilder {
    fn origin_type(&self) -> KeyZoneOrigin {
        KeyZoneOrigin::Trend
    }

    fn build(&self, mtc: &MultiTimeframeContext, timeframe: Timeframe) -> Vec<KeyZone> {
        mtc.get_trend_window(timeframe, 5)
            .into_iter()
            .map(|trend| KeyZone {
                id: None,
                timeframe,
                origin_type: KeyZoneOrigin::Trend,
                orientation: KeyZoneOrientation::Horizontal,
                upper: trend.high_price,
                lower: trend.low_price,
                touch_count: 0,
                last_touch_id: None,
                sbar_start_id: trend.sbar_start_id,
                sbar_end_id: trend.sbar_end_id,
                direction_hint: trend.direction,
                reactions: vec![],
            })
            .collect()
    }
}
