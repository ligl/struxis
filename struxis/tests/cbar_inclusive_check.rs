#[cfg(test)]
mod tests {
    use crate::bar::CBar;
    use crate::cbar_manager::CBarManager;
    use crate::constant::Timeframe;

    #[test]
    fn cbar_no_inclusive_neighbors() {
        // 构造一组典型的SBar数据（可替换为实际数据来源）
        let mut mgr = CBarManager::new(Timeframe::M15);
        // ...这里应补充实际SBar输入...
        // let sbars = ...
        // for sbar in sbars { mgr.on_sbar(&sbar); }
        let cbars = mgr.all_rows();
        for pair in cbars.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            assert!(
                !left.is_inclusive(right),
                "cbar包含关系未消除: left({:?},{:?}) right({:?},{:?})",
                left.high_price, left.low_price, right.high_price, right.low_price
            );
        }
    }
}
