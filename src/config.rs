use std::path::PathBuf;
use serde::{Deserialize};
use crate::{
    topic::CKBNetworkType
};



#[derive(Clone, Debug, Deserialize)]
pub struct CKBAnalyzerConfig {
    pub networks: Vec<CKBNetworkType>,
    pub db: DBConfig,
    pub witness_bound: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct RawCKBAnalyzerConfig {
    networks: Vec<String>,
    db: DBConfig,
    witness_bound: usize,
}

impl CKBAnalyzerConfig {
    pub fn new(networks : Vec<CKBNetworkType>, db : DBConfig, ipinfo_io_token: String, witness_bound: usize) -> Self {
        Self {
            networks,
            db,
            witness_bound,
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
            witness_bound: raw.witness_bound,
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
