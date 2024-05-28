use crate::utils::{assert_diagnostics, assert_output, EMPTY_FN};
use cairo_lang_macro::{Severity, TokenStream};
use indoc::formatdoc;
use snforge_scarb_plugin::attributes::internal_config_statement::internal_config_statement;

#[test]
fn fails_with_non_empty_args() {
    let item = TokenStream::new(EMPTY_FN.into());
    let args = TokenStream::new("(123)".into());

    let result = internal_config_statement(args, item);

    assert_diagnostics(
        &result,
        &[(
            Severity::Error,
            "#[__internal_config_statement] does not accept any arguments",
        )],
    );
}
#[test]
fn appends_config_statement() {
    let item = TokenStream::new(EMPTY_FN.into());
    let args = TokenStream::new(String::new());

    let result = internal_config_statement(args, item);

    assert_diagnostics(&result, &[]);

    assert_output(
        &result,
        "
            fn empty_fn() {
                if starknet::testing::cheatcode::<'is_config_mode'>() {
                    return;
                }
            }
        ",
    );
}

#[test]
fn is_used_once() {
    let item = TokenStream::new(formatdoc!(
        "
            #[__internal_config_statement]
            {EMPTY_FN}
        "
    ));
    let args = TokenStream::new(String::new()); //TODO check args empty

    let result = internal_config_statement(args, item);

    assert_diagnostics(
        &result,
        &[(
            Severity::Error,
            "#[__internal_config_statement] can only be used once per item",
        )],
    );
}
