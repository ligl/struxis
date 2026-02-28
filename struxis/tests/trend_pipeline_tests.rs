use chrono::Utc;

use struxis::{Direction, Swing, SwingState, TrendManager};

#[test]
fn trend_direction_and_boundaries_are_traceable_from_swings() {
    let swings = controlled_swings_for_switch();
    let mut manager = TrendManager::new();
    manager.rebuild_from_swings(&swings);
    let trends = manager.all_rows();

    assert!(!trends.is_empty(), "trend sequence should not be empty");

    let swing_by_id = swings
        .iter()
        .filter_map(|x| x.id.map(|id| (id, x)))
        .collect::<std::collections::HashMap<_, _>>();

    for trend in trends {
        assert!(
            matches!(trend.direction, Direction::Up | Direction::Down),
            "trend direction must be directional"
        );
        assert!(
            trend.swing_start_id <= trend.swing_end_id,
            "trend swing boundaries must be ordered"
        );

        let start = swing_by_id
            .get(&trend.swing_start_id)
            .copied()
            .expect("trend start swing id must exist");
        let end = swing_by_id
            .get(&trend.swing_end_id)
            .copied()
            .expect("trend end swing id must exist");

        assert_eq!(
            trend.sbar_start_id, start.sbar_start_id,
            "trend start boundary should trace to start swing"
        );
        assert_eq!(
            trend.sbar_end_id, end.sbar_end_id,
            "trend end boundary should trace to end swing"
        );

        let segment = swings
            .iter()
            .filter(|x| {
                let id = x.id.unwrap_or_default();
                trend.swing_start_id <= id && id <= trend.swing_end_id
            })
            .collect::<Vec<_>>();
        assert!(
            !segment.is_empty(),
            "trend swing range should map to at least one swing"
        );

        let same_dir = segment
            .iter()
            .filter(|x| x.direction == trend.direction)
            .count();
        let opposite_dir = segment
            .iter()
            .filter(|x| x.direction == trend.direction.opposite())
            .count();
        assert!(
            same_dir >= opposite_dir,
            "trend direction should be dominant inside its swing range"
        );
    }
}

#[test]
fn completed_trend_switch_keeps_opposite_direction_and_contiguous_ids() {
    let swings = controlled_swings_for_switch();
    let mut manager = TrendManager::new();
    manager.rebuild_from_swings(&swings);

    let trends = manager.all_rows();
    assert!(trends.len() >= 2, "need multiple trends for switch validation");

    let mut checked = 0usize;
    for pair in trends.windows(2) {
        let left = &pair[0];
        let right = &pair[1];

        if left.state != SwingState::Confirmed {
            continue;
        }

        assert_eq!(
            right.direction,
            left.direction.opposite(),
            "new trend direction should flip after previous trend is completed"
        );
        assert_eq!(
            right.swing_start_id,
            left.swing_end_id + 1,
            "new trend should start from next swing after previous trend end"
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "need at least one completed->next trend pair to validate switch rule"
    );
}

fn controlled_swings_for_switch() -> Vec<Swing> {
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
            state: SwingState::Confirmed,
            created_at: Utc::now(),
        }
    }

    vec![
        swing(1, Direction::Up, 10.0, 5.0),
        swing(2, Direction::Down, 9.0, 4.0),
        swing(3, Direction::Up, 11.0, 6.0),
        swing(4, Direction::Down, 11.0, 9.0),
        swing(5, Direction::Up, 12.0, 10.0),
        swing(6, Direction::Down, 10.0, 8.0),
        swing(7, Direction::Up, 13.0, 11.0),
        swing(8, Direction::Down, 12.0, 7.0),
    ]
}