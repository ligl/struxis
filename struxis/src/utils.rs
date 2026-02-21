pub(crate) fn approx_eq_f64(a: f64, b: f64) -> bool {
    (a - b).abs() <= f64::EPSILON
}

pub(crate) fn first_changed_id<T, FId, FEq>(
    previous: &[T],
    current: &[T],
    id_of: FId,
    equals: FEq,
    append_as_change: bool,
) -> Option<u64>
where
    FId: Fn(&T) -> Option<u64>,
    FEq: Fn(&T, &T) -> bool,
{
    let min_len = previous.len().min(current.len());
    for idx in 0..min_len {
        if !equals(&previous[idx], &current[idx]) {
            return id_of(&previous[idx]).or_else(|| id_of(&current[idx]));
        }
    }

    if previous.len() > current.len() {
        return previous.get(min_len).and_then(id_of);
    }

    if append_as_change && current.len() > previous.len() {
        return current.get(min_len).and_then(id_of);
    }

    None
}
