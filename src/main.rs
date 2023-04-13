use crate::topic::{
    CellCrawler, ChainCrawler, ChainTransactionCrawler, CompactBlockCrawler, EpochCrawler,
    NetworkCrawler, PoolCrawler, RetentionTransactionCrawler, SubscribeNewTransaction,
    SubscribeProposedTransaction, SubscribeRejectedTransaction,
};
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

#[tokio::main]
async fn main() {
    let _logger_guard = init_logger();
    log::info!("CKBAnalyzer starting");

    let matches = clap_app().get_matches();
    let rpc_url = {
        let raw = match matches.value_of("ckb-rpc-url") {
            Some(raw) => raw.to_string(),
            None => match env::var_os("CKB_RPC_URL") {
                Some(raw) => raw.to_string_lossy().to_string(),
                None => {
                    panic!("Miss CKB Rpc url via neither --ckb-rpc-url nor environment variable \"CKB_RPC_URL\"");
                }
            },
        };
        let _ = url::Url::parse(&raw)
            .map_err(|err| panic!("Invalid CKB RPC url, url: \"{}\", error: {:?}", raw, err));
        raw
    };
    let subscription_addr = match matches.value_of("ckb-subscription-addr") {
        Some(raw) => raw.to_string(),
        None => match env::var_os("CKB_SUBSCRIPTION_ADDR") {
            Some(raw) => raw.to_string_lossy().to_string(),
            None => {
                panic!("Miss CKB subscription addr via neither --ckb-subscription-addr nor environment variable \"CKB_SUBSCRIPTION_ADDR\"");
            }
        },
    };
    let topics = values_t_or_exit!(matches, "topics", String);
    log::info!("CKB CKB RPC: \"{}\"", rpc_url);
    log::info!("CKB Node Subscription: \"{}\"", subscription_addr);
    log::info!("Topics: {:?}", topics);


    let pg_config = {
        let host = env::var_os("PGHOST")
            .or_else(|| env::var_os("POSTGRES_HOST"))
            .unwrap_or("127.0.0.1".into())
            .to_string_lossy()
            .to_string();
        let port = env::var_os("PGPORT")
            .or_else(|| env::var_os("POSTGRES_PORT"))
            .map(|raw| {
                raw.to_string_lossy()
                    .to_string()
                    .parse::<u16>()
                    .expect("invalid environment variable \"PGPORT\" or \"POSTGRES_PORT\"")
            })
            .unwrap_or(5432);
        //let database = env::var_os("PGDATABASE")
        //    .or_else(|| env::var_os("POSTGRES_DB"))
        //    .expect("requires environment variable \"PGDATABASE\" or \"POSTGRES_DB\"")
        //    .to_string_lossy()
        //    .to_string();
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
            //.dbname(&database)
            .user(&user)
            .password(&password)
            .application_name("CKBAnalyzer");
        config
    };
    let pg = {
        log::info!("Connecting to Postgres, {:?}", pg_config);
        let (pg, conn) = pg_config.connect(tokio_postgres::NoTls).await.unwrap();
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
    let node = Node::init_from_url(&rpc_url, PathBuf::new());

    log::info!("node.consensus().id = {}", format!("{}", node.consensus().id));
    let mut _connectors = Vec::new();

    for topic in topics {
        match topic.as_str() {
            "ChainCrawler" => {
                let last_block_number = {
                    match pg
                        .query_opt(
                            format!(
                                "SELECT number FROM {}.block ORDER BY time DESC LIMIT 1",
                                node.consensus().id,
                            )
                            .as_str(),
                            &[],
                        )
                        .await
                        .expect("query last block number")
                    {
                        None => 0,
                        Some(raw) => {
                            let number: i64 = raw.get(0);
                            number as u64
                        }
                    }
                };
                let handler = ChainCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(last_block_number).await;
                });
            }
            "EpochCrawler" => {
                let last_epoch_number = {
                    match pg
                        .query_opt(
                            format!(
                                "SELECT number FROM {}.epoch ORDER BY start_time DESC LIMIT 1",
                                node.consensus().id,
                            )
                            .as_str(),
                            &[],
                        )
                        .await
                        .expect("query last epoch number")
                    {
                        None => 0,
                        Some(raw) => {
                            let number: i64 = raw.get(0);
                            number as u64
                        }
                    }
                };
                let handler = EpochCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(last_epoch_number).await;
                });
            }
            "PoolCrawler" => {
                let handler = PoolCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run().await;
                });
            }
            "ChainTransactionCrawler" => {
                let last_block_number = {
                    match pg
                        .query_opt(
                            format!(
                                "SELECT number FROM {}.block_transaction ORDER BY time DESC LIMIT 1",
                                node.consensus().id,
                            )
                                .as_str(),
                            &[],
                        )
                        .await
                        .expect("query last block number")
                    {
                        None => 0,
                        Some(raw) => {
                            let number: i64 = raw.get(0);
                            number as u64
                        }
                    }
                };
                let handler = ChainTransactionCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(last_block_number).await;
                });
            }
            "SubscribeNewTransaction" => {
                let mut handler = SubscribeNewTransaction::new(node.clone(), query_sender.clone());
                let subscription_addr = subscription_addr.clone();
                tokio::spawn(async move {
                    handler.run(subscription_addr).await;
                });
            }
            "SubscribeProposedTransaction" => {
                let subscription_addr = subscription_addr.clone();
                let mut handler =
                    SubscribeProposedTransaction::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(subscription_addr).await;
                });
            }
            "SubscribeRejectedTransaction" => {
                let subscription_addr = subscription_addr.clone();
                let mut handler =
                    SubscribeRejectedTransaction::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(subscription_addr).await;
                });
            }
            "RetentionTransactionCrawler" => {
                let handler = RetentionTransactionCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run().await;
                });
            }
            "CellCrawler" => {
                let last_cell_block_number = {
                    match pg
                        .query_opt(
                            format!(
                                "SELECT block_number FROM {}.created_cell ORDER BY time DESC LIMIT 1",
                                node.consensus().id,
                            )
                            .as_str(),
                            &[],
                        )
                        .await
                        .expect("query last block number")
                    {
                        None => 0,
                        Some(raw) => {
                            let number: i64 = raw.get(0);
                            number as u64
                        }
                    }
                };
                let handler = CellCrawler::new(node.clone(), query_sender.clone());
                tokio::spawn(async move {
                    handler.run(last_cell_block_number).await;
                });
            }
            "NetworkCrawler" => {
                let shared = Arc::new(RwLock::new(SharedState::new()));
                let network_crawler =
                    NetworkCrawler::new(node.clone(), query_sender.clone(), Arc::clone(&shared));
                // workaround for Rust lifetime
                _connectors.push(
                    ConnectorBuilder::new()
                        .protocol_metas(network_crawler.build_protocol_metas())
                        .listening_addresses(vec![])
                        .build(network_crawler, shared),
                );
            }
            "CompactBlockCrawler" => {
                let shared = Arc::new(RwLock::new(SharedState::new()));
                let peer_state_crawler = CompactBlockCrawler::new(
                    node.clone(),
                    query_sender.clone(),
                    Arc::clone(&shared),
                );
                // workaround for Rust lifetime
                _connectors.push(
                    ConnectorBuilder::new()
                        .protocol_metas(peer_state_crawler.build_protocol_metas())
                        .listening_addresses(vec![])
                        .build(peer_state_crawler, shared),
                );
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
            Arg::with_name("envfile")
                .long("envfile")
                .value_name("FILEPATH")
                .required(false)
                .takes_value(true)
                .validator(|filepath| {
                    dotenv::from_path(&filepath)
                        .map(|_| ())
                        .map_err(|err| err.to_string())
                }),
        )
        .arg(
            Arg::with_name("ckb-rpc-url")
                .long("ckb-rpc-url")
                .value_name("URL")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("ckb-subscription-addr")
                .long("ckb-subscription-addr")
                .value_name("HOST:PORT")
                .required(false)
                .takes_value(true),
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
                    "ChainCrawler,\
                    EpochCrawler,\
                    PoolCrawler,\
                    ChainTransactionCrawler,\
                    SubscribeNewTransaction,\
                    SubscribeProposedTransaction,\
                    SubscribeRejectedTransaction,\
                    RetentionTransactionCrawler,\
                    CellCrawler,\
                    NetworkCrawler,\
                    CompactBlockCrawler",
                )
                .possible_values(&[
                    "ChainCrawler",
                    "EpochCrawler",
                    "PoolCrawler",
                    "ChainTransactionCrawler",
                    "SubscribeNewTransaction",
                    "SubscribeProposedTransaction",
                    "SubscribeRejectedTransaction",
                    "RetentionTransactionCrawler",
                    "CellCrawler",
                    "NetworkCrawler",
                    "CompactBlockCrawler",
                ]),
        )
}
