# Environment Setup

> 💡 **Info**
> 
> This setup is for development of Starknet Foundry.
>
> If you don't wish to contribute, you can omit these instructions.

Install the latest stable [Rust](https://www.rust-lang.org/tools/install) version.
If you already have Rust installed make sure to upgrade it by running:

```shell
$ rustup update
```

To verify that project was cloned and set up correctly, you can run:

```shell
$ cargo check
```

## Running Tests

1. Run `./scripts/install_devnet.sh`
2. Set [Scarb](https://docs.swmansion.com/scarb/) version [compatible](https://github.com/foundry-rs/starknet-foundry/releases) with both `snforge` and `sncast`
3. Install the newest version of [universal-sierra-compiler](https://github.com/software-mansion/universal-sierra-compiler)
4. Set `RPC_URL` environmental variable to a Sepolia testnet node URL:
    - either manually in your shell
    ```shell
    $ export RPC_URL="https://example.com/rpc/v0_7" 
    ```
    - or inside [`.cargo/config.toml` file](https://doc.rust-lang.org/cargo/reference/config.html#configuration-format)
    ```toml
    [env]
    RPC_URL = "https://example.com/rpc/v0_7"
    ```
5. (CI tests) [Set a secret variable](https://docs.github.com/en/actions/security-guides/using-secrets-in-github-actions#creating-secrets-for-a-repository)
   `RPC_URL` in your ``starknet-foundry`` fork repository

After performing these steps, you can run tests with:
```shell
$ cargo test
``` 

> 💡 **Info**
>
> Please make sure you're using scarb installed via [`asdf`](https://asdf-vm.com/) - otherwise some tests may fail.
> To verify, run:
> 
> ```shell
> $ which scarb
> $HOME/.asdf/shims/scarb
> ```
> 
> If you previously installed Scarb using official installer, you may need to remove that installation or modify your `PATH`
> to make sure the version installed by `asdf` is always used.


> ❗️ **Warning**
> 
> If you haven't pushed your branch to the remote yet (you've been working only locally), two tests will fail:
> 
> - `e2e::running::init_new_project_test`
> - `e2e::running::simple_package_with_git_dependency`
> 
> After pushing the branch to the remote, those tests should pass locally.

## Formatting and Lints

Starknet Foundry uses [rustfmt](https://github.com/rust-lang/rustfmt) for formatting. You can run the formatter with:

```shell
$ cargo fmt
```

For linting, it uses [clippy](https://github.com/rust-lang/rust-clippy). You can run it with:

```shell
$ cargo clippy --all-targets --all-features -- --no-deps -W clippy::pedantic -A clippy::missing_errors_doc -A clippy::missing_panics_doc -A clippy::default_trait_access
```

or using our defined alias:

```shell
$ cargo lint
```

## Spelling

Starknet Foundry uses [typos](https://github.com/marketplace/actions/typos-action) for spelling checks.

You can run the checker with:

```shell
$ typos
```

Some typos can be automatically fixed by running:

```shell
$ typos -w
```
