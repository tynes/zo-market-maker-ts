use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// Represents a price+size pair, allowing conversion to wire format.
#[derive(Debug, Clone)]
pub struct QuoteSize {
    pub price: Decimal,
    pub size: Decimal,
}

impl QuoteSize {
    pub fn new(price: Decimal, size: Decimal) -> Self {
        Self { price, size }
    }

    /// The quote value (price * size).
    pub fn value(&self) -> Decimal {
        self.price * self.size
    }

    /// Convert to wire format scaled integers.
    pub fn to_wire(&self, price_decimals: u32, size_decimals: u32) -> (u64, u64) {
        let price_scale = Decimal::from(10u64.pow(price_decimals));
        let size_scale = Decimal::from(10u64.pow(size_decimals));

        let price_wire = (self.price * price_scale)
            .to_u64()
            .expect("price overflow");
        let size_wire = (self.size * size_scale)
            .to_u64()
            .expect("size overflow");

        (price_wire, size_wire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_value_basic() {
        let qs = QuoteSize::new(dec!(50000.00), dec!(0.5));
        assert_eq!(qs.value(), dec!(25000.00));
    }

    #[test]
    fn test_value_fractional() {
        let qs = QuoteSize::new(dec!(3250.75), dec!(1.2));
        assert_eq!(qs.value(), dec!(3900.900));
    }

    #[test]
    fn test_value_zero_size() {
        let qs = QuoteSize::new(dec!(100.00), dec!(0));
        assert_eq!(qs.value(), dec!(0));
    }

    #[test]
    fn test_value_zero_price() {
        let qs = QuoteSize::new(dec!(0), dec!(10));
        assert_eq!(qs.value(), dec!(0));
    }

    #[test]
    fn test_to_wire_btc_like() {
        // BTC-like market: price_decimals=2, size_decimals=4
        let qs = QuoteSize::new(dec!(50000.50), dec!(0.1234));
        let (price_wire, size_wire) = qs.to_wire(2, 4);
        // 50000.50 * 100 = 5_000_050
        assert_eq!(price_wire, 5_000_050);
        // 0.1234 * 10000 = 1234
        assert_eq!(size_wire, 1234);
    }

    #[test]
    fn test_to_wire_whole_numbers() {
        let qs = QuoteSize::new(dec!(100), dec!(10));
        let (price_wire, size_wire) = qs.to_wire(0, 0);
        assert_eq!(price_wire, 100);
        assert_eq!(size_wire, 10);
    }

    #[test]
    fn test_to_wire_high_decimals() {
        // 6 price decimals, 6 size decimals
        let qs = QuoteSize::new(dec!(1.123456), dec!(2.654321));
        let (price_wire, size_wire) = qs.to_wire(6, 6);
        assert_eq!(price_wire, 1_123_456);
        assert_eq!(size_wire, 2_654_321);
    }

    #[test]
    fn test_to_wire_zero() {
        let qs = QuoteSize::new(dec!(0), dec!(0));
        let (price_wire, size_wire) = qs.to_wire(8, 8);
        assert_eq!(price_wire, 0);
        assert_eq!(size_wire, 0);
    }
}
