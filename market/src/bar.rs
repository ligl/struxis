//! 行情数据结构定义模块。
//!
//! - `Bar`：`struxis::SBar` 的类型别名，market 内部与下游分发统一 bar。
//! - `BrokerBar`：`struxis::SBar` 的类型别名，作为 broker->market 统一 bar。
//! - `SharedBar`：跨线程分发时的共享引用类型。

use std::sync::Arc;

use struxis::SBar;

/// market 内部统一使用的 K 线结构（与 `struxis::SBar` 完全一致）。
pub type Bar = SBar;

/// broker 层输入到 market 的 K 线结构（与 `struxis::SBar` 完全一致）。
pub type BrokerBar = SBar;

/// 分发通道与队列中共享的 bar 指针类型。
pub type SharedBar = Arc<Bar>;
