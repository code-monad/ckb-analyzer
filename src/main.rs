use crate::topic::{CellCrawler, ChainCrawler, ChainTransactionCrawler, CKBNetworkType, CompactBlockCrawler, EpochCrawler, NetworkCrawler, PoolCrawler, RetentionTransactionCrawler, SubscribeNewTransaction, SubscribeProposedTransaction, SubscribeRejectedTransaction};
use crate::util::crossbeam_channel_to_tokio_channel;
use ckb_testkit::{connector::SharedState, ConnectorBuilder, Node};
use clap::{crate_version, values_t_or_exit, App, Arg};
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub use ckb_testkit::ckb_jsonrpc_types;
pub use ckb_testkit::ckb_types;

mod entry;
mod topic;
mod util;
mod config;

use config::CKBAnalyzerConfig;

#[tokio::main]
async fn main() {
    let _logger_guard = init_logger();
    log::info!("CKBAnalyzer starting");
    let matches = clap_app().get_matches();
    let topics = values_t_or_exit!(matches, "topics", String);
    let networks = values_t_or_exit!(matches, "network", String);
    //log::info!("Topics: {:?}", topics);
    log::info!("Networks: {:?}", networks);
    let config = if matches.is_present("config") {
        Some(CKBAnalyzerConfig::from_file(matches.value_of("config").unwrap().into()))
    } else {
        None
    };
    log::info!("Config: {:?}", config);

    let pg_config = match config {
        Some(config) => {
            let host = config.db.host;
            let port = config.db.port;
            let database = config.db.database;
            let user = config.db.user;
            let password = config.db.password;

            // change setting
            if !config.ipinfo_io_token.is_empty() {
                std::env::set_var("IPINFO_IO_TOKEN", config.ipinfo_io_token)
            }
            let mut config = tokio_postgres::Config::new();
            config
                .host(&host)
                .port(port)
                .dbname(&database)
                .user(&user)
                .password(&password)
                .application_name("CKBAnalyzer");
            config
        },

        // Use env var if no config presents
        None => {
            let host = env::var_os("PGHOST")
                .or_else(|| env::var_os("POSTGRES_HOST"))
                .expect("requires environment variable \"PGHOST\" or \"POSTGRES_HOST\"")
                .to_string_lossy()
                .to_string();
            let port = env::var_os("PGPORT")
                .or_else(|| env::var_os("POSTGRES_PORT"))
                .expect("requires environment variable \"PGPORT\" or \"POSTGRES_PORT\"")
                .to_string_lossy()
                .parse::<u16>()
                .expect("requires environment variable \"PGPORT\" or \"POSTGRES_PORT\" to be a number");
            let database = env::var_os("PGDATABASE")
                .or_else(|| env::var_os("POSTGRES_DB"))
                .expect("requires environment variable \"PGDATABASE\" or \"POSTGRES_DB\"")
                .to_string_lossy()
                .to_string();
            let user = env::var_os("PGUSER")
                .or_else(|| env::var_os("POSTGRES_USER"))
                .expect("requires environment variable \"PGUSER\" or \"POSTGRES_USER\"")
                .to_string_lossy()
                .to_string();
            let password = env::var_os("PGPASSWORD")
                .or_else(|| env::var_os("POSTGRES_PASSWORD"))
                .expect("requires environment variable \"PGPASSWORD\" or \"POSTGRES_PASSWORD\"")
                .to_string_lossy()
                .to_string();

            let mut config = tokio_postgres::Config::new();
            config
                .host(&host)
                .port(port)
                .dbname(&database)
                .user(&user)
                .password(&password)
                .application_name("CKBAnalyzer");
            config
        }
    };

    let pg = {
        log::info!("Connecting to Postgres, {:?}", pg_config);
        let (pg, conn) = pg_config.connect(tokio_postgres::NoTls).await.expect("Failed to connect to Postgres");
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                log::error!("postgres connection error: {}", err);
            }
        });
        pg
    };

    // start handlers
    let (query_sender, mut query_receiver) =
        crossbeam_channel_to_tokio_channel::channel::<String>(5000);
    let network_types = networks.into_iter().map(|x| CKBNetworkType::from(x)).collect::<Vec<CKBNetworkType>>();
    let mut _connectors = Vec::new();
    for topic in topics {
        match topic.as_str() {
            "NetworkCrawler" => {
                for network in network_types.iter() {
                    log::info!("Start listening {:?}", network);
                    let shared = Arc::new(RwLock::new(SharedState::new()));
                    let network_crawler =
                        NetworkCrawler::new(network.clone(), query_sender.clone(), Arc::clone(&shared));
                    // workaround for Rust lifetime
                    _connectors.push(
                        ConnectorBuilder::new()
                            .protocol_metas(network_crawler.build_protocol_metas())
                            .listening_addresses(vec![])
                            .build(network_crawler, shared),
                    );
                }
            }
            _ => {
                ckb_testkit::error!("Unknown topic \"{}\"", topic);
                unreachable!()
            }
        }
    }

    // loop listen and batch execute queries
    let max_batch_size: usize = 200;
    let max_batch_timeout = Duration::from_secs(3);
    let mut batch: Vec<String> = Vec::with_capacity(max_batch_size);
    let mut last_batch_instant = Instant::now();
    while let Some(query) = query_receiver.recv().await {
        log::debug!("new query: {}", query);
        batch.push(query);

        if batch.len() >= max_batch_size || last_batch_instant.elapsed() >= max_batch_timeout {
            log::debug!("batch_execute {} queries", batch.len());

            let batch_query: String = batch.join(";");
            pg.batch_execute(&batch_query).await.unwrap_or_else(|err| {
                log::error!("batch_execute(\"{}\"), error: {}", batch_query, err)
            });

            last_batch_instant = Instant::now();
            batch = Vec::new();
        }
    }
    log::info!("CKBAnalyzer shutdown");
}

fn init_logger() -> ckb_logger_service::LoggerInitGuard {
    let filter = match env::var("RUST_LOG") {
        Ok(filter) if filter.is_empty() => Some("info".to_string()),
        Ok(filter) => Some(filter),
        Err(_) => Some("info".to_string()),
    };
    let config = ckb_logger_config::Config {
        filter,
        color: false,
        log_to_file: false,
        log_to_stdout: true,
        ..Default::default()
    };
    ckb_logger_service::init(None, config)
        .unwrap_or_else(|err| panic!("failed to init the logger service, error: {}", err))
}

pub fn clap_app() -> App<'static, 'static> {
    App::new("ckb-analyzer")
        .version(crate_version!())
        .arg(
            Arg::with_name("config")
                .long("config")
                .value_name("CONFIG")
                .required(false)
                .takes_value(true)
                .default_value("ckb-analyzer.toml"),
        )
        .arg(
            Arg::with_name("network")
                .long("ckb-network")
                .value_name("NETWORK")
                .required(false)
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .default_value(
                    "mirana,pudge"
                )
                .possible_values(&[
                    "mirana", "main", // main net
                    "pudge", "test", // test net
                    "dev", // dev net
                    // Below are for legacy compatibility
                    "ckb",
                    "ckb_testnet"
                ]),
        )
        .arg(
            Arg::with_name("topics")
                .long("topics")
                .value_name("TOPIC")
                .required(false)
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .default_value(
                    "NetworkCrawler",
                )
                .possible_values(&[
                    "ChainCrawler",
                    "EpochCrawler",
                    "PoolCrawler",
                    "CellCrawler",
                    "NetworkCrawler",
                ]),
        )
}
