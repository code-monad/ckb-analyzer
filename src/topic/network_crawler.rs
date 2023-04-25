use crate::util::{bootnodes::bootnodes, ipinfo::lookup_ipinfo, multiaddr::addr_to_ip};
use ckb_testkit::connector::message::build_discovery_get_nodes;
use ckb_testkit::{
    ckb_types::{packed, prelude::*},
    compress,
    connector::SharedState,
    decompress, Node,
};

use ckb_network::SupportProtocols;
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
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio_util::codec::{length_delimited::LengthDelimitedCodec, Decoder, Encoder};
use async_trait::async_trait;

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
    node: Node,
    query_sender: crossbeam::channel::Sender<String>,
    shared: Arc<RwLock<SharedState>>,

    // all observed addresses
    observed_addresses: Arc<RwLock<HashSet<Multiaddr>>>,

    // #{ ip => peer_info }
    online: Arc<RwLock<HashMap<Ip, PeerInfo>>>,

    // already known iP
    known_ips: HashSet<String>,
}

type Ip = String;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    address: Multiaddr,
    last_seen_time: Option<Instant>,
    reachable: HashSet<Ip>,
    client_version: String,
}

impl Clone for NetworkCrawler {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            query_sender: self.query_sender.clone(),
            shared: Arc::clone(&self.shared),
            observed_addresses: Arc::clone(&self.observed_addresses),
            online: Arc::clone(&self.online),
            known_ips: self.known_ips.clone(),
        }
    }
}

impl NetworkCrawler {
    /// Create a NetworkCrawler
    pub fn new(
        node: Node,
        query_sender: crossbeam::channel::Sender<String>,
        shared: Arc<RwLock<SharedState>>,
    ) -> Self {
        #[allow(clippy::mutable_key_type)]
        let bootnodes = bootnodes(&node);
        Self {
            node,
            query_sender,
            shared,
            observed_addresses: Arc::new(RwLock::new(bootnodes.clone())),
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
                            },
                        )
                    })
                    .collect(),
            )),
            known_ips: Default::default(),
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

    fn received_identify(&mut self, context: P2PProtocolContextMutRef<'_>, data: Bytes) {
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
                        ckb_testkit::debug!(
                            "NetworkCrawler received IdentifyMessage, address: {}, time: {:?}",
                            context.session.address,
                            Instant::now()
                        );
                        if let Ok(mut online) = self.online.write() {
                            let entry = online
                                .entry(addr_to_ip(&context.session.address))
                                .or_insert_with(|| PeerInfo {
                                    address: context.session.address.clone(),
                                    last_seen_time: Default::default(),
                                    reachable: Default::default(),
                                    client_version: Default::default(),
                                });
                            entry.client_version = client_version;
                            entry.last_seen_time = Some(Instant::now());
                        }
                    }
                    Err(err) => {
                        ckb_testkit::error!("NetworkCrawler received invalid Identify Payload, address: {}, error: {:?}", context.session.address, err);
                    }
                }
            }
            Err(err) => {
                ckb_testkit::error!(
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
                        ckb_testkit::debug!(
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
                                        if observed_addresses.insert(addr.clone()) {
                                            ckb_testkit::debug!(
                                                "NetworkCrawler observed new address: {}",
                                                addr
                                            );
                                        }
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
                        ckb_testkit::error!(
                            "NetworkCrawler received invalid DiscoveryMessage, address: {}, error: {:?}",
                            context.session.address,
                            err
                        );
                    }
                }
            }
        }
    }

    async fn connected_discovery(&mut self, context: P2PProtocolContextMutRef<'_>, protocol_version: &str) {
        let discovery_get_node_message = build_discovery_get_nodes(None, 1000u32, 1u32);
        if protocol_version == "0.0.1" {
            let mut codec = LengthDelimitedCodec::new();
            let mut bytes = BytesMut::new();
            codec
                .encode(discovery_get_node_message.as_bytes(), &mut bytes)
                .expect("encode must be success");
            let message_bytes = bytes.freeze();
            context.send_message(message_bytes).await.unwrap();
        } else {
            let message_bytes = discovery_get_node_message.as_bytes();
            context.send_message(message_bytes).await.unwrap();
        }
    }
}

#[async_trait]
impl P2PServiceProtocol for NetworkCrawler {
    async fn init(&mut self, context: &mut P2PProtocolContext) {
        if context.proto_id == SupportProtocols::Sync.protocol_id() {
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    DIAL_ONLINE_ADDRESSES_INTERVAL,
                    DIAL_ONLINE_ADDRESSES_TOKEN,
                ).await
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    PRUNE_OFFLINE_ADDRESSES_INTERVAL,
                    PRUNE_OFFLINE_ADDRESSES_TOKEN,
                ).await
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    DISCONNECT_TIMEOUT_SESSION_INTERVAL,
                    DISCONNECT_TIMEOUT_SESSION_TOKEN,
                ).await
                .unwrap();
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    POSTGRES_ONLINE_ADDRESS_INTERVAL,
                    POSTGRES_ONLINE_ADDRESSES_TOKEN,
                ).await
                .unwrap();
        }
    }

    async fn notify(&mut self, context: &mut P2PProtocolContext, token: u64) {
        match token {
            DIAL_ONLINE_ADDRESSES_TOKEN => {
                // TODO reset notify to adjust the length of observed_addresses
                // context.remove_service_notify();
                // context.set_service_notify();
                let mut rng = thread_rng();
                if let Ok(observed_addresses) = self.observed_addresses.write() {
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
                        .get_session(random_address)
                        .is_none()
                    {
                        let _ = context.dial((*random_address).clone(), P2PTargetProtocol::All);
                    }
                };
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
                                    network: self.node.consensus().id.clone(),
                                    time: now,
                                    version: peer_info.client_version.clone(),
                                    ip: ip.clone(),
                                    n_reachable: n_reachable as i32,
                                };
                                entries.push(entry);
                            }
                        }
                    }
                }

                for entry in entries.iter() {
                    let raw_query = format!(
                        "INSERT INTO {}.peer(time, version, ip, n_reachable) \
                             VALUES ('{}', '{}', '{}', {})",
                        entry.network, entry.time, entry.version, entry.ip, entry.n_reachable,
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
                            ..
                        }) = lookup_ipinfo(&entry.ip)
                        {
                            let entry = crate::entry::IpInfo {
                                network: entry.network,
                                ip,
                                country,
                                city,
                                region,
                                company: company.map(|company| company.name).unwrap_or_default(),
                            };
                            let raw_query = format!(
                                "INSERT INTO {}.ipinfo(ip, country, city, region, company) \
                                VALUES ('{}', '{}', '{}', '{}', '{}') ON CONFLICT DO NOTHING",
                                entry.network,
                                entry.ip,
                                entry.country,
                                entry.city,
                                entry.region,
                                entry.company,
                            );
                            self.known_ips.insert(entry.ip);
                            self.query_sender.send(raw_query).unwrap();
                        }
                    }
                }
            }
            PRUNE_OFFLINE_ADDRESSES_TOKEN => {
                // TODO prune offline addresses
            }
            _ => unreachable!(),
        }
    }

    async fn connected(&mut self, context: P2PProtocolContextMutRef<'_>, protocol_version: &str) {
        ckb_testkit::debug!(
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
            self.connected_discovery(context, protocol_version).await
        }
    }

    async fn disconnected(&mut self, context: P2PProtocolContextMutRef<'_>) {
        ckb_testkit::debug!(
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

    async fn received(&mut self, context: P2PProtocolContextMutRef<'_>, data: Bytes) {
        if context.proto_id == SupportProtocols::Discovery.protocol_id() {
            self.received_discovery(context, data)
        } else if context.proto_id == SupportProtocols::Identify.protocol_id() {
            self.received_identify(context, data)
        }
    }
}

#[async_trait]
impl P2PServiceHandle for NetworkCrawler {
    async fn handle_error(&mut self, _context: &mut P2PServiceContext, error: P2PServiceError) {
        match &error {
            P2PServiceError::DialerError { .. } | P2PServiceError::ProtocolSelectError { .. } => {
                // discard
            }
            _ => {
                ckb_testkit::error!("NetworkCrawler detect service error, error: {:?}", error);
            }
        }
    }

    /// Handling session establishment and disconnection events
    async fn handle_event(&mut self, context: &mut P2PServiceContext, event: P2PServiceEvent) {
        match event {
            P2PServiceEvent::SessionOpen {
                session_context: session,
            } => {
                ckb_testkit::debug!("NetworkCrawler open session: {:?}", session);
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
                ckb_testkit::debug!("NetworkCrawler close session: {:?}", session);
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
