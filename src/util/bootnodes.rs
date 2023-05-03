use ckb_testkit::Node;
use p2p::multiaddr::Multiaddr;
use std::collections::HashSet;
use crate::topic::CKBNetworkType;

#[allow(clippy::mutable_key_type)]
pub fn bootnodes(network: CKBNetworkType) -> HashSet<Multiaddr> {
    let bootnode = match network {
        CKBNetworkType::Mirana => {
            "/ip4/47.110.15.57/tcp/8114/p2p/QmXS4Kbc9HEeykHUTJCm2tNmqghbvWyYpUp6BtE5b6VrAU"
        },
        CKBNetworkType::Pudge => {
            "/ip4/47.111.169.36/tcp/8111/p2p/QmNQ4jky6uVqLDrPU7snqxARuNGWNLgSrTnssbRuy3ij2W"
        }
        CKBNetworkType::Dev => {
            // Use local node
            "/ip4/127.0.0.1/tcp/8114"
        },
        _ => unreachable!(),
    };
    let mut bootnodes = HashSet::new();
    bootnodes.insert(bootnode.parse().unwrap());
    bootnodes
}
