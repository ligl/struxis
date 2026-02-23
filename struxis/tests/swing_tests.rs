use chrono::Utc;

use struxis::{Direction, Swing, SwingState};

fn swing(id: u64, direction: Direction, cbar_start_id: u64, cbar_end_id: u64) -> Swing {
    Swing {
        id: Some(id),
        direction,
        cbar_start_id,
        cbar_end_id,
        sbar_start_id: cbar_start_id,
        sbar_end_id: cbar_end_id,
        high_price: 10.0,
        low_price: 5.0,
        span: 1,
        volume: 1.0,
        start_oi: 1.0,
        end_oi: 1.0,
        is_completed: false,
        state: SwingState::Forming,
        created_at: Utc::now(),
    }
}

#[test]
fn swing_distance_and_overlap_work() {
    let up = swing(1, Direction::Up, 1, 3);
    let down = swing(2, Direction::Down, 3, 5);

    assert!((up.distance() - 5.0).abs() < 1e-9);
    assert!(up.overlap(&down));
}
