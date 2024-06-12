use crate::starknet_commands::account::accounts_format::AccountData;
use crate::starknet_commands::account::add::Add;
use crate::starknet_commands::account::create::Create;
use crate::starknet_commands::account::delete::Delete;
use crate::starknet_commands::account::deploy::Deploy;
use crate::{chain_id_to_network_name, decode_chain_id, helpers::configuration::CastConfig};
use anyhow::{anyhow, bail, Context, Result};
use camino::Utf8PathBuf;
use clap::{Args, Subcommand};
use configuration::{
    find_config_file, load_global_config, search_config_upwards_relative_to, CONFIG_FILENAME,
};
use starknet::core::types::FieldElement;
use std::{fs::OpenOptions, io::Write};
use toml::Value;

pub mod account_factory;
pub mod accounts_format;
pub mod add;
pub mod create;
pub mod delete;
pub mod deploy;

#[derive(Args)]
#[command(about = "Creates and deploys an account to the Starknet")]
pub struct Account {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Add(Add),
    Create(Create),
    Deploy(Deploy),
    Delete(Delete),
}

#[allow(clippy::too_many_arguments)]
pub fn write_account_to_accounts_file(
    account: &str,
    accounts_file: &Utf8PathBuf,
    chain_id: FieldElement,
    account_data: &AccountData,
) -> Result<()> {
    let account_json = serde_json::to_value(account_data)?;
    if !accounts_file.exists() {
        std::fs::create_dir_all(accounts_file.clone().parent().unwrap())?;
        std::fs::write(accounts_file.clone(), "{}")?;
    }

    let contents = std::fs::read_to_string(accounts_file.clone())?;
    let mut items: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|_| anyhow!("Failed to parse accounts file at = {}", accounts_file))?;

    let network_name = chain_id_to_network_name(chain_id);

    if !items[&network_name][account].is_null() {
        bail!(
            "Account with name = {} already exists in network with chain_id = {}",
            account,
            decode_chain_id(chain_id)
        );
    }
    items[&network_name][account] = account_json;

    std::fs::write(
        accounts_file.clone(),
        serde_json::to_string_pretty(&items).unwrap(),
    )?;
    Ok(())
}

pub fn add_created_profile_to_configuration(
    profile: &Option<String>,
    cast_config: &CastConfig,
    path: &Option<Utf8PathBuf>,
) -> Result<()> {
    if !load_global_config::<CastConfig>(path, profile)
        .unwrap_or_default()
        .account
        .is_empty()
    {
        bail!(
            "Failed to add profile = {} to the snfoundry.toml. Profile already exists",
            profile.as_ref().unwrap_or(&"default".to_string())
        );
    }

    let toml_string = {
        let mut new_profile = toml::value::Table::new();

        new_profile.insert("url".to_string(), Value::String(cast_config.url.clone()));
        new_profile.insert(
            "account".to_string(),
            Value::String(cast_config.account.clone()),
        );
        if let Some(keystore) = cast_config.keystore.clone() {
            new_profile.insert("keystore".to_string(), Value::String(keystore.to_string()));
        } else {
            new_profile.insert(
                "accounts-file".to_string(),
                Value::String(cast_config.accounts_file.to_string()),
            );
        }
        let mut profile_config = toml::value::Table::new();
        profile_config.insert(
            profile
                .clone()
                .unwrap_or_else(|| cast_config.account.clone()),
            Value::Table(new_profile),
        );

        let mut sncast_config = toml::value::Table::new();
        sncast_config.insert(String::from("sncast"), Value::Table(profile_config));

        toml::to_string(&Value::Table(sncast_config)).context("Failed to convert toml to string")?
    };

    let config_path = match path.as_ref() {
        Some(p) => search_config_upwards_relative_to(p)?,
        None => find_config_file().unwrap_or(Utf8PathBuf::from(CONFIG_FILENAME)),
    };

    let mut snfoundry_toml = OpenOptions::new()
        .create(true)
        .append(true)
        .open(config_path)
        .context("Failed to open snfoundry.toml")?;
    snfoundry_toml
        .write_all(format!("\n{toml_string}").as_bytes())
        .context("Failed to write to the snfoundry.toml")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::helpers::configuration::CastConfig;
    use crate::helpers::constants::DEFAULT_ACCOUNTS_FILE;
    use camino::Utf8PathBuf;
    use configuration::copy_config_to_tempdir;
    use std::fs;

    use crate::starknet_commands::account::add_created_profile_to_configuration;

    #[test]
    fn test_add_created_profile_to_configuration_happy_case() {
        let tempdir =
            copy_config_to_tempdir("tests/data/files/correct_snfoundry.toml", None).unwrap();
        let path = Utf8PathBuf::try_from(tempdir.path().to_path_buf()).unwrap();
        let config = CastConfig {
            url: String::from("http://some-url"),
            account: String::from("some-name"),
            accounts_file: "accounts".into(),
            ..Default::default()
        };
        let res = add_created_profile_to_configuration(
            &Some(String::from("some-name")),
            &config,
            &Some(path.clone()),
        );
        assert!(res.is_ok());

        let contents =
            fs::read_to_string(path.join("snfoundry.toml")).expect("Failed to read snfoundry.toml");
        assert!(contents.contains("[sncast.some-name]"));
        assert!(contents.contains("account = \"some-name\""));
        assert!(contents.contains("url = \"http://some-url\""));
        assert!(contents.contains("accounts-file = \"accounts\""));
    }

    #[test]
    fn test_add_created_profile_to_configuration_profile_already_exists() {
        let tempdir =
            copy_config_to_tempdir("tests/data/files/correct_snfoundry.toml", None).unwrap();
        let config = CastConfig {
            url: String::from("http://127.0.0.1:5055/rpc"),
            account: String::from("user1"),
            accounts_file: DEFAULT_ACCOUNTS_FILE.into(),
            ..Default::default()
        };
        let res = add_created_profile_to_configuration(
            &Some(String::from("default")),
            &config,
            &Some(Utf8PathBuf::try_from(tempdir.path().to_path_buf()).unwrap()),
        );
        assert!(res.is_err());
    }
}
