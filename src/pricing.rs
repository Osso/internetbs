use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub(crate) struct TldPrice {
    pub(crate) tld: String,
    pub(crate) registration: String,
    pub(crate) renewal: String,
    pub(crate) transfer: String,
    pub(crate) currency: String,
}

pub(crate) struct ProductEntry<'a> {
    pub(crate) tld: &'a str,
    pub(crate) price_type: &'a str,
    pub(crate) price: &'a str,
    pub(crate) currency: &'a str,
}

pub(crate) fn parse_product(product: &serde_json::Value) -> Option<ProductEntry<'_>> {
    let name = product.get("name").and_then(|v| v.as_str())?;
    if !name.starts_with('.') {
        return None;
    }
    let (tld, price_type) = name.rsplit_once(' ')?;
    if price_type == "restore" {
        return None;
    }
    Some(ProductEntry {
        tld,
        price_type,
        price: product.get("price").and_then(|v| v.as_str()).unwrap_or("-"),
        currency: product
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("USD"),
    })
}

pub(crate) fn matches_tld_filter(tld: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else { return true };
    let normalized = if filter.starts_with('.') {
        filter.to_lowercase()
    } else {
        format!(".{}", filter.to_lowercase())
    };
    tld.to_lowercase() == normalized
}

pub(crate) fn filter_pricelist(
    response: &serde_json::Value,
    tld_filter: Option<&str>,
) -> Vec<TldPrice> {
    let empty = vec![];
    let products = response
        .get("product")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut tld_map: HashMap<String, TldPrice> = HashMap::new();
    for product in products {
        let Some(entry) = parse_product(product) else {
            continue;
        };
        if !matches_tld_filter(entry.tld, tld_filter) {
            continue;
        }
        let tld_entry = tld_map
            .entry(entry.tld.to_string())
            .or_insert_with(|| TldPrice {
                tld: entry.tld.to_string(),
                registration: "-".to_string(),
                renewal: "-".to_string(),
                transfer: "-".to_string(),
                currency: entry.currency.to_string(),
            });
        match entry.price_type {
            "registration" => tld_entry.registration = entry.price.to_string(),
            "renewal" => tld_entry.renewal = entry.price.to_string(),
            "transfer" => tld_entry.transfer = entry.price.to_string(),
            _ => {}
        }
    }

    let mut prices: Vec<TldPrice> = tld_map.into_values().collect();
    prices.sort_by(|a, b| a.tld.to_lowercase().cmp(&b.tld.to_lowercase()));
    prices
}

pub(crate) fn max_field_width(widths: impl Iterator<Item = usize>, min: usize) -> usize {
    widths.max().unwrap_or(min).max(min)
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn print_pricelist(prices: &[TldPrice]) {
    if prices.is_empty() {
        println!("No pricing data found.");
        return;
    }

    let tld_w = max_field_width(prices.iter().map(|p| p.tld.len()), 4);
    let reg_w = max_field_width(prices.iter().map(|p| p.registration.len()), 12);
    let ren_w = max_field_width(prices.iter().map(|p| p.renewal.len()), 7);
    let xfr_w = max_field_width(prices.iter().map(|p| p.transfer.len()), 8);

    println!(
        "{:<tld_w$}  {:>reg_w$}  {:>ren_w$}  {:>xfr_w$}",
        "TLD", "Registration", "Renewal", "Transfer"
    );
    println!(
        "{:<tld_w$}  {:>reg_w$}  {:>ren_w$}  {:>xfr_w$}",
        "-".repeat(tld_w),
        "-".repeat(reg_w),
        "-".repeat(ren_w),
        "-".repeat(xfr_w)
    );

    for price in prices {
        println!(
            "{:<tld_w$}  {:>reg_w$}  {:>ren_w$}  {:>xfr_w$}",
            price.tld, price.registration, price.renewal, price.transfer
        );
    }

    println!("\n{} TLD(s)", prices.len());
}
