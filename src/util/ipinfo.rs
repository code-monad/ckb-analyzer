use ipinfo::{IpDetails, IpError, IpInfo};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    static ref IPINFO: Mutex<IpInfo> = {
        let ipinfo_io_token = match ::std::env::var("IPINFO_IO_TOKEN") {
            Ok(token) if !token.is_empty() => Some(token),
            _ => {
                log::warn!("Miss environment variable \"IPINFO_IO_TOKEN\", use empty value");
                None
            }
        };
        let ipinfo = ipinfo::IpInfo::new(ipinfo::IpInfoConfig {
            token: ipinfo_io_token,
            cache_size: 10000,
            timeout: ::std::time::Duration::from_secs(365 * 24 * 60 * 60),
            ..Default::default()
        })
        .expect("Connect to https://ipinfo.io");
        Mutex::new(ipinfo)
    };
    static ref IPINFO_CACHE: Mutex<HashMap<String, IpDetails>> = Mutex::new(Default::default());
}

pub fn lookup_ipinfo(ip: &str) -> Result<IpDetails, IpError> {
    if let Ok(cache) = IPINFO_CACHE.lock() {
        if let Some(ipdetails) = cache.get(&ip.to_string()) {
            return Ok(ipdetails.clone());
        }
    }

    if let Ok(mut ipinfo) = IPINFO.lock() {
        match ipinfo.lookup(&[ip]) {
            Ok(ipdetails) => {
                if let Ok(mut cache) = IPINFO_CACHE.lock() {
                    cache.insert(ip.to_string(), ipdetails[ip].to_owned());
                }

                return Ok(ipdetails[ip].to_owned());
            }
            Err(err) => return Err(err),
        }
    }

    unreachable!()
}

pub mod test {
    #[test]
    #[ignore] // This case needs env var "IPINFO_IO_TOKEN"
    fn test_lookup_ipinfo_cache() {
        use crate::util::ipinfo::{lookup_ipinfo, IPINFO_CACHE};
        {
            let cache = IPINFO_CACHE.lock().unwrap();
            assert!(cache.get("8.8.8.8").is_none());
        }

        let _ipdetails = lookup_ipinfo("8.8.8.8").unwrap();

        {
            let cache = IPINFO_CACHE.lock().unwrap();
            assert!(cache.get("8.8.8.8").is_some());
        }
    }
}
