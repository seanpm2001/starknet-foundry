use crate::cairo_runner::sierra_casm_runner::create_metadata;
use crate::fuzzer::RandomFuzzer;
use crate::printing::print_test_result;
use crate::running::{run_fuzz_test, run_test, TestDetails};
use crate::test_case_summary::TestCaseSummary;
use crate::test_crate_summary::TestCrateSummary;
use anyhow::{anyhow, bail, Result};
use cairo_felt::Felt252;
use cairo_lang_sierra::extensions::core::{CoreLibfunc, CoreType};
use cairo_lang_sierra::ids::ConcreteTypeId;
use cairo_lang_sierra::program::{Function, Program};
use cairo_lang_sierra::program_registry::ProgramRegistry;
use cairo_lang_sierra_to_casm::compiler::CairoProgram;
use cairo_lang_sierra_to_casm::metadata::MetadataComputationConfig;
use cairo_lang_sierra_type_size::get_type_size_map;
use camino::Utf8PathBuf;
use conversions::IntoConv;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use num_bigint::BigInt;
use once_cell::sync::Lazy;
use scarb_artifacts::StarknetContractArtifacts;
use smol_str::SmolStr;
use starknet::core::types::BlockId;
use starknet::core::types::BlockTag::Latest;
use std::collections::HashMap;
use std::sync::Arc;
use test_case_summary::FuzzingGasUsage;
use test_collector::{ExpectedTestResult, FuzzerConfig};
use test_collector::{LinkedLibrary, RawForkParams};
use tokio::sync::mpsc::{channel, Sender};
use tokio::task::JoinHandle;
use url::Url;

pub mod test_case_summary;
pub mod test_crate_summary;

mod cairo_runner;
mod fuzzer;
mod gas;
mod printing;
mod running;

pub const CACHE_DIR: &str = ".snfoundry_cache";

pub static BUILTINS: Lazy<Vec<&str>> = Lazy::new(|| {
    vec![
        "Pedersen",
        "RangeCheck",
        "Bitwise",
        "EcOp",
        "Poseidon",
        "SegmentArena",
        "GasBuiltin",
        "System",
    ]
});

/// Configuration of the test runner
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct RunnerConfig {
    pub workspace_root: Utf8PathBuf,
    pub exit_first: bool,
    pub fuzzer_runs: u32,
    pub fuzzer_seed: u64,
}

impl RunnerConfig {
    #[must_use]
    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
    pub fn new(
        workspace_root: Utf8PathBuf,
        exit_first: bool,
        fuzzer_runs: u32,
        fuzzer_seed: u64,
    ) -> Self {
        Self {
            workspace_root,
            exit_first,
            fuzzer_runs,
            fuzzer_seed,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct RunnerParams {
    corelib_path: Utf8PathBuf,
    contracts: HashMap<String, StarknetContractArtifacts>,
    environment_variables: HashMap<String, String>,
    linked_libraries: Vec<LinkedLibrary>,
}

impl RunnerParams {
    #[must_use]
    pub fn new(
        corelib_path: Utf8PathBuf,
        contracts: HashMap<String, StarknetContractArtifacts>,
        environment_variables: HashMap<String, String>,
        linked_libraries: Vec<LinkedLibrary>,
    ) -> Self {
        Self {
            corelib_path,
            contracts,
            environment_variables,
            linked_libraries,
        }
    }

    #[must_use]
    pub fn linked_libraries(&self) -> &Vec<LinkedLibrary> {
        &self.linked_libraries
    }
    #[must_use]
    pub fn corelib_path(&self) -> &Utf8PathBuf {
        &self.corelib_path
    }
}

/// Exit status of the runner
#[derive(Debug, PartialEq, Clone)]
#[non_exhaustive]
pub enum RunnerStatus {
    /// Runner exited without problems
    Default,
    /// Some test failed
    TestFailed,
    /// Runner did not run, e.g. when test cases got skipped
    DidNotRun,
}

#[derive(Debug, Clone)]
pub struct TestCaseRunnable {
    pub name: String,
    pub available_gas: Option<usize>,
    pub ignored: bool,
    pub expected_result: ExpectedTestResult,
    pub fork_config: Option<ValidatedForkConfig>,
    pub fuzzer_config: Option<FuzzerConfig>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidatedForkConfig {
    url: Url,
    block_id: BlockId,
}

impl ValidatedForkConfig {
    #[must_use]
    pub fn new(url: Url, block_id: BlockId) -> Self {
        Self { url, block_id }
    }
}

impl TryFrom<RawForkParams> for ValidatedForkConfig {
    type Error = anyhow::Error;

    fn try_from(value: RawForkParams) -> Result<Self, Self::Error> {
        let block_id = match value.block_id_type.to_lowercase().as_str() {
            "number" => BlockId::Number(value.block_id_value.parse().unwrap()),
            "hash" => BlockId::Hash(
                Felt252::from(value.block_id_value.parse::<BigInt>().unwrap()).into_(),
            ),
            "tag" => {
                assert_eq!(value.block_id_value, "Latest");
                BlockId::Tag(Latest)
            }
            value => bail!("Invalid value passed for block_id = {value}"),
        };
        Ok(ValidatedForkConfig {
            url: value.url.parse()?,
            block_id,
        })
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompiledTestCrateRunnable {
    sierra_program: Program,
    test_cases: Vec<TestCaseRunnable>,
}

impl CompiledTestCrateRunnable {
    #[must_use]
    pub fn new(sierra_program: Program, test_cases: Vec<TestCaseRunnable>) -> Self {
        Self {
            sierra_program,
            test_cases,
        }
    }
}

pub trait TestCaseFilter {
    fn should_be_run(&self, test_case: &TestCaseRunnable) -> bool;
}

#[non_exhaustive]
pub enum TestCrateRunResult {
    Ok(TestCrateSummary),
    Interrupted(TestCrateSummary),
}

/// This will be removed once we migrate to outputting casm + details from the test collector in scarb.
fn build_test_details(test_name: &str, sierra_program: &Program) -> TestDetails {
    let sierra_program_registry =
        ProgramRegistry::<CoreType, CoreLibfunc>::new(sierra_program).unwrap();
    let type_sizes = get_type_size_map(sierra_program, &sierra_program_registry).unwrap();
    let func = sierra_program
        .funcs
        .iter()
        .find(|f| f.id.debug_name.clone().unwrap().ends_with(test_name))
        .unwrap();
    let parameter_types = func
        .signature
        .param_types
        .iter()
        .map(|pt| {
            let td = sierra_program
                .type_declarations
                .iter()
                .find(|td| &td.id == pt)
                .unwrap();
            let generic_id = &td.long_id.generic_id;
            let size = type_sizes[&td.id];
            (generic_id.clone(), size)
        })
        .collect::<Vec<_>>();
    // dbg!(&func.signature.ret_types);
    let return_types = func
        .signature
        .ret_types
        .iter()
        .map(|pt| {
            let td = sierra_program
                .type_declarations
                .iter()
                .find(|td| &td.id == pt)
                .unwrap();
            let generic_id = &td.long_id.generic_id;
            let size = type_sizes[&td.id];
            (generic_id.clone(), size)
        })
        .collect::<Vec<_>>();

    TestDetails {
        entry_point_offset: func.entry_point.0,
        parameter_types,
        return_types,
    }
}

pub async fn run_tests_from_crate(
    tests: Arc<CompiledTestCrateRunnable>,
    runner_config: Arc<RunnerConfig>,
    runner_params: Arc<RunnerParams>,
    tests_filter: &impl TestCaseFilter,
) -> Result<TestCrateRunResult> {
    let sierra_program = tests.sierra_program.clone();
    let metadata_config = Some(MetadataComputationConfig::default());
    let gas_usage_check = metadata_config.is_some();
    let metadata = create_metadata(&sierra_program, metadata_config).unwrap();
    let casm_program =
        cairo_lang_sierra_to_casm::compiler::compile(&sierra_program, &metadata, gas_usage_check)
            .unwrap();

    // let runner = SierraCasmRunner::new(casm_program).context("Failed setting up runner.")?;
    let casm_program = Arc::new(casm_program);

    let mut tasks = FuturesUnordered::new();
    let test_cases = &tests.test_cases;
    // Initiate two channels to manage the `--exit-first` flag.
    // Owing to `cheatnet` fork's utilization of its own Tokio runtime for RPC requests,
    // test execution must occur within a `tokio::spawn_blocking`.
    // As `spawn_blocking` can't be prematurely cancelled (refer: https://dtantsur.github.io/rust-openstack/tokio/task/fn.spawn_blocking.html),
    // a channel is used to signal the task that test processing is no longer necessary.
    let (send, mut rec) = channel(1);

    for case in test_cases {
        let case_name = case.name.clone();

        if !tests_filter.should_be_run(case) {
            tasks.push(tokio::task::spawn(async {
                Ok(TestCaseSummary::Ignored { name: case_name })
            }));
            continue;
        };

        let function = sierra_program
            .funcs
            .iter()
            .find(|f| f.id.debug_name.clone().unwrap().ends_with(&case_name))
            .unwrap();
        let args = function_args(function, &BUILTINS);

        let case = Arc::new(case.clone());
        let args: Vec<ConcreteTypeId> = args.into_iter().cloned().collect();
        let test_details = Arc::new(build_test_details(&case.name, &sierra_program));

        tasks.push(choose_test_strategy_and_run(
            args,
            case.clone(),
            casm_program.clone(),
            test_details.clone(),
            runner_config.clone(),
            runner_params.clone(),
            &send,
        ));
    }

    let mut results = vec![];
    let mut interrupted = false;

    while let Some(task) = tasks.next().await {
        let result = task??;

        print_test_result(&result);

        if let TestCaseSummary::Failed { .. } = result {
            if runner_config.exit_first {
                interrupted = true;
                rec.close();
            }
        }

        results.push(result);
    }

    let contained_fuzzed_tests = results.iter().any(|summary| summary.runs().is_some());
    let summary = TestCrateSummary {
        test_case_summaries: results,
        runner_exit_status: RunnerStatus::Default,
        contained_fuzzed_tests,
    };

    if interrupted {
        Ok(TestCrateRunResult::Interrupted(summary))
    } else {
        Ok(TestCrateRunResult::Ok(summary))
    }
}

#[allow(clippy::too_many_arguments)]
fn choose_test_strategy_and_run(
    args: Vec<ConcreteTypeId>,
    case: Arc<TestCaseRunnable>,
    casm_program: Arc<CairoProgram>,
    test_details: Arc<TestDetails>,
    runner_config: Arc<RunnerConfig>,
    runner_params: Arc<RunnerParams>,
    send: &Sender<()>,
) -> JoinHandle<Result<TestCaseSummary>> {
    if args.is_empty() {
        run_test(
            case,
            casm_program,
            test_details,
            runner_config,
            runner_params,
            send.clone(),
        )
    } else {
        run_with_fuzzing(
            args,
            case,
            casm_program,
            test_details,
            runner_config,
            runner_params,
            send.clone(),
        )
    }
}

// TODO remove
#[allow(clippy::too_many_arguments)]
fn run_with_fuzzing(
    args: Vec<ConcreteTypeId>,
    case: Arc<TestCaseRunnable>,
    casm_program: Arc<CairoProgram>,
    test_details: Arc<TestDetails>,
    runner_config: Arc<RunnerConfig>,
    runner_params: Arc<RunnerParams>,
    send: Sender<()>,
) -> JoinHandle<Result<TestCaseSummary>> {
    tokio::task::spawn(async move {
        if send.is_closed() {
            return Ok(TestCaseSummary::Skipped {});
        }

        let (fuzzing_send, mut fuzzing_rec) = channel(1);
        let args = args
            .iter()
            .map(|arg| {
                arg.debug_name
                    .as_ref()
                    .ok_or_else(|| anyhow!("Type {arg:?} does not have a debug name"))
                    .map(SmolStr::as_str)
            })
            .collect::<Result<Vec<_>>>()?;

        let (fuzzer_runs, fuzzer_seed) = match case.fuzzer_config {
            Some(FuzzerConfig {
                fuzzer_runs,
                fuzzer_seed,
            }) => (fuzzer_runs, fuzzer_seed),
            _ => (runner_config.fuzzer_runs, runner_config.fuzzer_seed),
        };
        let mut fuzzer = RandomFuzzer::create(fuzzer_seed, fuzzer_runs, &args)?;

        let mut tasks = FuturesUnordered::new();

        for _ in 1..=fuzzer_runs {
            let args = fuzzer.next_args();

            tasks.push(run_fuzz_test(
                args,
                case.clone(),
                casm_program.clone(),
                test_details.clone(),
                runner_config.clone(),
                runner_params.clone(),
                send.clone(),
                fuzzing_send.clone(),
            ));
        }

        let mut results = vec![];
        let mut gas_usages = None;
        while let Some(task) = tasks.next().await {
            let result = task??;

            results.push(result.clone());

            if let TestCaseSummary::Failed { .. } = result {
                fuzzing_rec.close();
                break;
            }
        }

        let final_result = results
            .last()
            .expect("Test should always run at least once");

        let runs = u32::try_from(
            results
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        TestCaseSummary::Passed { .. } | TestCaseSummary::Failed { .. }
                    )
                })
                .count(),
        )?;

        if let TestCaseSummary::Passed { .. } = final_result {
            // Because we execute tests parallel, it's possible to
            // get Passed after Skipped. To treat fuzzing a test as Passed
            // we have to ensure that all fuzzing subtests Passed
            if runs != fuzzer_runs {
                return Ok(TestCaseSummary::Skipped {});
            };

            let gas_usages_vec: Vec<&f64> = results
                .iter()
                .filter(|item| matches!(item, TestCaseSummary::Passed { .. }))
                .map(|a| match a {
                    TestCaseSummary::Passed { gas_used, .. } => gas_used,
                    _ => unreachable!(),
                })
                .collect();

            let max = gas_usages_vec
                .clone()
                .into_iter()
                .copied()
                .reduce(f64::max)
                .unwrap();
            let min = gas_usages_vec
                .into_iter()
                .copied()
                .reduce(f64::min)
                .unwrap();

            gas_usages = Some(FuzzingGasUsage { min, max });
        };

        Ok(final_result
            .clone()
            .with_runs_and_gas_usage(runs, gas_usages))
    })
}

fn function_args<'a>(function: &'a Function, builtins: &[&str]) -> Vec<&'a ConcreteTypeId> {
    let builtins: Vec<_> = builtins
        .iter()
        .map(|builtin| Some(SmolStr::new(builtin)))
        .collect();

    function
        .signature
        .param_types
        .iter()
        .filter(|pt| !builtins.contains(&pt.debug_name))
        .collect()
}
