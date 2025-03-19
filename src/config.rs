use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Configuration for the TON proxy
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// List of TON domains to handle specially
    pub ton_domains: Vec<String>,
    
    /// Default TON gateway to use for TON sites
    pub ton_gateway: String,
    
    /// Whether to log detailed request information
    pub verbose_logging: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ton_domains: vec![
                "ton".to_string(),
                "t.me".to_string(),
            ],
            ton_gateway: "https://gateway.ton.org".to_string(),
            verbose_logging: false,
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config = serde_json::from_str(&contents)?;
        Ok(config)
    }
    
    /// Save configuration to a file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
    
    /// Check if a domain is a TON domain
    pub fn is_ton_domain(&self, domain: &str) -> bool {
        self.ton_domains.iter().any(|d| domain.ends_with(d))
    }
}