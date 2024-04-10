use crate::test_utils::{read_rpc_url, read_rpc_url_with_no_version};
use lazy_static::lazy_static;
use url::Url;

pub const EXPECTED_RPC_VERSION: &str = "0.7.0";

lazy_static! {
    pub static ref RPC_URL: Url = read_rpc_url().unwrap();
    pub static ref RPC_URL_WITH_NO_VERSION: Url = read_rpc_url_with_no_version().unwrap();
}
