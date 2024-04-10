use anyhow::Result;
use anyhow::{anyhow, Context};
use indoc::indoc;
use std::env;
use url::Url;

pub mod output_assert;

pub(crate) fn read_rpc_url() -> Result<Url> {
    let rpc_url = env::var("RPC_URL")
        .with_context(|| indoc! (
            r#"
            The required environmental variable `RPC_URL` is not set. Please set it manually or in .cargo/config.toml file:

            [env]
            RPC_URL = https://example.com/rpc/v0_7
            "#
        )
    )?;

    Url::parse(&rpc_url).with_context(|| {
        format!(
            "Failed to parse the URL from the `RPC_URL` environmental variable: {}",
            rpc_url
        )
    })
}

pub(crate) fn read_rpc_url_with_no_version() -> Result<Url> {
    let mut url = read_rpc_url().unwrap();
    match url.path_segments_mut() {
        Ok(mut path) => {
            path.clear();
        }
        Err(_) => {
            // TODO maybe add here `", got {}", &url`
            return Err(anyhow!(
                "Make sure that `RPC_URL` environmental variable is a valid node address"
            ));
        }
    }

    url.set_query(None);

    Ok(url)
}
