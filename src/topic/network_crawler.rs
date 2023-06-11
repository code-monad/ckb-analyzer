use crate::util::{bootnodes::bootnodes, ipinfo::lookup_ipinfo, multiaddr::addr_to_ip};
use ckb_testkit::connector::message::build_discovery_get_nodes;
use ckb_testkit::{
    ckb_types::{packed, prelude::*},
    compress,
    connector::SharedState,
    decompress, Node, SupportProtocols,
};
use p2p::{
    builder::MetaBuilder as P2PMetaBuilder,
    bytes::{Bytes, BytesMut},
    context::ProtocolContext as P2PProtocolContext,
    context::ProtocolContextMutRef as P2PProtocolContextMutRef,
    context::ServiceContext as P2PServiceContext,
    context::SessionContext,
    multiaddr::Multiaddr,
    service::ProtocolHandle as P2PProtocolHandle,
    service::ProtocolMeta as P2PProtocolMeta,
    service::ServiceError as P2PServiceError,
    service::ServiceEvent as P2PServiceEvent,
    service::TargetProtocol as P2PTargetProtocol,
    traits::ServiceHandle as P2PServiceHandle,
    traits::ServiceProtocol as P2PServiceProtocol,
};
use rand::{thread_rng, Rng};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::error::Error;
use std::ops::Mul;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use p2p::error::{DialerErrorKind, SendErrorKind};
use tokio::runtime::Handle;
use tokio_util::codec::{length_delimited::LengthDelimitedCodec, Decoder, Encoder};


// TODO Adjust the parameters
const DIAL_ONLINE_ADDRESSES_INTERVAL: Duration = Duration::from_secs(1);
const PRUNE_OFFLINE_ADDRESSES_INTERVAL: Duration = Duration::from_secs(30 * 60);
const DISCONNECT_TIMEOUT_SESSION_INTERVAL: Duration = Duration::from_secs(10);
const POSTGRES_ONLINE_ADDRESS_INTERVAL: Duration = Duration::from_secs(60);
const DIAL_ONLINE_ADDRESSES_TOKEN: u64 = 1;
const PRUNE_OFFLINE_ADDRESSES_TOKEN: u64 = 2;
const DISCONNECT_TIMEOUT_SESSION_TOKEN: u64 = 3;
const POSTGRES_ONLINE_ADDRESSES_TOKEN: u64 = 4;

const ADDRESS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Copy, Clone, Debug, serde::Deserialize)]
pub enum CKBNetworkType {
    Mirana,
    Pudge,
    Dev
}

impl From<String> for CKBNetworkType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "mirana"|"ckb"|"main" => CKBNetworkType::Mirana,
            "pudge"|"ckb_testnet"|"test" => CKBNetworkType::Pudge,
            "dev"|"ckb_dev" => CKBNetworkType::Dev,
            _ => unreachable!(),
        }
    }
}

impl CKBNetworkType {
    pub fn into_legacy_str(&self) -> String {
        match self {
            CKBNetworkType::Mirana => "ckb".to_string(),
            CKBNetworkType::Pudge => "ckb_testnet".to_string(),
            CKBNetworkType::Dev => "ckb_dev".to_string(),
        }
    }
}

/// NetworkCrawler crawl the network reachability info.
///
/// This service opens 2 protocols, Identify and Discovery:
///
/// * A ticker to trigger dialing observed addresses
/// * A ticker to trigger pruning timeout sessions
/// * TODO A ticker to trigger pruning offline addresses
/// * When opening Identify protocol on a session, reject it if its session type is inbound or
/// identify name is "CKBAnalyzer", record into `self.online_nodes`.
/// * When opening Discovery protocol on a session, send `GetNodes` message.
/// * When receiving inv `Nodes`, record into `self.reachable`
pub struct NetworkCrawler {
    network_type: CKBNetworkType,
    query_sender: crossbeam::channel::Sender<String>,
    shared: Arc<RwLock<SharedState>>,

    // all observed addresses
    observed_addresses: Arc<RwLock<HashMap<Multiaddr, usize>>>,

    // #{ ip => peer_info }
    online: Arc<RwLock<HashMap<Ip, PeerInfo>>>,

    // Already known iP
    known_ips: HashSet<String>,

    // For identify version outside peer protocol context
    observed_version: Arc<RwLock<HashMap<Multiaddr, String>>>,

    // If observed count over this, even peer unable to dial will treat as observed
    witness_bound: usize,
}

type Ip = String;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    address: Multiaddr,
    last_seen_time: Option<Instant>,
    reachable: HashSet<Ip>,
    client_version: String,
    is_full_node: u8, // 0: unknown, 1: full node, 2: not full node
}

impl Clone for NetworkCrawler {
    fn clone(&self) -> Self {
        Self {
            network_type: self.network_type.clone(),
            query_sender: self.query_sender.clone(),
            shared: Arc::clone(&self.shared),
            observed_addresses: Arc::clone(&self.observed_addresses),
            online: Arc::clone(&self.online),
            known_ips: self.known_ips.clone(),
            observed_version: self.observed_version.clone(),
            witness_bound: self.witness_bound.clone(),
        }
    }
}

impl NetworkCrawler {
    /// Create a NetworkCrawler
    pub fn new(
        network_type: CKBNetworkType,
        query_sender: crossbeam::channel::Sender<String>,
        shared: Arc<RwLock<SharedState>>,
        witness_bound: usize,
    ) -> Self {
        #[allow(clippy::mutable_key_type)]
        let bootnodes = bootnodes(network_type);
        let observed_addresses = bootnodes
            .iter()
            .map(|address| (address.clone(), 1))
            .collect::<HashMap<_, _>>();
        Self {
            network_type,
            query_sender,
            shared,
            observed_addresses: Arc::new(RwLock::new(observed_addresses.clone())),
            online: Arc::new(RwLock::new(
                bootnodes
                    .into_iter()
                    .map(|address| {
                        (
                            addr_to_ip(&address),
                            PeerInfo {
                                address,
                                last_seen_time: Default::default(),
                                reachable: Default::default(),
                                client_version: Default::default(),
                                is_full_node: 1,
                            },
                        )
                    })
                    .collect(),
            )),
            known_ips: Default::default(),
            observed_version: Default::default(),
            witness_bound
        }
    }

    /// Convert NetworkCrawler into P2PProtocolMeta
    pub fn build_protocol_metas(&self) -> Vec<P2PProtocolMeta> {
        vec![
            {
                let meta_builder: P2PMetaBuilder = SupportProtocols::Identify.into();
                meta_builder
                    .service_handle(move || P2PProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
            {
                let meta_builder: P2PMetaBuilder = SupportProtocols::Discovery.into();
                meta_builder
                    .service_handle(move || P2PProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
            {
                // Necessary to communicate with CKB full node
                let meta_builder: P2PMetaBuilder = SupportProtocols::Sync.into();
                meta_builder
                    // Only Timer, Sync, Relay make compress
                    .before_send(compress)
                    .before_receive(|| Some(Box::new(decompress)))
                    .service_handle(move || P2PProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
        ]
    }

    fn received_identify(&mut self, context: P2PProtocolContextMutRef, data: Bytes) {
        match packed::IdentifyMessage::from_compatible_slice(data.as_ref()) {
            Ok(message) => {
                match packed::Identify::from_compatible_slice(
                    message.identify().raw_data().as_ref(),
                ) {
                    Ok(identify_payload) => {
                        let client_version_vec: Vec<u8> =
                            identify_payload.client_version().unpack();
                        let client_version =
                            String::from_utf8_lossy(&client_version_vec).to_string();

                        let client_name_vec: Vec<u8> = identify_payload.name().unpack();

                        let client_flag: u64 =  identify_payload.flag().unpack();

                        // protocol is private mod in ckb, use the bitflag map directly
                        // since a light node can't provide LIGHT_CLIENT serv but full node can, use this as a workaround
                        let is_full_node = (client_flag & 0b10000) == 0b10000;

                        log::info!(
                            "NetworkCrawler received IdentifyMessage, address: {}, time: {:?}",
                            context.session.address,
                            Instant::now()
                        );
                        if let Ok(mut observed_addresses) = self.observed_addresses.write() {
                            *observed_addresses.entry(context.session.address.clone()).or_insert(1) = 1;
                        }
                        if let Ok(mut version_map) = self.observed_version.write() {
                            *version_map.entry(context.session.address.clone()).or_insert(client_version.clone()) = client_version.clone();
                        }
                        if let (Ok(mut online), client_version) = (self.online.write(), client_version) {
                            let entry = online
                                .entry(addr_to_ip(&context.session.address))
                                .or_insert_with(|| PeerInfo {
                                    address: context.session.address.clone(),
                                    last_seen_time: Default::default(),
                                    reachable: Default::default(),
                                    client_version: Default::default(),
                                    is_full_node: if is_full_node { 1 } else { 2 },
                                });
                            entry.client_version = client_version;
                            entry.last_seen_time = Some(Instant::now());
                        }
                    }
                    Err(err) => {
                        log::error!("NetworkCrawler received invalid Identify Payload, address: {}, error: {:?}", context.session.address, err);
                    }
                }
            }
            Err(err) => {
                log::error!(
                    "NetworkCrawler received invalid IdentifyMessage, address: {}, error: {:?}",
                    context.session.address,
                    err
                );
            }
        }
    }

    fn received_discovery(&mut self, context: P2PProtocolContextMutRef, data: Bytes) {
        match packed::DiscoveryMessage::from_compatible_slice(data.as_ref()) {
            Ok(message) => {
                match message.payload().to_enum() {
                    packed::DiscoveryPayloadUnion::Nodes(discovery_nodes) => {
                        log::debug!(
                            "NetworkCrawler received DiscoveryMessages Nodes, address: {}, nodes.len: {}",
                            context.session.address,
                            discovery_nodes.items().len(),
                        );

                        if let Ok(mut observed_addresses) = self.observed_addresses.write() {
                            for node in discovery_nodes.items() {
                                for address in node.addresses() {
                                    if let Ok(addr) =
                                        Multiaddr::try_from(address.raw_data().to_vec())
                                    {
                                        log::debug!(
                                                "NetworkCrawler observed new address: {}",
                                                addr
                                            );

                                        // insert default 1 or increment
                                        *observed_addresses.entry(addr).or_insert(1) += 1;
                                    }
                                }
                            }
                        }

                        {
                            if let Ok(mut online) = self.online.write() {
                                let entry = online
                                    .entry(addr_to_ip(&context.session.address))
                                    .or_insert_with(|| PeerInfo {
                                        address: context.session.address.clone(),
                                        last_seen_time: Default::default(),
                                        reachable: Default::default(),
                                        client_version: Default::default(),
                                        is_full_node: 0, // Can't get this, leave unknown
                                    });
                                for node in discovery_nodes.items() {
                                    for address in node.addresses() {
                                        if let Ok(addr) =
                                            Multiaddr::try_from(address.raw_data().to_vec())
                                        {
                                            entry.reachable.insert(addr_to_ip(&addr));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    packed::DiscoveryPayloadUnion::GetNodes(_discovery_get_nodes) => {
                        // discard
                    }
                }
            }
            Err(err) => {
                // ckb2019, before hard fork
                let mut data = BytesMut::from(data.as_ref());
                let mut codec = LengthDelimitedCodec::new();
                match codec.decode(&mut data) {
                    Ok(Some(frame)) => self.received_discovery(context, frame.freeze()),
                    _ => {
                        log::error!(
                            "NetworkCrawler received invalid DiscoveryMessage, address: {}, error: {:?}",
                            context.session.address,
                            err
                        );
                    }
                }
            }
        }
    }

    fn connected_discovery(&mut self, context: P2PProtocolContextMutRef, protocol_version: &str) {
        let discovery_get_node_message = build_discovery_get_nodes(None, 1000u32, 1u32);
        if protocol_version == "0.0.1" {
            let mut codec = LengthDelimitedCodec::new();
            let mut bytes = BytesMut::new();
            codec
                .encode(discovery_get_node_message.as_bytes(), &mut bytes)
                .expect("encode must be success");
            let message_bytes = bytes.freeze();
            context.send_message(message_bytes).unwrap();
        } else {
            let message_bytes = discovery_get_node_message.as_bytes();
            context.send_message(message_bytes).unwrap();
        }
    }

    fn online_witnesses(&mut self, addr: &Multiaddr) {
        if let Ok(mut observed_addresses) = self.observed_addresses.write() {
            if let Some(witnesses_count) = observed_addresses.get(&addr) {
                if witnesses_count >= &self.witness_bound {
                    log::info!("Failed to dial {:?} but still treat as online because of multiple witnesses. witnesses_count: {}", addr, witnesses_count);
                    if let Ok(mut online) = self.online.write() {
                        let entry = online
                            .entry(addr_to_ip(&addr))
                            .or_insert_with(|| PeerInfo {
                                address: addr.clone(),
                                last_seen_time: Default::default(),
                                reachable: Default::default(),
                                client_version: Default::default(),
                                is_full_node: 0, // can't get this, leave unknown
                            });
                        entry.last_seen_time = Some(Instant::now());
                        if let Ok(version_map) = self.observed_version.read() {
                            if version_map.contains_key(&addr) {
                                let version= version_map.get(&addr).unwrap();
                                entry.client_version = version.clone();
                            }
                        }
                        // Reset witness count
                        observed_addresses.entry(addr.to_owned()).and_modify( |cnt|
                            *cnt = 0
                        );
                    }
                }
            }
        }
    }
}

impl P2PServiceProtocol for NetworkCrawler {
    fn init(&mut self, context: &mut P2PProtocolContext) {
        if context.proto_id == SupportProtocols::Sync.protocol_id() {
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    DIAL_ONLINE_ADDRESSES_INTERVAL,
                    DIAL_ONLINE_ADDRESSES_TOKEN,
                )
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    PRUNE_OFFLINE_ADDRESSES_INTERVAL,
                    PRUNE_OFFLINE_ADDRESSES_TOKEN,
                )
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    DISCONNECT_TIMEOUT_SESSION_INTERVAL,
                    DISCONNECT_TIMEOUT_SESSION_TOKEN,
                )
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    POSTGRES_ONLINE_ADDRESS_INTERVAL,
                    POSTGRES_ONLINE_ADDRESSES_TOKEN,
                )
                .unwrap();
        }
    }

    fn notify(&mut self, context: &mut P2PProtocolContext, token: u64) {
        match token {
            DIAL_ONLINE_ADDRESSES_TOKEN => {
                // context.remove_service_notify();
                // context.set_service_notify();
                let mut rng = thread_rng();
                let mut dial_res = None;
                let mut addr = None;
                if let Ok(mut observed_addresses) = self.observed_addresses.write() {
                    let random_index = rng.gen_range(0..observed_addresses.len());
                    let random_address = observed_addresses
                        .iter()
                        .collect::<Vec<_>>()
                        .get(random_index)
                        .cloned()
                        .unwrap();
                    if self
                        .shared
                        .read()
                        .unwrap()
                        .get_session(random_address.0)
                        .is_none()
                    {
                        dial_res = Some(context.dial(random_address.0.clone(), P2PTargetProtocol::All));
                        addr = Some(random_address.0.clone());
                    }
                };
                if let Some(Err(_)) = dial_res {
                    self.online_witnesses(&addr.unwrap());
                }
            }
            DISCONNECT_TIMEOUT_SESSION_TOKEN => {
                let sessions = {
                    self.shared
                        .read()
                        .map(|shared| {
                            shared
                                .get_sessions()
                                .iter()
                                .map(|s| (*s).clone())
                                .collect::<Vec<SessionContext>>()
                        })
                        .unwrap_or_default()
                };
                if let Ok(online) = self.online.read() {
                    for session in sessions {
                        if let Some(peer_info) = online.get(&addr_to_ip(&session.address)) {
                            if let Some(last_seen_time) = peer_info.last_seen_time {
                                if last_seen_time.elapsed() > Duration::from_secs(10) {
                                    let _ = context.disconnect(session.id);
                                }
                            }
                        }
                    }
                }
            }
            POSTGRES_ONLINE_ADDRESSES_TOKEN => {
                let now = chrono::Utc::now().naive_utc();
                let mut entries = Vec::new();
                if let Ok(online) = self.online.read() {
                    for (ip, peer_info) in online.iter() {
                        if let Some(last_seen_time) = peer_info.last_seen_time {
                            if last_seen_time.elapsed() <= ADDRESS_TIMEOUT {
                                // It's a online address
                                let n_reachable = {
                                    peer_info
                                        .reachable
                                        .iter()
                                        .filter(|ip1| online.contains_key(*ip1))
                                        .count()
                                };
                                let entry = crate::entry::Peer {
                                    network: self.network_type.into_legacy_str(),
                                    time: now,
                                    version: peer_info.client_version.clone(),
                                    ip: ip.clone(),
                                    n_reachable: n_reachable as i32,
                                    address: peer_info.address.to_string().clone(),
                                    peer_id: peer_info.address.to_string().split('/').collect::<Vec<&str>>().last().unwrap_or(&"").to_string(),
                                    node_type: peer_info.is_full_node,
                                };
                                entries.push(entry);
                            }
                        }
                    }
                }

                for entry in entries.iter() {
                    let raw_query = format!(
                        "INSERT INTO {}.peer(time, version, ip, n_reachable, address, peer_id, node_type) \
                        VALUES ('{}', '{}', '{}', {}, '{}', '{}', {}) \
                        ON CONFLICT (address) DO UPDATE SET time = excluded.time, n_reachable = excluded.n_reachable",
                        entry.network, entry.time, entry.version, entry.ip, entry.n_reachable, entry.address, entry.peer_id, entry.node_type,
                    );
                    self.query_sender.send(raw_query).unwrap();
                }

                for entry in entries {
                    if !self.known_ips.contains(&entry.ip) {
                        if let Ok(ipinfo::IpDetails {
                            ip,
                            country,
                            city,
                            region,
                            company,
                            loc,
                            ..
                        }) = lookup_ipinfo(&entry.ip)
                        {
                            let entry = crate::entry::IpInfo {
                                network: entry.network,
                                ip,
                                country: country.replace("'", "''"),
                                city: city.replace("'", "''"),
                                region,
                                company: company.map(|company| company.name).unwrap_or_default().replace("'", "''"),
                            };

                            let mut lat_lon = loc.split(',');
                            // Parse each part to f64, providing a default if the value can't be parsed
                            let latitude: f64 = lat_lon.next().and_then(|s| f64::from_str(s).ok()).unwrap_or_default();
                            let longitude: f64 = lat_lon.next().and_then(|s| f64::from_str(s).ok()).unwrap_or_default();

                            let raw_query = format!(
                                "INSERT INTO {}.ipinfo(ip, country, city, region, company, latitude, longitude) \
                                VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}') ON CONFLICT DO NOTHING",
                                entry.network,
                                entry.ip,
                                entry.country,
                                entry.city,
                                entry.region,
                                entry.company,
                                latitude,
                                longitude,
                            );
                            self.known_ips.insert(entry.ip);
                            self.query_sender.send(raw_query).unwrap();

                            let query = format!("INSERT INTO common_info.lat_info (city, country, state1, latitude, longitude)
                            VALUES ({}, {}, {}, {}, {}) ON CONFLICT (city, country) DO NOTHING", entry.city, entry.country, entry.region, latitude, longitude);

                        } else {
                            log::warn!("Failed to lookup ipinfo for {}", entry.ip);
                        }
                    }
                }
            }
            PRUNE_OFFLINE_ADDRESSES_TOKEN => {
                // TODO: prune offline addresses
            }
            _ => unreachable!(),
        }
    }

    fn connected(&mut self, context: P2PProtocolContextMutRef, protocol_version: &str) {
        log::debug!(
            "NetworkCrawler open protocol, protocol_name: {} address: {}",
            context
                .protocols()
                .get(&context.proto_id())
                .map(|p| p.name.as_str())
                .unwrap_or_default(),
            context.session.address
        );
        if let Ok(mut shared) = self.shared.write() {
            shared.add_protocol(context.session, context.proto_id);
        }

        if context.proto_id() == SupportProtocols::Discovery.protocol_id() {
            self.connected_discovery(context, protocol_version)
        }
    }

    fn disconnected(&mut self, context: P2PProtocolContextMutRef) {
        log::debug!(
            "NetworkCrawler close protocol, protocol_name: {}, address: {:?}",
            context
                .protocols()
                .get(&context.proto_id())
                .map(|p| p.name.as_str())
                .unwrap_or_default(),
            context.session.address
        );
        if let Ok(mut shared) = self.shared.write() {
            shared.remove_protocol(&context.session.id, &context.proto_id());
        }
    }

    fn received(&mut self, context: P2PProtocolContextMutRef, data: Bytes) {
        if context.proto_id == SupportProtocols::Discovery.protocol_id() {
            self.received_discovery(context, data)
        } else if context.proto_id == SupportProtocols::Identify.protocol_id() {
            self.received_identify(context, data)
        }
    }
}

impl P2PServiceHandle for NetworkCrawler {
    fn handle_error(&mut self, _context: &mut P2PServiceContext, error: P2PServiceError) {
        match &error {
            P2PServiceError::DialerError { address, error }  => {
                self.online_witnesses(address);
            },
            P2PServiceError::ProtocolSelectError { .. } => {
                // discard
            }
            _ => {
                log::error!("NetworkCrawler detect service error, error: {:?}", error);
            }
        }
    }

    /// Handling session establishment and disconnection events
    fn handle_event(&mut self, context: &mut P2PServiceContext, event: P2PServiceEvent) {
        match event {
            P2PServiceEvent::SessionOpen {
                session_context: session,
            } => {
                log::debug!("NetworkCrawler open session: {:?}", session);
                // Reject passive connection
                if session.ty.is_inbound() {
                    let _ = context.disconnect(session.id);
                    return;
                }

                let _add = self
                    .shared
                    .write()
                    .map(|mut shared| shared.add_session(session.as_ref().to_owned()));
            }
            P2PServiceEvent::SessionClose {
                session_context: session,
            } => {
                log::debug!("NetworkCrawler close session: {:?}, addr: {:?}", session, session.address);
                let _removed = self
                    .shared
                    .write()
                    .map(|mut shared| shared.remove_session(&session.id));
            }
            _ => {
                unimplemented!()
            }
        }
    }
}
