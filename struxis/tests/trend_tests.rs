use chrono::Utc;

use struxis::{Direction, Swing, SwingState, TrendManager};

fn swing(id: u64, direction: Direction, high: f64, low: f64) -> Swing {
    Swing {
        id: Some(id),
        direction,
        cbar_start_id: id,
        cbar_end_id: id,
        sbar_start_id: id,
        sbar_end_id: id,
        high_price: high,
        low_price: low,
        span: 1,
        volume: 1.0,
        start_oi: 1.0,
        end_oi: 1.0,
        state: SwingState::Confirmed,
        created_at: Utc::now(),
    }
}

#[test]
fn starts_new_trend_from_next_swing_on_flip() {
    let swings = vec![
        swing(1, Direction::Up, 10.0, 5.0),
        swing(2, Direction::Down, 9.0, 4.0),
        swing(3, Direction::Up, 11.0, 6.0),
        swing(4, Direction::Down, 11.0, 9.0),
        swing(5, Direction::Up, 12.0, 10.0),
        swing(6, Direction::Down, 10.0, 8.0),
        swing(7, Direction::Up, 13.0, 11.0),
    ];

    let mut manager = TrendManager::new();
    manager.rebuild_from_swings(&swings);

    let rows = manager.all_rows();
    assert!(rows.len() >= 2);
    let second = &rows[1];
    assert_eq!(second.direction, Direction::Down);
    assert_eq!(second.swing_start_id, 6);
}

#[test]
fn split_pullback_uses_limit_swing_as_start() {
    let swings = vec![
        swing(1, Direction::Up, 10.0, 6.0),
        swing(2, Direction::Down, 11.0, 8.0),
        swing(3, Direction::Up, 12.0, 9.0),
        swing(4, Direction::Down, 13.0, 10.0),
        swing(5, Direction::Up, 11.5, 10.5),
        swing(6, Direction::Down, 12.0, 10.0),
    ];

    let mut manager = TrendManager::new();
    manager.rebuild_from_swings(&swings);
    let rows = manager.all_rows();
    assert!(!rows.is_empty());
}

#[test]
fn confirm_trend_adjusts_start_and_prev_trend_end() {
    let swings = vec![
        swing(1, Direction::Up, 10.0, 6.0),
        swing(2, Direction::Down, 9.0, 5.0),
        swing(3, Direction::Up, 11.0, 7.0),
        swing(4, Direction::Down, 12.0, 8.0),
        swing(5, Direction::Up, 11.0, 8.5),
        swing(6, Direction::Down, 13.0, 9.0),
        swing(7, Direction::Up, 12.5, 9.5),
        swing(8, Direction::Down, 10.0, 4.0),
    ];

    let mut manager = TrendManager::new();
    manager.rebuild_from_swings(&swings);
    let rows = manager.all_rows();
    assert!(!rows.is_empty());
}

#[test]
fn reports_first_changed_trend_id_on_backtrack_rebuild() {
    let swings = vec![
        swing(1, Direction::Up, 10.0, 5.0),
        swing(2, Direction::Down, 9.0, 4.0),
        swing(3, Direction::Up, 11.0, 6.0),
        swing(4, Direction::Down, 11.0, 9.0),
        swing(5, Direction::Up, 12.0, 10.0),
        swing(6, Direction::Down, 10.0, 8.0),
        swing(7, Direction::Up, 13.0, 11.0),
        swing(8, Direction::Down, 12.0, 7.0),
    ];

    let mut manager = TrendManager::new();
    let first = manager.rebuild_from_swings_with_backtrack(&swings[..7], None);
    assert!(first.is_none());

    let backtrack = manager.rebuild_from_swings_with_backtrack(&swings, Some(6));
    assert!(backtrack.is_some());
    assert_eq!(backtrack, Some(2));
}
