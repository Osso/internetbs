use super::*;
use clap::CommandFactory;
use serde_json::json;

#[test]
fn client_uses_production_or_test_api_base() {
    let live = InternetBsClient::new("key".into(), "pass".into(), false);
    let test = InternetBsClient::new("key".into(), "pass".into(), true);

    assert_eq!(live.base_url, API_BASE);
    assert_eq!(test.base_url, TEST_API_BASE);
    assert_eq!(live.api_key, "key");
    assert_eq!(test.password, "pass");
}

#[test]
fn parse_product_accepts_domain_prices_and_defaults_missing_fields() {
    let product = json!({
        "name": ".com registration",
        "price": "10.00"
    });
    let entry = parse_product(&product).expect("product should parse");

    assert_eq!(entry.tld, ".com");
    assert_eq!(entry.price_type, "registration");
    assert_eq!(entry.price, "10.00");
    assert_eq!(entry.currency, "USD");
}

#[test]
fn parse_product_rejects_non_domain_restore_and_malformed_names() {
    assert!(parse_product(&json!({"name": "hosting registration"})).is_none());
    assert!(parse_product(&json!({"name": ".com restore"})).is_none());
    assert!(parse_product(&json!({"name": ".com"})).is_none());
    assert!(parse_product(&json!({})).is_none());
}

#[test]
fn matches_tld_filter_normalizes_leading_dot() {
    assert!(matches_tld_filter(".io", None));
    assert!(matches_tld_filter(".io", Some("io")));
    assert!(matches_tld_filter(".io", Some(".io")));
    assert!(!matches_tld_filter(".io", Some("com")));
}

#[test]
fn filter_pricelist_groups_price_types_and_applies_filter() {
    let response = json!({
        "product": [
            {"name": ".com registration", "price": "10", "currency": "USD"},
            {"name": ".com renewal", "price": "11", "currency": "USD"},
            {"name": ".com transfer", "price": "9", "currency": "USD"},
            {"name": ".net registration", "price": "8", "currency": "EUR"},
            {"name": ".com restore", "price": "99", "currency": "USD"},
            {"name": "hosting registration", "price": "5", "currency": "USD"}
        ]
    });

    let all = filter_pricelist(&response, None);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].tld, ".com");
    assert_eq!(all[0].registration, "10");
    assert_eq!(all[0].renewal, "11");
    assert_eq!(all[0].transfer, "9");
    assert_eq!(all[0].currency, "USD");
    assert_eq!(all[1].tld, ".net");
    assert_eq!(all[1].registration, "8");
    assert_eq!(all[1].renewal, "-");
    assert_eq!(all[1].transfer, "-");
    assert_eq!(all[1].currency, "EUR");

    let filtered = filter_pricelist(&response, Some("net"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tld, ".net");
}

#[test]
fn filter_pricelist_tolerates_missing_or_non_array_product() {
    assert!(filter_pricelist(&json!({}), None).is_empty());
    assert!(filter_pricelist(&json!({"product": {"name": ".com registration"}}), None).is_empty());
}

#[test]
fn max_field_width_respects_minimum_and_largest_width() {
    assert_eq!(max_field_width([1, 7, 3].into_iter(), 4), 7);
    assert_eq!(max_field_width([1, 2].into_iter(), 4), 4);
}

#[test]
fn format_value_renders_scalars_and_compound_json() {
    assert_eq!(format_value(&json!("hello")), "hello");
    assert_eq!(format_value(&json!(42)), "42");
    assert_eq!(format_value(&json!(true)), "true");
    assert_eq!(format_value(&serde_json::Value::Null), "null");
    assert_eq!(format_value(&json!({"a": 1})), "{\"a\":1}");
    assert_eq!(format_value(&json!(["x"])), "[\"x\"]");
}

#[test]
fn clap_definition_is_valid() {
    Cli::command().debug_assert();
}
