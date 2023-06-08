use crate::ckb_types::{
    bytes::Bytes,
    core::ScriptHashType,
    packed::{Byte32, OutPoint, ProposalShortId},
};

/// Peer info
#[derive(Clone, Debug)]
pub struct Peer {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub version: String,
    pub ip: String,
    pub n_reachable: i32,
    pub address: String,
    pub node_type: u8,
}

/// IP info
#[derive(Clone, Debug)]
pub struct IpInfo {
    pub network: String,
    pub ip: String,
    pub country: String,
    pub city: String,
    pub region: String,
    pub company: String,
}

/// Block Info
#[derive(Clone, Debug)]
pub struct Block {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub number: i64,
    pub n_transactions: i32,
    pub n_proposals: i32,
    pub n_uncles: i32,
    pub cellbase_client_version: String,
    pub cellbase_miner_source: String,
    pub miner_lock_args: String,
    pub interval: i64, // ms
    pub hash: Byte32,
}

/// Epoch Info
#[derive(Clone, Debug)]
pub struct Epoch {
    pub network: String,
    pub start_time: chrono::NaiveDateTime,
    pub end_time: chrono::NaiveDateTime,
    pub number: u64,
    pub length: u64,
    pub start_number: u64,
    pub n_uncles: i32,
    pub difficulty: String,
}

/// Tx Pool Info
#[derive(Clone, Debug)]
pub struct TxPoolInfo {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub total_tx_cycles: i64,
    pub total_tx_size: i64,
    pub pending: i64,
    pub proposed: i64,
    pub orphan: i64,
}

/// Block Transaction Info
#[derive(Clone, Debug)]
pub struct BlockTransaction {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub number: i64,
    pub size: i32,
    pub n_inputs: i32,
    pub n_outputs: i32,
    pub n_header_deps: i32,
    pub n_cell_deps: i32,
    pub total_data_size: i32,
    pub proposal_id: String,
    pub hash: String,
}

/// Subscribed New Transaction Info
#[derive(Clone, Debug)]
pub struct SubscribedNewTransaction {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub size: u64,
    pub cycles: u64,
    pub fee: u64,
    pub n_inputs: usize,
    pub n_outputs: usize,
    pub n_header_deps: usize,
    pub n_cell_deps: usize,
    pub proposal_id: ProposalShortId,
    pub hash: Byte32,
}

/// Subscribed Proposed Transaction Info
#[derive(Clone, Debug)]
pub struct SubscribedProposedTransaction {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub size: u64,
    pub cycles: u64,
    pub fee: u64,
    pub n_inputs: usize,
    pub n_outputs: usize,
    pub n_header_deps: usize,
    pub n_cell_deps: usize,
    pub proposal_id: ProposalShortId,
    pub hash: Byte32,
}

/// Subscribed Rejected Transaction Info
#[derive(Clone, Debug)]
pub struct SubscribedRejectedTransaction {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub reason: String,
    pub size: u64,
    pub cycles: u64,
    pub fee: u64,
    pub n_inputs: usize,
    pub n_outputs: usize,
    pub n_header_deps: usize,
    pub n_cell_deps: usize,
    pub proposal_id: ProposalShortId,
    pub hash: Byte32,
}

/// Retention Transaction Info
#[derive(Clone, Debug)]
pub struct RetentionTransaction {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub hash: Byte32,
}

/// Cell Info
#[derive(Clone, Debug)]
pub struct CreatedCell {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub block_number: u64,
    pub tx_index: usize,
    pub out_point: OutPoint,

    pub lock_hash_type: ScriptHashType,
    pub lock_code_hash: Byte32,
    pub lock_args: Option<Bytes>,
    pub type_hash_type: Option<ScriptHashType>,
    pub type_code_hash: Option<Byte32>,
}

#[derive(Clone, Debug)]
pub struct SpentCell {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub block_number: u64,
    pub out_point: OutPoint,
}

/// Compact block first received from
#[derive(Clone, Debug)]
pub struct CompactBlockFirstSeen {
    pub network: String,
    pub time: chrono::NaiveDateTime,
    pub block_number: u64,
    pub ip: String,
}

/// Peer's last sent compact block
///
/// Note: This table is not time-serie. It should be indexed by ip and keep update in place.
#[derive(Clone, Debug)]
pub struct PeerLastCompactBlock {
    pub network: String,
    pub ip: String,
    pub block_hash: Byte32,
    pub block_number: u64,
    pub time: chrono::NaiveDateTime,
}
