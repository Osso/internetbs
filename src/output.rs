#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn print_response(json: bool, value: &serde_json::Value) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        print_value(value, 0, true);
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn print_value(value: &serde_json::Value, indent: usize, skip_transactid: bool) {
    match value {
        serde_json::Value::Object(map) => print_object(map, indent, skip_transactid),
        serde_json::Value::Array(arr) => print_array(arr, indent),
        _ => println!("{}{}", " ".repeat(indent), format_value(value)),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn print_object(
    map: &serde_json::Map<String, serde_json::Value>,
    indent: usize,
    skip_transactid: bool,
) {
    let prefix = " ".repeat(indent);
    for (k, v) in map {
        if skip_transactid && k == "transactid" {
            continue;
        }
        if v.is_object() || v.is_array() {
            println!("{}{}:", prefix, k);
            print_value(v, indent + 2, false);
        } else {
            println!("{}{}: {}", prefix, k, format_value(v));
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn print_array(arr: &[serde_json::Value], indent: usize) {
    let prefix = " ".repeat(indent);
    for (i, item) in arr.iter().enumerate() {
        println!("{}[{}]", prefix, i);
        print_value(item, indent + 2, false);
    }
}

pub(crate) fn format_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => v.to_string(),
    }
}
