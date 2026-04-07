use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const API_BASE: &str = "https://api.internet.bs";
const TEST_API_BASE: &str = "https://testapi.internet.bs";

macro_rules! params {
    ($($key:expr => $val:expr),* $(,)?) => {{
        let mut map = HashMap::new();
        $(map.insert($key.to_string(), $val.to_string());)*
        map
    }};
}

#[derive(Parser)]
#[command(name = "internetbs", about = "InternetBS domain registrar CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Use test API
    #[arg(long, global = true)]
    test: bool,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure API credentials
    Config {
        /// API key
        #[arg(long)]
        api_key: Option<String>,
        /// API password
        #[arg(long)]
        password: Option<String>,
    },
    /// Domain operations
    Domain {
        #[command(subcommand)]
        action: DomainAction,
    },
    /// DNS record operations
    Dns {
        #[command(subcommand)]
        action: DnsAction,
    },
}

#[derive(Subcommand)]
enum DomainAction {
    /// Check domain availability
    Check {
        /// Domain name to check
        domain: String,
    },
    /// Get domain info
    Info {
        /// Domain name
        domain: String,
    },
    /// List all domains
    List {
        /// Filter by expiring within N days
        #[arg(long)]
        expiring: Option<u32>,
        /// Filter by search term
        #[arg(long, short = 's')]
        search: Option<String>,
        /// Show detailed info (expiration, status, lock)
        #[arg(long, short = 'd')]
        detailed: bool,
    },
    /// Register a new domain
    Create {
        /// Domain name to register
        domain: String,
        /// Registration period in years
        #[arg(long, default_value = "1")]
        period: u32,
        /// Clone contacts from existing domain (e.g., ossonet.com)
        #[arg(long)]
        clone_from: String,
        /// Nameservers (comma-separated, defaults to topdns)
        #[arg(long)]
        ns: Option<String>,
        /// Enable private whois
        #[arg(long)]
        private_whois: bool,
    },
    /// Renew a domain
    Renew {
        /// Domain name
        domain: String,
        /// Renewal period in years
        #[arg(long, default_value = "1")]
        period: u32,
    },
    /// Update domain settings
    Update {
        /// Domain name
        domain: String,
        /// New nameservers (comma-separated)
        #[arg(long)]
        ns: Option<String>,
        /// Enable/disable private whois
        #[arg(long)]
        private_whois: Option<bool>,
        /// Enable/disable registrar lock
        #[arg(long)]
        registrar_lock: Option<bool>,
    },
    /// Get domain pricing
    Price {
        /// TLD filter (e.g., ".io", ".com") - omit to list all
        tld: Option<String>,
    },
}

#[derive(Subcommand)]
enum DnsAction {
    /// List DNS records
    List {
        /// Domain name
        domain: String,
        /// Filter by record type (A, AAAA, CNAME, MX, TXT, NS)
        #[arg(long, short = 't')]
        record_type: Option<String>,
    },
    /// Add a DNS record
    Add {
        /// Full record name (e.g., www.example.com)
        name: String,
        /// Record type (A, AAAA, CNAME, MX, TXT, NS, SRV)
        #[arg(long, short = 't')]
        record_type: String,
        /// Record value
        value: String,
        /// TTL in seconds
        #[arg(long, default_value = "3600")]
        ttl: u32,
        /// Priority (for MX/SRV records)
        #[arg(long)]
        priority: Option<u32>,
    },
    /// Update a DNS record
    Update {
        /// Full record name (e.g., www.example.com)
        name: String,
        /// Record type
        #[arg(long, short = 't')]
        record_type: String,
        /// Current value
        current_value: String,
        /// New value
        new_value: String,
        /// TTL in seconds
        #[arg(long)]
        ttl: Option<u32>,
        /// Priority (for MX/SRV records)
        #[arg(long)]
        priority: Option<u32>,
    },
    /// Remove a DNS record
    Remove {
        /// Full record name (e.g., www.example.com)
        name: String,
        /// Record type
        #[arg(long, short = 't')]
        record_type: String,
        /// Record value to remove
        value: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    api_key: String,
    password: String,
}

struct InternetBsClient {
    client: Client,
    api_key: String,
    password: String,
    base_url: String,
}

impl InternetBsClient {
    fn new(api_key: String, password: String, test_mode: bool) -> Self {
        let base_url = if test_mode {
            TEST_API_BASE.to_string()
        } else {
            API_BASE.to_string()
        };
        Self {
            client: Client::new(),
            api_key,
            password,
            base_url,
        }
    }

    async fn request(
        &self,
        endpoint: &str,
        mut params: HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        params.insert("apiKey".to_string(), self.api_key.clone());
        params.insert("password".to_string(), self.password.clone());
        params.insert("ResponseFormat".to_string(), "JSON".to_string());

        let url = format!("{}/{}", self.base_url, endpoint);
        let resp = self
            .client
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("Failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.context("Failed to read response")?;

        if !status.is_success() {
            bail!("API error ({}): {}", status, text);
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).context("Failed to parse JSON response")?;

        if let Some(api_status) = json.get("status").and_then(|s| s.as_str())
            && api_status == "FAILURE"
        {
            let message = json
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("API error: {}", message);
        }

        Ok(json)
    }

    async fn domain_check(&self, domain: &str) -> Result<serde_json::Value> {
        self.request("Domain/Check", params!("Domain" => domain))
            .await
    }

    async fn domain_info(&self, domain: &str) -> Result<serde_json::Value> {
        self.request("Domain/Info", params!("Domain" => domain))
            .await
    }

    async fn domain_list(
        &self,
        expiring_days: Option<u32>,
        search: Option<&str>,
        detailed: bool,
    ) -> Result<serde_json::Value> {
        let mut params = HashMap::new();
        if detailed {
            params.insert("CompactList".to_string(), "no".to_string());
        }
        if let Some(days) = expiring_days {
            params.insert("ExpiringOnly".to_string(), days.to_string());
        }
        if let Some(term) = search {
            params.insert("searchTermFilter".to_string(), term.to_string());
        }
        self.request("Domain/List", params).await
    }

    async fn domain_create(
        &self,
        domain: &str,
        period: u32,
        clone_from: &str,
        nameservers: Option<&str>,
        private_whois: bool,
    ) -> Result<serde_json::Value> {
        let ns = nameservers.unwrap_or("ns-canada.topdns.com,ns-uk.topdns.com,ns-usa.topdns.com");
        let mut params = params!(
            "Domain" => domain,
            "Period" => format!("{}Y", period),
            "CloneContactsFromDomain" => clone_from,
            "Ns_list" => ns,
        );
        if private_whois {
            params.insert("privateWhois".to_string(), "FULL".to_string());
        }
        self.request("Domain/Create", params).await
    }

    async fn domain_renew(&self, domain: &str, period: u32) -> Result<serde_json::Value> {
        self.request(
            "Domain/Renew",
            params!("Domain" => domain, "Period" => format!("{}Y", period)),
        )
        .await
    }

    async fn domain_update(
        &self,
        domain: &str,
        nameservers: Option<&str>,
        private_whois: Option<bool>,
        registrar_lock: Option<bool>,
    ) -> Result<serde_json::Value> {
        let mut params = params!("Domain" => domain);
        if let Some(ns) = nameservers {
            params.insert("Ns_list".to_string(), ns.to_string());
        }
        if let Some(private) = private_whois {
            params.insert(
                "privateWhois".to_string(),
                if private { "FULL" } else { "DISABLED" }.to_string(),
            );
        }
        if let Some(lock) = registrar_lock {
            params.insert(
                "registrarLock".to_string(),
                if lock { "ENABLED" } else { "DISABLED" }.to_string(),
            );
        }
        self.request("Domain/Update", params).await
    }

    async fn account_pricelist(&self) -> Result<serde_json::Value> {
        self.request("Account/PriceList/Get", HashMap::new()).await
    }

    async fn dns_list(&self, domain: &str, record_type: Option<&str>) -> Result<serde_json::Value> {
        let mut params = params!("Domain" => domain);
        if let Some(rt) = record_type {
            params.insert("FilterType".to_string(), rt.to_uppercase());
        }
        self.request("Domain/DnsRecord/List", params).await
    }

    async fn dns_add(
        &self,
        name: &str,
        record_type: &str,
        value: &str,
        ttl: u32,
        priority: Option<u32>,
    ) -> Result<serde_json::Value> {
        let mut params = params!(
            "FullRecordName" => name,
            "Type" => record_type.to_uppercase(),
            "Value" => value,
            "Ttl" => ttl,
        );
        if let Some(prio) = priority {
            params.insert("Priority".to_string(), prio.to_string());
        }
        self.request("Domain/DnsRecord/Add", params).await
    }

    async fn dns_update(
        &self,
        name: &str,
        record_type: &str,
        current_value: &str,
        new_value: &str,
        ttl: Option<u32>,
        priority: Option<u32>,
    ) -> Result<serde_json::Value> {
        let mut params = params!(
            "FullRecordName" => name,
            "Type" => record_type.to_uppercase(),
            "CurrentValue" => current_value,
            "NewValue" => new_value,
        );
        if let Some(t) = ttl {
            params.insert("Ttl".to_string(), t.to_string());
        }
        if let Some(prio) = priority {
            params.insert("Priority".to_string(), prio.to_string());
        }
        self.request("Domain/DnsRecord/Update", params).await
    }

    async fn dns_remove(
        &self,
        name: &str,
        record_type: &str,
        value: &str,
    ) -> Result<serde_json::Value> {
        self.request(
            "Domain/DnsRecord/Remove",
            params!(
                "FullRecordName" => name,
                "Type" => record_type.to_uppercase(),
                "Value" => value,
            ),
        )
        .await
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("internetbs")
        .join("config.toml")
}

fn load_config() -> Result<Config> {
    let path = config_path();
    let content = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "Config not found at {:?}. Run 'internetbs config' first.",
            path
        )
    })?;
    toml::from_str(&content).context("Failed to parse config")
}

fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    println!("Config saved to {:?}", path);
    Ok(())
}

/// Represents a single TLD price entry
#[derive(Debug, Serialize)]
struct TldPrice {
    tld: String,
    registration: String,
    renewal: String,
    transfer: String,
    currency: String,
}

struct ProductEntry<'a> {
    tld: &'a str,
    price_type: &'a str,
    price: &'a str,
    currency: &'a str,
}

fn parse_product(product: &serde_json::Value) -> Option<ProductEntry<'_>> {
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

fn matches_tld_filter(tld: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else { return true };
    let normalized = if filter.starts_with('.') {
        filter.to_lowercase()
    } else {
        format!(".{}", filter.to_lowercase())
    };
    tld.to_lowercase() == normalized
}

fn filter_pricelist(response: &serde_json::Value, tld_filter: Option<&str>) -> Vec<TldPrice> {
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

fn max_field_width(widths: impl Iterator<Item = usize>, min: usize) -> usize {
    widths.max().unwrap_or(min).max(min)
}

fn print_pricelist(prices: &[TldPrice]) {
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

fn print_response(json: bool, value: &serde_json::Value) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        print_value(value, 0, true);
    }
}

fn print_value(value: &serde_json::Value, indent: usize, skip_transactid: bool) {
    match value {
        serde_json::Value::Object(map) => print_object(map, indent, skip_transactid),
        serde_json::Value::Array(arr) => print_array(arr, indent),
        _ => println!("{}{}", " ".repeat(indent), format_value(value)),
    }
}

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

fn print_array(arr: &[serde_json::Value], indent: usize) {
    let prefix = " ".repeat(indent);
    for (i, item) in arr.iter().enumerate() {
        println!("{}[{}]", prefix, i);
        print_value(item, indent + 2, false);
    }
}

fn format_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => v.to_string(),
    }
}

fn build_client(test_mode: bool) -> Result<InternetBsClient> {
    let config = load_config()?;
    Ok(InternetBsClient::new(
        config.api_key,
        config.password,
        test_mode,
    ))
}

fn show_current_config() -> Result<()> {
    match load_config() {
        Ok(c) => {
            println!("API Key: {}...", &c.api_key[..8.min(c.api_key.len())]);
            println!("Password: ****");
            println!("Config path: {:?}", config_path());
        }
        Err(_) => {
            println!("No config found. Set with:");
            println!("  internetbs config --api-key KEY --password PASS");
        }
    }
    Ok(())
}

async fn handle_config(api_key: Option<String>, password: Option<String>) -> Result<()> {
    let Some((key, pass)) = api_key.zip(password) else {
        return show_current_config();
    };
    save_config(&Config {
        api_key: key,
        password: pass,
    })
}

async fn handle_domain(client: &InternetBsClient, action: DomainAction, json: bool) -> Result<()> {
    let result = match action {
        DomainAction::Check { domain } => client.domain_check(&domain).await?,
        DomainAction::Info { domain } => client.domain_info(&domain).await?,
        DomainAction::List {
            expiring,
            search,
            detailed,
        } => {
            client
                .domain_list(expiring, search.as_deref(), detailed)
                .await?
        }
        DomainAction::Create {
            domain,
            period,
            clone_from,
            ns,
            private_whois,
        } => {
            client
                .domain_create(&domain, period, &clone_from, ns.as_deref(), private_whois)
                .await?
        }
        DomainAction::Renew { domain, period } => client.domain_renew(&domain, period).await?,
        DomainAction::Update {
            domain,
            ns,
            private_whois,
            registrar_lock,
        } => {
            client
                .domain_update(&domain, ns.as_deref(), private_whois, registrar_lock)
                .await?
        }
        DomainAction::Price { tld } => {
            let result = client.account_pricelist().await?;
            let filtered = filter_pricelist(&result, tld.as_deref());
            if json {
                println!("{}", serde_json::to_string_pretty(&filtered).unwrap());
            } else {
                print_pricelist(&filtered);
            }
            return Ok(());
        }
    };
    print_response(json, &result);
    Ok(())
}

async fn handle_dns(client: &InternetBsClient, action: DnsAction, json: bool) -> Result<()> {
    let result = match action {
        DnsAction::List {
            domain,
            record_type,
        } => client.dns_list(&domain, record_type.as_deref()).await?,
        DnsAction::Add {
            name,
            record_type,
            value,
            ttl,
            priority,
        } => {
            client
                .dns_add(&name, &record_type, &value, ttl, priority)
                .await?
        }
        DnsAction::Update {
            name,
            record_type,
            current_value,
            new_value,
            ttl,
            priority,
        } => {
            client
                .dns_update(
                    &name,
                    &record_type,
                    &current_value,
                    &new_value,
                    ttl,
                    priority,
                )
                .await?
        }
        DnsAction::Remove {
            name,
            record_type,
            value,
        } => client.dns_remove(&name, &record_type, &value).await?,
    };
    print_response(json, &result);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Config { api_key, password } => handle_config(api_key, password).await,
        Commands::Domain { action } => {
            let client = build_client(cli.test)?;
            handle_domain(&client, action, cli.json).await
        }
        Commands::Dns { action } => {
            let client = build_client(cli.test)?;
            handle_dns(&client, action, cli.json).await
        }
    }
}
