use struxis::{Direction, KeyZoneBehavior, KeyZoneSignal};

#[test]
fn signed_strength_respects_direction_and_behavior() {
    let signal = KeyZoneSignal {
        zone_id: Some(1),
        behavior: KeyZoneBehavior::StrongAccept,
        direction: Direction::Up,
        strength: 0.8,
        sbar_id: 10,
    };
    assert!(signal.signed_strength() > 0.0);

    let signal = KeyZoneSignal {
        zone_id: Some(2),
        behavior: KeyZoneBehavior::StrongReject,
        direction: Direction::Up,
        strength: 0.8,
        sbar_id: 11,
    };
    assert!(signal.signed_strength() < 0.0);
}
