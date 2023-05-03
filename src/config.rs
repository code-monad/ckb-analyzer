use std::path::PathBuf;
use serde::{Deserialize};
use crate::{
    topic::CKBNetworkType
};



#[derive(Clone, Debug, Deserialize)]
pub struct CKBAnalyzerConfig {
    pub networks: Vec<CKBNetworkType>,
    pub db: DBConfig,
    pub ipinfo_io_token: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawCKBAnalyzerConfig {
    networks: Vec<String>,
    db: DBConfig,
    ipinfo_io_token: String,
}

impl CKBAnalyzerConfig {
    pub fn new(networks : Vec<CKBNetworkType>, db : DBConfig, ipinfo_io_token: String) -> Self {
        Self {
            networks,
            db,
            ipinfo_io_token,
        }
    }

    pub fn from_file(f: PathBuf) -> Self {
        let raw_config: RawCKBAnalyzerConfig = toml::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        Self::from(raw_config)
    }
}

impl From<RawCKBAnalyzerConfig> for CKBAnalyzerConfig {
    fn from(raw: RawCKBAnalyzerConfig) -> Self {
        let networks = raw.networks.into_iter().map(|s| CKBNetworkType::from(s)).collect();
        Self {
            networks,
            db: raw.db,
            ipinfo_io_token: raw.ipinfo_io_token,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
// for postgresql connection
pub struct DBConfig {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) database: String,
    pub(crate) user: String,
    pub(crate) password: String,
}

impl DBConfig {
    pub fn new(host : String, port : u16, database : String, user : String, password : String) -> Self {
        Self {
            host,
            port,
            database,
            user,
            password,
        }
    }
}
