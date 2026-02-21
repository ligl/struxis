use market::BrokerBar;

use crate::error::BrokerError;

pub trait ExchangeFeed {
    fn next_bar(&mut self) -> Option<BrokerBar>;
}

pub trait ExchangeAdapter {
    fn venue(&self) -> &str;
    fn connect(&mut self) -> Result<(), BrokerError>;
    fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError>;

    fn subscribe_symbol(&mut self, _symbol: &str) -> Result<(), BrokerError> {
        Ok(())
    }

    fn heartbeat(&mut self) -> Result<(), BrokerError> {
        Ok(())
    }
}
