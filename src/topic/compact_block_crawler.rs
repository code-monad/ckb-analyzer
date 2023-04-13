use crate::util::{bootnodes::bootnodes, ipinfo::lookup_ipinfo, multiaddr::addr_to_ip};
use ckb_testkit::{
    ckb_types::{packed, prelude::*},
    compress,
    connector::message::{build_discovery_get_nodes, build_identify_message},
    connector::SharedState,
    decompress, Node, SupportProtocols,
};
use lru::LruCache;
use p2p::{
    builder::MetaBuilder as P2PMetaBuilder,
    bytes::{Bytes, BytesMut},
    context::ProtocolContext as P2PProtocolContext,
    context::ProtocolContextMutRef as P2PProtocolContextMutRef,
    context::ServiceContext as P2PServiceContext,
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
use std::collections::HashSet;
use std::convert::TryFrom;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::codec::{length_delimited::LengthDelimitedCodec, Decoder, Encoder};

type Ip = String;

const DIAL_ONLINE_ADDRESSES_INTERVAL: Duration = Duration::from_secs(1);
const DIAL_ONLINE_ADDRESSES_TOKEN: u64 = 1;

/// NOTE: CKB full node eviction mechanism only faces to outbound peers. We don't need to care
/// about the mechanism evict us.
pub struct CompactBlockCrawler {
    node: Node,
    query_sender: crossbeam::channel::Sender<String>,
    shared: Arc<RwLock<SharedState>>,

    // RPC local_node_info.version
    client_version: String,

    // all observed addresses
    observed_addresses: Arc<RwLock<HashSet<Multiaddr>>>,

    // Only exist for RelayProtocol and RelayV2Protocol.
    // It does not need to share between RelayProtocol and RelayV2Protocol since they will not
    // work at one time.
    compact_blocks: Option<LruCache<packed::Byte32, Ip>>,

    known_ips: HashSet<Ip>,
}

impl Clone for CompactBlockCrawler {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            query_sender: self.query_sender.clone(),
            shared: Arc::clone(&self.shared),
            observed_addresses: Arc::clone(&self.observed_addresses),
            client_version: self.client_version.clone(),
            compact_blocks: None,
            known_ips: HashSet::new(),
        }
    }
}

impl CompactBlockCrawler {
    pub fn new(
        node: Node,
        query_sender: crossbeam::channel::Sender<String>,
        shared: Arc<RwLock<SharedState>>,
    ) -> Self {
        #[allow(clippy::mutable_key_type)]
        let bootnodes = bootnodes(&node);
        let client_version = node.rpc_client().local_node_info().version;
        Self {
            node,
            query_sender,
            shared,
            observed_addresses: Arc::new(RwLock::new(bootnodes)),
            client_version,
            compact_blocks: Default::default(),
            known_ips: Default::default(),
        }
    }

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
            {
                let meta_builder: P2PMetaBuilder = SupportProtocols::Relay.into();
                meta_builder
                    .before_send(compress)
                    .before_receive(|| Some(Box::new(decompress)))
                    .service_handle(move || P2PProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
            {
                let meta_builder: P2PMetaBuilder = SupportProtocols::RelayV2.into();
                meta_builder
                    .before_send(compress)
                    .before_receive(|| Some(Box::new(decompress)))
                    .service_handle(move || P2PProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
        ]
    }

    fn received_discovery(&mut self, context: P2PProtocolContextMutRef, data: Bytes) {
        match packed::DiscoveryMessage::from_compatible_slice(data.as_ref()) {
            Ok(message) => {
                match message.payload().to_enum() {
                    packed::DiscoveryPayloadUnion::Nodes(discovery_nodes) => {
                        ckb_testkit::debug!(
                            "CompactBlockCrawler received DiscoveryMessages Nodes, address: {}, nodes.len: {}",
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
                                                "CompactBlockCrawler observed new address: {}",
                                                addr
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    packed::DiscoveryPayloadUnion::GetNodes(_discovery_get_nodes) => {
                        // TODO
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
                            "CompactBlockCrawler received invalid DiscoveryMessage, address: {}, error: {:?}",
                            context.session.address,
                            err
                        );
                    }
                }
            }
        }
    }

    fn received_relay(&mut self, context: P2PProtocolContextMutRef, data: Bytes) {
        match packed::RelayMessage::from_compatible_slice(data.as_ref()) {
            Ok(message) => {
                match message.to_enum() {
                    packed::RelayMessageUnion::CompactBlock(block) => {
                        let ip = addr_to_ip(&context.session.address);
                        self.insert_ipinfo(&ip);
                        self.update_peer_last_compact_block(ip.clone(), &block);
                        self.insert_compact_block_first_seen(ip, &block);
                    }
                    packed::RelayMessageUnion::RelayTransactionHashes(_) => { /* discard */ }
                    item => {
                        ckb_testkit::warn!(
                            "CompactBlockCrawler received unexpected message \"{}\"",
                            item.item_name()
                        );
                    }
                }
            }
            Err(err) => {
                ckb_testkit::error!(
                    "CompactBlockCrawler received invalid RelayMessage, address: {}, error: {:?}",
                    context.session.address,
                    err
                );
            }
        }
    }

    fn connected_discovery(&mut self, context: P2PProtocolContextMutRef, protocol_version: &str) {
        let message = build_discovery_get_nodes(None, 1000, 1);
        if protocol_version == "0.0.1" {
            let mut codec = LengthDelimitedCodec::new();
            let mut bytes = BytesMut::new();
            codec
                .encode(message.as_bytes(), &mut bytes)
                .expect("encode must be success");
            let message_bytes = bytes.freeze();
            context.send_message(message_bytes).unwrap();
        } else {
            let message_bytes = message.as_bytes();
            context.send_message(message_bytes).unwrap();
        }
    }

    fn connected_identify(&mut self, context: P2PProtocolContextMutRef, _protocol_version: &str) {
        let network_identifier = {
            let consensus = self.node.consensus();
            let genesis_hash = format!("{:x}", consensus.genesis_hash);
            format!("/{}/{}", consensus.id, &genesis_hash[..8])
        };
        let client_version = &self.client_version;
        let listening_addresses = Vec::new();
        let observed_address = context.session.address.clone();
        let message = build_identify_message(
            &network_identifier,
            &client_version,
            listening_addresses,
            observed_address,
        );
        context.send_message(message.as_bytes()).unwrap();
    }

    fn insert_ipinfo(&mut self, ip: &str) {
        if self.known_ips.contains(ip) {
            return;
        }

        if let Ok(ipinfo::IpDetails {
            ip,
            country,
            city,
            region,
            company,
            ..
        }) = lookup_ipinfo(ip)
        {
            let entry = crate::entry::IpInfo {
                network: self.node.consensus().id.clone(),
                ip,
                country,
                city,
                region,
                company: company.map(|company| company.name).unwrap_or_default(),
            };
            let raw_query = format!(
                "INSERT INTO {}.ipinfo(ip, country, city, region, company) \
                VALUES ('{}', '{}', '{}', '{}', '{}') ON CONFLICT DO NOTHING",
                entry.network, entry.ip, entry.country, entry.city, entry.region, entry.company,
            );
            self.known_ips.insert(entry.ip);
            self.query_sender.send(raw_query).unwrap();
        }
    }

    fn insert_compact_block_first_seen(&mut self, ip: Ip, block: &packed::CompactBlock) {
        let block_number = block.header().raw().number().unpack();
        let block_hash = block.header().calc_header_hash();
        let compact_blocks = self.compact_blocks.as_mut().unwrap();
        if !compact_blocks.contains(&block_hash) {
            compact_blocks.put(block_hash, ip.clone());

            let entry = crate::entry::CompactBlockFirstSeen {
                network: self.node.consensus().id.to_string(),
                time: chrono::Utc::now().naive_utc(),
                block_number,
                ip,
            };
            let raw_query = format!(
                "INSERT INTO {}.compact_block_first_seen(time, block_number, ip) VALUES ('{}', {}, '{}')",
                entry.network, entry.time, entry.block_number, entry.ip
            );
            self.query_sender.send(raw_query).unwrap();
        }
    }

    fn update_peer_last_compact_block(&self, ip: Ip, block: &packed::CompactBlock) {
        let block_number = block.header().raw().number().unpack();
        let block_hash = block.header().calc_header_hash();
        let entry = crate::entry::PeerLastCompactBlock {
            network: self.node.consensus().id.clone(),
            ip,
            block_number,
            block_hash,
            time: chrono::Utc::now().naive_utc(),
        };
        let raw_query = format!(
            "INSERT INTO {}.peer_last_compact_block (ip, block_number, block_hash, time) \
            VALUES ('{}', {}, '{:#x}', '{}') \
            ON CONFLICT ( ip ) \
            DO UPDATE SET (block_number, block_hash, time) = (EXCLUDED.block_number, EXCLUDED.block_hash, EXCLUDED.time)",
            entry.network, entry.ip, entry.block_number, entry.block_hash, entry.time,
        );
        self.query_sender.send(raw_query).unwrap();
    }
}

impl P2PServiceProtocol for CompactBlockCrawler {
    fn init(&mut self, context: &mut P2PProtocolContext) {
        if context.proto_id == SupportProtocols::Sync.protocol_id() {
            context
                .set_service_notify(
                    SupportProtocols::Sync.protocol_id(),
                    DIAL_ONLINE_ADDRESSES_INTERVAL,
                    DIAL_ONLINE_ADDRESSES_TOKEN,
                )
                .unwrap();
        }
        if context.proto_id == SupportProtocols::Relay.protocol_id()
            || context.proto_id == SupportProtocols::RelayV2.protocol_id()
        {
            self.compact_blocks = Some(LruCache::new(NonZeroUsize::new(2000).unwrap()));
        }
    }

    fn notify(&mut self, context: &mut P2PProtocolContext, token: u64) {
        match token {
            DIAL_ONLINE_ADDRESSES_TOKEN => {
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
            _ => unreachable!(),
        }
    }

    fn connected(&mut self, context: P2PProtocolContextMutRef, protocol_version: &str) {
        ckb_testkit::debug!(
            "CompactBlockCrawler open protocol, protocol_name: {} address: {}",
            context.protocols().get(&context.proto_id()).unwrap().name,
            context.session.address
        );
        if let Ok(mut shared) = self.shared.write() {
            shared.add_protocol(context.session, context.proto_id);
        }

        if context.proto_id() == SupportProtocols::Discovery.protocol_id() {
            self.connected_discovery(context, protocol_version)
        } else if context.proto_id() == SupportProtocols::Identify.protocol_id() {
            self.connected_identify(context, protocol_version)
        }
    }

    fn disconnected(&mut self, context: P2PProtocolContextMutRef) {
        ckb_testkit::debug!(
            "NetworkCrawler close protocol, protocol_name: {}, address: {}",
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
        } else if context.proto_id() == SupportProtocols::Relay.protocol_id()
            || context.proto_id() == SupportProtocols::RelayV2.protocol_id()
        {
            self.received_relay(context, data)
        }
    }
}

impl P2PServiceHandle for CompactBlockCrawler {
    fn handle_error(&mut self, _context: &mut P2PServiceContext, error: P2PServiceError) {
        match &error {
            P2PServiceError::DialerError { .. } | P2PServiceError::ProtocolSelectError { .. } => {
                // discard
            }
            _ => {
                ckb_testkit::error!(
                    "CompactBlockCrawler detect service error, error: {:?}",
                    error
                );
            }
        }
    }

    /// Handling session establishment and disconnection events
    fn handle_event(&mut self, _context: &mut P2PServiceContext, event: P2PServiceEvent) {
        match event {
            P2PServiceEvent::SessionOpen {
                session_context: session,
            } => {
                ckb_testkit::debug!("CompactBlockCrawler open session: {:?}", session);
                let _add = self
                    .shared
                    .write()
                    .map(|mut shared| shared.add_session(session.as_ref().to_owned()));
            }
            P2PServiceEvent::SessionClose {
                session_context: session,
            } => {
                ckb_testkit::debug!("CompactBlockCrawler close session: {:?}", session);
                let _remove = self
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
