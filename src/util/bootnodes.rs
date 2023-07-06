use ckb_testkit::Node;
use p2p::multiaddr::Multiaddr;
use std::collections::HashSet;
use crate::topic::CKBNetworkType;

#[allow(clippy::mutable_key_type)]
pub fn bootnodes(network: CKBNetworkType) -> HashSet<Multiaddr> {
    let bootnode = match network {
        CKBNetworkType::Mirana => {
            [
                "/ip4/47.110.15.57/tcp/8114/p2p/QmXS4Kbc9HEeykHUTJCm2tNmqghbvWyYpUp6BtE5b6VrAU",
                "/ip4/13.234.144.148/tcp/8114/p2p/QmbT7QimcrcD5k2znoJiWpxoESxang6z1Gy9wof1rT1LKR",
                "/ip4/104.208.105.55/tcp/8114/p2p/QmejugEABNzAofqRhci7HAipLFvoyYKRacd272jNtnQBTE",
                "/ip4/34.64.120.143/tcp/8114/p2p/QmejEJEbDcGGMp4D6WtftMMVLkR1ZuBfMgyLFDMJymkDt6",
                "/ip4/3.218.170.86/tcp/8114/p2p/QmShw2vtVt49wJagc1zGQXGS6LkQTcHxnEV3xs6y8MAmQN",
                "/ip4/35.236.107.161/tcp/8114/p2p/QmSRj57aa9sR2AiTvMyrEea8n1sEM1cDTrfb2VHVJxnGuu",
                "/ip4/23.101.191.12/tcp/8114/p2p/QmexvXVDiRt2FBGptgK4gBJusWyyTEEaHeuCAa35EPNkZS",
                "/ip4/13.37.172.80/tcp/8114/p2p/QmXJg4iKbQzMpLhX75RyDn89Mv7N2H8vLePBR7kgZf6hYk",
                "/ip4/34.118.49.255/tcp/8114/p2p/QmeCzzVmSAU5LNYAeXhdJj8TCq335aJMqUxcvZXERBWdgS",
                "/ip4/40.115.75.216/tcp/8114/p2p/QmW3P1WYtuz9hitqctKnRZua2deHXhNePNjvtc9Qjnwp4q",
            ].to_vec()
        },
        CKBNetworkType::Pudge => {
            [
                "/ip4/47.111.169.36/tcp/8111/p2p/QmNQ4jky6uVqLDrPU7snqxARuNGWNLgSrTnssbRuy3ij2W",
                "/ip4/35.176.207.239/tcp/8111/p2p/QmSJTsMsMGBjzv1oBNwQU36VhQRxc2WQpFoRu1ZifYKrjZ",
                "/ip4/18.136.60.221/tcp/8111/p2p/QmTt6HeNakL8Fpmevrhdna7J4NzEMf9pLchf1CXtmtSrwb",
                "/ip4/47.74.66.72/tcp/8111/p2p/QmPhgweKm2ciYq52LjtEDmKFqHxGcg2WQ8RLCayRRycanD",
                "/ip4/47.254.175.152/tcp/8111/p2p/QmXw7RsAR9bghvW4LrjrVBEwTMbdnpTEdWEtZSVQFpUgqU",
                "/ip4/47.245.29.58/tcp/8111/p2p/QmYWiwxHasuyou5ztw5uQkHbhm6gs6RB84sbgYxZNGHgaW",
                "/ip4/47.254.234.14/tcp/8111/p2p/QmfUJGvgXRTM12rSFScXy9GyD3ssuLVSpvscP5nngbhNbU",
                "/ip4/47.89.252.15/tcp/8111/p2p/QmRcUV32qumGrhkTXCWXrMhwGZLKZTmuFzLWacMpSZJ3n9",
                "/ip4/39.104.177.87/tcp/8111/p2p/QmQ27jbnww6deXQiv7SmYAUBPA1S3vGcqP9aRsXa4VaXEi",
                "/ip4/13.228.149.113/tcp/8111/p2p/QmQoTR39rBkpZVgLApDGDoFnJ2YDBS9hYeiib1Z6aoAdEf",
            ].to_vec()
        }
        CKBNetworkType::Dev => {
            // Use local node
            ["/ip4/127.0.0.1/tcp/8114"].to_vec()
        },
        _ => unreachable!(),
    };
    let mut bootnodes: HashSet<Multiaddr> = HashSet::new();
    for addr in bootnode {
        bootnodes.insert(addr.parse().unwrap());
    }
    bootnodes
}
