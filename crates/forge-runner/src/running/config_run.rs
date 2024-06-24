use super::helpers::{create_entry_code, get_assembled_program, run_casm_program};
use crate::{
    package_tests::TestDetails,
    running::{build_syscall_handler, create_hints_dict, get_syscall_segment_index},
};
use anyhow::Result;
use blockifier::{
    blockifier::block::{BlockInfo, GasPrices},
    state::{
        cached_state::{CachedState, GlobalContractCache, GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST},
        state_api::StateReader,
    },
};
use cairo_felt::Felt252;
use cairo_lang_runner::SierraCasmRunner;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use cheatnet::runtime_extensions::forge_config_extension::{
    config::RawForgeConfig, ForgeConfigExtension,
};
use runtime::{starknet::context::build_context, ExtendedRuntime, StarknetRuntime};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use std::{default::Default, num::NonZeroU128};
use universal_sierra_compiler_api::AssembledProgramWithDebugInfo;

struct FakeStateReader;

impl StateReader for FakeStateReader {
    fn get_class_hash_at(
        &self,
        _contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::ClassHash> {
        unreachable!()
    }
    fn get_compiled_class_hash(
        &self,
        _class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::CompiledClassHash> {
        unreachable!()
    }
    fn get_compiled_contract_class(
        &self,
        _class_hash: starknet_api::core::ClassHash,
    ) -> blockifier::state::state_api::StateResult<
        blockifier::execution::contract_class::ContractClass,
    > {
        unreachable!()
    }
    fn get_nonce_at(
        &self,
        _contract_address: starknet_api::core::ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::Nonce> {
        unreachable!()
    }
    fn get_storage_at(
        &self,
        _contract_address: starknet_api::core::ContractAddress,
        _key: starknet_api::state::StorageKey,
    ) -> blockifier::state::state_api::StateResult<starknet_api::hash::StarkFelt> {
        unreachable!()
    }
}

#[allow(clippy::too_many_lines)]
pub fn run_config_pass(
    args: Vec<Felt252>,
    test_details: &TestDetails,
    casm_program: &AssembledProgramWithDebugInfo,
) -> Result<RawForgeConfig> {
    let mut cached_state = CachedState::new(
        FakeStateReader,
        GlobalContractCache::new(GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST),
    );
    let block_info = BlockInfo {
        block_number: BlockNumber(0),
        block_timestamp: BlockTimestamp(0),
        gas_prices: GasPrices {
            eth_l1_data_gas_price: NonZeroU128::new(2).unwrap(),
            eth_l1_gas_price: NonZeroU128::new(2).unwrap(),
            strk_l1_data_gas_price: NonZeroU128::new(2).unwrap(),
            strk_l1_gas_price: NonZeroU128::new(2).unwrap(),
        },
        sequencer_address: 0_u8.into(),
        use_kzg_da: true,
    };
    let (entry_code, builtins) = create_entry_code(args, test_details, casm_program);
    let footer = SierraCasmRunner::create_code_footer();

    let assembled_program = get_assembled_program(casm_program, entry_code, footer);

    let (string_to_hint, hints_dict) = create_hints_dict(&assembled_program);

    let mut context = build_context(&block_info);

    let mut execution_resources = ExecutionResources::default();

    let syscall_handler = build_syscall_handler(
        &mut cached_state,
        &string_to_hint,
        &mut execution_resources,
        &mut context,
        get_syscall_segment_index(&test_details.parameter_types),
    );

    let mut config = RawForgeConfig::default();

    let mut forge_config_runtime = ExtendedRuntime {
        extension: ForgeConfigExtension {
            config: &mut config,
        },
        extended_runtime: StarknetRuntime {
            hint_handler: syscall_handler,
        },
    };

    run_casm_program(
        &assembled_program,
        builtins,
        hints_dict,
        &mut forge_config_runtime,
    )?;

    Ok(config)
}
