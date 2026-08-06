use bevy::log::debug;
use dcl::js::player_identity;
use deno_core::{error::AnyError, op2, OpDecl, OpState};

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
    let address = player_identity(state)
        .map(|id| id.address)
        .unwrap_or_default();
    debug!("local storage address: {address:?}");
    address
}

/// The `{address}:` prefix every key for this player is stored under.
fn address_prefix(state: &OpState) -> String {
    format!("{}:", address(state))
}

/// This player's keys, still carrying their `{address}:` prefix.
///
/// Matching is on the full `{address}:` prefix, never the bare address:
/// `address` falls back to an empty string when there is no player identity,
/// and `starts_with("")` holds for every key in the store — including other
/// players'. The bare-address match also collided one address with a longer
/// one sharing its hex prefix.
fn prefixed_keys(state: &mut OpState, persistent: bool) -> Result<Vec<String>, AnyError> {
    let prefix = address_prefix(state);
    let iter = deno_webstorage::op_webstorage_iterate_keys__raw_fn(state, persistent)?;
    Ok(iter.into_iter().filter(|k| k.starts_with(&prefix)).collect())
}

/// The same keys as the scene names them, prefix removed.
///
/// A key that does not carry the prefix is dropped rather than split, so no
/// path remains where a foreign key reaches a `split_once` that would unwrap.
fn scene_keys(state: &mut OpState, persistent: bool) -> Result<Vec<String>, AnyError> {
    let prefix = address_prefix(state);
    let iter = deno_webstorage::op_webstorage_iterate_keys__raw_fn(state, persistent)?;
    Ok(iter
        .into_iter()
        .filter_map(|k| k.strip_prefix(&prefix).map(ToOwned::to_owned))
        .collect())
}

#[op2(fast)]
pub fn op_webstorage_length(state: &mut OpState, persistent: bool) -> Result<u32, AnyError> {
    Ok(scene_keys(state, persistent)?.len() as u32)
}

#[op2]
#[string]
pub fn op_webstorage_key(
    state: &mut OpState,
    #[smi] index: u32,
    persistent: bool,
) -> Result<Option<String>, AnyError> {
    Ok(scene_keys(state, persistent)?.into_iter().nth(index as usize))
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
    for key in prefixed_keys(state, persistent)? {
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
    scene_keys(state, persistent)
}
