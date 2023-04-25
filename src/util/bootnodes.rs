use ckb_testkit::Node;
use p2p::multiaddr::Multiaddr;
use std::collections::HashSet;

#[allow(clippy::mutable_key_type)]
pub fn bootnodes(node: &Node) -> HashSet<Multiaddr> {
    let local_node_info = node.rpc_client().local_node_info();
    if !local_node_info.addresses.is_empty() {
        return local_node_info
            .addresses
            .into_iter()
            .map(|addr| addr.address.parse().unwrap())
            .collect();
    }

    let bootnode = match node.consensus().id.as_str() {
        "ckb" => "/ip4/47.110.15.57/tcp/8114/p2p/QmXS4Kbc9HEeykHUTJCm2tNmqghbvWyYpUp6BtE5b6VrAU",
        "ckb_testnet" => {
            "/ip4/47.111.169.36/tcp/8111/p2p/QmNQ4jky6uVqLDrPU7snqxARuNGWNLgSrTnssbRuy3ij2W"
        },
        "dev" => {
            "/ip4/127.0.0.1/tcp/8115"
        },
        _ => unreachable!(),
    };
    let mut bootnodes = HashSet::new();
    bootnodes.insert(bootnode.parse().unwrap());
    bootnodes
}
