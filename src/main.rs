#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

mod output;
mod pricing;

#[cfg(test)]
use output::format_value;
use output::print_response;
use pricing::{filter_pricelist, print_pricelist};
#[cfg(test)]
use pricing::{matches_tld_filter, max_field_width, parse_product};

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
    Check {
        domain: String,
    },
    Info {
        domain: String,
    },
    List {
        #[arg(long)]
        expiring: Option<u32>,
        #[arg(long, short = 's')]
        search: Option<String>,
        #[arg(long, short = 'd')]
        detailed: bool,
    },
    Create {
        domain: String,
        #[arg(long, default_value = "1")]
        period: u32,
        #[arg(long)]
        clone_from: String,
        #[arg(long)]
        ns: Option<String>,
        #[arg(long)]
        private_whois: bool,
    },
    Renew {
        domain: String,
        #[arg(long, default_value = "1")]
        period: u32,
    },
    Update {
        domain: String,
        #[arg(long)]
        ns: Option<String>,
        #[arg(long)]
        private_whois: Option<bool>,
        #[arg(long)]
        registrar_lock: Option<bool>,
    },
    Price {
        tld: Option<String>,
    },
}

#[derive(Subcommand)]
enum DnsAction {
    List {
        domain: String,
        #[arg(long, short = 't')]
        record_type: Option<String>,
    },
    Add {
        name: String,
        #[arg(long, short = 't')]
        record_type: String,
        value: String,
        #[arg(long, default_value = "3600")]
        ttl: u32,
        #[arg(long)]
        priority: Option<u32>,
    },
    Update {
        name: String,
        #[arg(long, short = 't')]
        record_type: String,
        current_value: String,
        new_value: String,
        #[arg(long)]
        ttl: Option<u32>,
        #[arg(long)]
        priority: Option<u32>,
    },
    Remove {
        name: String,
        #[arg(long, short = 't')]
        record_type: String,
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn domain_check(&self, domain: &str) -> Result<serde_json::Value> {
        self.request("Domain/Check", params!("Domain" => domain))
            .await
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn domain_info(&self, domain: &str) -> Result<serde_json::Value> {
        self.request("Domain/Info", params!("Domain" => domain))
            .await
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn domain_renew(&self, domain: &str, period: u32) -> Result<serde_json::Value> {
        self.request(
            "Domain/Renew",
            params!("Domain" => domain, "Period" => format!("{}Y", period)),
        )
        .await
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn account_pricelist(&self) -> Result<serde_json::Value> {
        self.request("Account/PriceList/Get", HashMap::new()).await
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn dns_list(&self, domain: &str, record_type: Option<&str>) -> Result<serde_json::Value> {
        let mut params = params!("Domain" => domain);
        if let Some(rt) = record_type {
            params.insert("FilterType".to_string(), rt.to_uppercase());
        }
        self.request("Domain/DnsRecord/List", params).await
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg_attr(coverage_nightly, coverage(off))]
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("internetbs")
        .join("config.toml")
}

#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg_attr(coverage_nightly, coverage(off))]
fn build_client(test_mode: bool) -> Result<InternetBsClient> {
    let config = load_config()?;
    Ok(InternetBsClient::new(
        config.api_key,
        config.password,
        test_mode,
    ))
}

#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg_attr(coverage_nightly, coverage(off))]
async fn handle_config(api_key: Option<String>, password: Option<String>) -> Result<()> {
    let Some((key, pass)) = api_key.zip(password) else {
        return show_current_config();
    };
    save_config(&Config {
        api_key: key,
        password: pass,
    })
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn handle_price(client: &InternetBsClient, tld: Option<String>, json: bool) -> Result<()> {
    let result = client.account_pricelist().await?;
    let filtered = filter_pricelist(&result, tld.as_deref());
    if json {
        println!("{}", serde_json::to_string_pretty(&filtered).unwrap());
    } else {
        print_pricelist(&filtered);
    }
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn handle_domain(client: &InternetBsClient, action: DomainAction, json: bool) -> Result<()> {
    if let DomainAction::Price { tld } = action {
        return handle_price(client, tld, json).await;
    }
    let result = execute_domain_action(client, action).await?;
    print_response(json, &result);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn handle_dns(client: &InternetBsClient, action: DnsAction, json: bool) -> Result<()> {
    let result = execute_dns_action(client, action).await?;
    print_response(json, &result);
    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_domain_action(
    client: &InternetBsClient,
    action: DomainAction,
) -> Result<serde_json::Value> {
    match action {
        DomainAction::Check { .. } | DomainAction::Info { .. } | DomainAction::Renew { .. } => {
            execute_basic_domain_action(client, action).await
        }
        DomainAction::List { .. } | DomainAction::Create { .. } | DomainAction::Update { .. } => {
            execute_extended_domain_action(client, action).await
        }
        DomainAction::Price { .. } => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_basic_domain_action(
    client: &InternetBsClient,
    action: DomainAction,
) -> Result<serde_json::Value> {
    match action {
        DomainAction::Check { domain } => client.domain_check(&domain).await,
        DomainAction::Info { domain } => client.domain_info(&domain).await,
        DomainAction::Renew { domain, period } => client.domain_renew(&domain, period).await,
        _ => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_extended_domain_action(
    client: &InternetBsClient,
    action: DomainAction,
) -> Result<serde_json::Value> {
    match action {
        DomainAction::List {
            expiring,
            search,
            detailed,
        } => execute_domain_list(client, expiring, search.as_deref(), detailed).await,
        DomainAction::Create { .. } | DomainAction::Update { .. } => {
            execute_domain_write_action(client, action).await
        }
        _ => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_domain_list(
    client: &InternetBsClient,
    expiring: Option<u32>,
    search: Option<&str>,
    detailed: bool,
) -> Result<serde_json::Value> {
    client.domain_list(expiring, search, detailed).await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_domain_write_action(
    client: &InternetBsClient,
    action: DomainAction,
) -> Result<serde_json::Value> {
    match action {
        DomainAction::Create {
            domain,
            period,
            clone_from,
            ns,
            private_whois,
        } => {
            execute_domain_create(
                client,
                &domain,
                period,
                &clone_from,
                ns.as_deref(),
                private_whois,
            )
            .await
        }
        DomainAction::Update {
            domain,
            ns,
            private_whois,
            registrar_lock,
        } => {
            execute_domain_update(
                client,
                &domain,
                ns.as_deref(),
                private_whois,
                registrar_lock,
            )
            .await
        }
        _ => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_domain_create(
    client: &InternetBsClient,
    domain: &str,
    period: u32,
    clone_from: &str,
    nameservers: Option<&str>,
    private_whois: bool,
) -> Result<serde_json::Value> {
    client
        .domain_create(domain, period, clone_from, nameservers, private_whois)
        .await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_domain_update(
    client: &InternetBsClient,
    domain: &str,
    nameservers: Option<&str>,
    private_whois: Option<bool>,
    registrar_lock: Option<bool>,
) -> Result<serde_json::Value> {
    client
        .domain_update(domain, nameservers, private_whois, registrar_lock)
        .await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_dns_action(
    client: &InternetBsClient,
    action: DnsAction,
) -> Result<serde_json::Value> {
    match action {
        DnsAction::Update { .. } => execute_dns_update_action(client, action).await,
        _ => execute_basic_dns_action(client, action).await,
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_basic_dns_action(
    client: &InternetBsClient,
    action: DnsAction,
) -> Result<serde_json::Value> {
    match action {
        DnsAction::List {
            domain,
            record_type,
        } => client.dns_list(&domain, record_type.as_deref()).await,
        DnsAction::Add {
            name,
            record_type,
            value,
            ttl,
            priority,
        } => execute_dns_add(client, &name, &record_type, &value, ttl, priority).await,
        DnsAction::Remove {
            name,
            record_type,
            value,
        } => execute_dns_remove(client, &name, &record_type, &value).await,
        _ => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_dns_update_action(
    client: &InternetBsClient,
    action: DnsAction,
) -> Result<serde_json::Value> {
    match action {
        DnsAction::Update {
            name,
            record_type,
            current_value,
            new_value,
            ttl,
            priority,
        } => {
            execute_dns_update(
                client,
                &name,
                &record_type,
                &current_value,
                &new_value,
                ttl,
                priority,
            )
            .await
        }
        _ => unreachable!(),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_dns_add(
    client: &InternetBsClient,
    name: &str,
    record_type: &str,
    value: &str,
    ttl: u32,
    priority: Option<u32>,
) -> Result<serde_json::Value> {
    client
        .dns_add(name, record_type, value, ttl, priority)
        .await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_dns_update(
    client: &InternetBsClient,
    name: &str,
    record_type: &str,
    current_value: &str,
    new_value: &str,
    ttl: Option<u32>,
    priority: Option<u32>,
) -> Result<serde_json::Value> {
    client
        .dns_update(name, record_type, current_value, new_value, ttl, priority)
        .await
}

#[cfg_attr(coverage_nightly, coverage(off))]
async fn execute_dns_remove(
    client: &InternetBsClient,
    name: &str,
    record_type: &str,
    value: &str,
) -> Result<serde_json::Value> {
    client.dns_remove(name, record_type, value).await
}

#[tokio::main]
#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg(test)]
mod tests;
