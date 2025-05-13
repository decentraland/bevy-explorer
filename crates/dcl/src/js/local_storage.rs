use deno_core::{error::AnyError, op2, OpDecl, OpState};
use wallet::Wallet;

// wrap localStorage to include player address in all operations

pub fn override_ops() -> Vec<OpDecl> {
    vec![
        op_webstorage_length(),
        op_webstorage_key(),
        op_webstorage_set(),
        op_webstorage_get(),
        op_webstorage_remove(),
        op_webstorage_clear(),
        op_webstorage_iterate_keys(),
    ]
}

fn address(state: &OpState) -> String {
    state
        .borrow::<Wallet>()
        .address()
        .map(|a| format!("{a:#x}"))
        .unwrap_or_default()
}

fn iterate_keys(
    state: &mut OpState,
    persistent: bool,
) -> Result<impl Iterator<Item = String>, AnyError> {
    let address = address(state);
    let iter = deno_webstorage::op_webstorage_iterate_keys__raw_fn(state, persistent)?;
    Ok(iter.into_iter().filter(move |k| k.starts_with(&address)))
}

#[op2(fast)]
pub fn op_webstorage_length(state: &mut OpState, persistent: bool) -> Result<u32, AnyError> {
    Ok(iterate_keys(state, persistent)?.count() as u32)
}

#[op2]
#[string]
pub fn op_webstorage_key(
    state: &mut OpState,
    #[smi] index: u32,
    persistent: bool,
) -> Result<Option<String>, AnyError> {
    Ok(iterate_keys(state, persistent)?.nth(index as usize))
}

#[op2(fast)]
pub fn op_webstorage_set(
    state: &mut OpState,
    #[string] key: &str,
    #[string] value: &str,
    persistent: bool,
) -> Result<(), AnyError> {
    let address = address(state);
    deno_webstorage::op_webstorage_set__raw_fn(
        state,
        &format!("{address}:{key}"),
        value,
        persistent,
    )
}

#[op2]
#[string]
pub fn op_webstorage_get(
    state: &mut OpState,
    #[string] key_name: String,
    persistent: bool,
) -> Result<Option<String>, AnyError> {
    let address = address(state);
    deno_webstorage::op_webstorage_get__raw_fn(state, format!("{address}:{key_name}"), persistent)
}

#[op2(fast)]
pub fn op_webstorage_remove(
    state: &mut OpState,
    #[string] key_name: &str,
    persistent: bool,
) -> Result<(), AnyError> {
    let address = address(state);
    deno_webstorage::op_webstorage_remove__raw_fn(
        state,
        &format!("{address}:{key_name}"),
        persistent,
    )
}

#[op2(fast)]
pub fn op_webstorage_clear(state: &mut OpState, persistent: bool) -> Result<(), AnyError> {
    for key in iterate_keys(state, persistent)? {
        deno_webstorage::op_webstorage_remove__raw_fn(state, &key, persistent)?;
    }

    Ok(())
}

#[op2]
#[serde]
pub fn op_webstorage_iterate_keys(
    state: &mut OpState,
    persistent: bool,
) -> Result<Vec<String>, AnyError> {
    Ok(iterate_keys(state, persistent)?.collect())
}
