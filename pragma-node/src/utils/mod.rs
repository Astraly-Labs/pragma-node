pub use aws::PragmaSignerBuilder;
pub use conversion::{
    convert_via_quote, felt_from_decimal, format_bigdecimal_price, normalize_to_decimals,
};
pub use custom_extractors::json_extractor::JsonExtractor;
pub use custom_extractors::path_extractor::PathExtractor;
pub use signing::starkex::StarkexPrice;
pub use signing::typed_data::TypedData;
pub use signing::{assert_request_signature_is_valid, sign_data, typed_data};

use bigdecimal::num_bigint::ToBigInt;
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDateTime;
use deadpool_diesel::postgres::Pool;
use pragma_common::types::Network;
use pragma_entities::{Entry, FutureEntry};
use std::collections::HashMap;

use crate::infra::repositories::{
    entry_repository::MedianEntry, onchain_repository::entry::get_existing_pairs,
};

mod aws;
mod conversion;
mod custom_extractors;
mod signing;

const ONE_YEAR_IN_SECONDS: f64 = 3153600_f64;

/// Converts two currencies pairs to a new routed pair id.
///
/// e.g "btc/usd" and "eth/usd" to "btc/eth"
pub(crate) fn currency_pairs_to_routed_pair_id(base_pair: &str, quote_pair: &str) -> String {
    let (base, _) = pair_id_to_currency_pair(base_pair);
    let (quote, _) = pair_id_to_currency_pair(quote_pair);
    format!("{}/{}", base.to_uppercase(), quote.to_uppercase())
}

/// Converts a currency pair to a pair id.
///
/// e.g "btc" and "usd" to "BTC/USD"
pub(crate) fn currency_pair_to_pair_id(base: &str, quote: &str) -> String {
    format!("{}/{}", base.to_uppercase(), quote.to_uppercase())
}

/// Converts a pair_id to a currency pair.
///
/// e.g "BTC/USD" to ("BTC", "USD")
pub(crate) fn pair_id_to_currency_pair(pair_id: &str) -> (String, String) {
    let parts: Vec<&str> = pair_id.split('/').collect();
    (parts[0].to_string(), parts[1].to_string())
}

/// From a map of currencies and their decimals, returns the number of decimals for a given pair.
/// If the currency is not found in the map, the default value is 8.
pub(crate) fn get_decimals_for_pair(
    currencies: &HashMap<String, BigDecimal>,
    pair_id: &str,
) -> u32 {
    let (base, quote) = pair_id_to_currency_pair(pair_id);
    let base_decimals = match currencies.get(&base) {
        Some(decimals) => decimals.to_u32().unwrap_or_default(),
        None => 8,
    };
    let quote_decimals = match currencies.get(&quote) {
        Some(decimals) => decimals.to_u32().unwrap_or_default(),
        None => 8,
    };
    std::cmp::min(base_decimals, quote_decimals)
}

/// Returns the mid price between two prices.
pub fn get_mid_price(low: &BigDecimal, high: &BigDecimal) -> BigDecimal {
    (low + high) / BigDecimal::from(2)
}

/// Computes the median price and time from a list of entries.
/// The median price is computed as the median of the median prices of each entry.
/// The median time is computed as the median of the times of each entry.
/// The median is computed as the middle value of a sorted list of values.
/// If the list has an even number of values, the median is computed as the average of the two middle values.
/// If the list is empty, None is returned.
#[allow(dead_code)]
pub(crate) fn compute_median_price_and_time(
    entries: &mut [MedianEntry],
) -> Option<(BigDecimal, NaiveDateTime)> {
    if entries.is_empty() {
        return None;
    }

    // Compute median price
    entries.sort_by(|a, b| a.median_price.cmp(&b.median_price));
    let mid = entries.len() / 2;
    let median_price = if entries.len() % 2 == 0 {
        (&entries[mid - 1].median_price + &entries[mid].median_price) / BigDecimal::from(2)
    } else {
        entries[mid].median_price.clone()
    };

    let latest_time = entries.last().unwrap().time;

    Some((median_price, latest_time))
}

/// Given a pair and a network, returns if it exists in the
/// onchain database.
pub(crate) async fn is_onchain_existing_pair(pool: &Pool, pair: &String, network: Network) -> bool {
    let existings_pairs = get_existing_pairs(pool, &network)
        .await
        .expect("Couldn't get the existing pairs from the database.");

    existings_pairs.into_iter().any(|p| p.pair_id == *pair)
}

/// Computes the volatility from a list of entries.
/// The volatility is computed as the annualized standard deviation of the log returns.
/// The log returns are computed as the natural logarithm of the ratio between two consecutive median prices.
/// The annualized standard deviation is computed as the square root of the variance multiplied by 10^8.
pub(crate) fn compute_volatility(entries: &[MedianEntry]) -> f64 {
    if entries.len() < 2 {
        return 0.0;
    }
    let mut values = Vec::new();
    for i in 1..entries.len() {
        if entries[i].median_price.to_f64().unwrap_or(0.0) > 0.0
            && entries[i - 1].median_price.to_f64().unwrap() > 0.0
            && (entries[i].time - entries[i - 1].time).num_seconds() > 0
        {
            let log_return = (entries[i].median_price.to_f64().unwrap()
                / entries[i - 1].median_price.to_f64().unwrap())
            .ln()
            .powi(2);

            let time = (entries[i].time - entries[i - 1].time)
                .num_seconds()
                .to_f64()
                .unwrap()
                / ONE_YEAR_IN_SECONDS;

            values.push((log_return, time));
        }
    }

    let variance: f64 = values.iter().map(|v| v.0 / v.1).sum::<f64>() / values.len() as f64;
    variance.sqrt() * 10_f64.powi(8)
}

/// Converts a big decimal price to a hex string 0x prefixed.
pub(crate) fn big_decimal_price_to_hex(price: &BigDecimal) -> String {
    format!(
        "0x{}",
        price.to_bigint().unwrap_or_default().to_str_radix(16)
    )
}

/// Given a list of pairs, only return the ones that exists in the
/// database in separate lists.
/// TODO: handle future pairs?
/// A list of pairs can contains:
/// - Spot pairs: formatted as usual (e.g. "BTC/USD")
/// - Perpetual pairs: usual pair with a mark suffix (e.g. "BTC/USD:MARK").
pub(crate) async fn only_existing_pairs(
    pool: &Pool,
    pairs: Vec<String>,
) -> (
    Vec<String>, // spot pairs
    Vec<String>, // perpetual pairs
                 // TODO: future_pairs
) {
    let conn = pool.get().await.expect("Couldn't connect to the database.");

    let pairs = pairs
        .iter()
        .map(|pair| pair.to_uppercase().trim().to_string())
        .collect::<Vec<String>>();

    // Check spot entries
    let spot_pairs = pairs
        .iter()
        .filter(|pair| !pair.contains(':'))
        .map(|pair| pair.to_string())
        .collect::<Vec<String>>();
    let spot_pairs = conn
        .interact(move |conn| Entry::get_existing_pairs(conn, spot_pairs))
        .await
        .expect("Couldn't check if pair exists")
        .expect("Couldn't get table result");

    // Check perp entries
    let perp_pairs = pairs
        .iter()
        .filter(|pair| pair.contains(":MARK"))
        .map(|pair| pair.replace(":MARK", "").to_string())
        .collect::<Vec<String>>();

    let perp_pairs = conn
        .interact(move |conn| FutureEntry::get_existing_perp_pairs(conn, perp_pairs))
        .await
        .expect("Couldn't check if pair exists")
        .expect("Couldn't get table result")
        .into_iter()
        .collect::<Vec<String>>();

    (spot_pairs, perp_pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn new_entry(median_price: u32, timestamp: i64) -> MedianEntry {
        MedianEntry {
            time: DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc(),
            median_price: median_price.into(),
            num_sources: 5,
        }
    }

    #[test]
    fn test_compute_volatility_no_entries() {
        let entries = vec![];
        assert_eq!(compute_volatility(&entries), 0.0);
    }

    #[test]
    fn test_compute_volatility_simple() {
        let entries = vec![new_entry(100, 1640995200), new_entry(110, 1641081600)];

        let expected_log_return = (110_f64 / 100_f64).ln().powi(2);
        let expected_time = ((1641081600 - 1640995200) as f64) / ONE_YEAR_IN_SECONDS;
        let expected_variance = expected_log_return / expected_time;
        let expected_volatility = expected_variance.sqrt() * 10_f64.powi(8);
        let computed_volatility = compute_volatility(&entries);

        const EPSILON: f64 = 1e-6;
        assert!((computed_volatility - expected_volatility).abs() < EPSILON);
    }

    #[test]
    fn test_compute_volatility() {
        let entries = vec![
            new_entry(47686, 1640995200),
            new_entry(47345, 1641081600),
            new_entry(46458, 1641168000),
            new_entry(45897, 1641254400),
            new_entry(43569, 1641340800),
        ];
        assert_eq!(compute_volatility(&entries), 17264357.96367333);
    }

    #[test]
    fn test_compute_volatility_zero_price() {
        let entries = vec![
            new_entry(47686, 1640995200),
            new_entry(0, 1641081600),
            new_entry(46458, 1641168000),
        ];
        // TODO: Shall this really return NaN?
        assert!(f64::is_nan(compute_volatility(&entries)));
    }

    #[test]
    fn test_compute_volatility_constant_prices() {
        let entries = vec![
            new_entry(47686, 1640995200),
            new_entry(47686, 1641081600),
            new_entry(47686, 1641168000),
            new_entry(47686, 1641254400),
            new_entry(47686, 1641340800),
        ];
        assert_eq!(compute_volatility(&entries), 0.0);
    }

    #[test]
    fn test_compute_volatility_increasing_prices() {
        let entries = vec![
            new_entry(13569, 1640995200),
            new_entry(15897, 1641081600),
            new_entry(16458, 1641168000),
            new_entry(17345, 1641254400),
            new_entry(47686, 1641340800),
        ];
        assert_eq!(compute_volatility(&entries), 309805011.67283577);
    }

    #[test]
    fn test_compute_volatility_decreasing_prices() {
        let entries = vec![
            new_entry(27686, 1640995200),
            new_entry(27345, 1641081600),
            new_entry(26458, 1641168000),
            new_entry(25897, 1641254400),
            new_entry(23569, 1641340800),
        ];
        assert_eq!(compute_volatility(&entries), 31060897.84391914);
    }
}
