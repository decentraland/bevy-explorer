use bevy::log::debug;
use deno_core::{anyhow, error::AnyError, op2, OpDecl, OpState};
use std::{cell::RefCell, rc::Rc};
use system_bridge::SystemApi;
use wallet::Wallet;

use super::SuperUserScene;

// list of op declarations
pub fn ops(super_user: bool) -> Vec<OpDecl> {
    if super_user {
        vec![
            op_check_for_update(),
            op_motd(),
            op_get_current_login(),
            op_get_previous_login(),
            op_login_previous(),
            op_login_new_code(),
            op_login_new_success(),
            op_login_cancel(),
            op_login_guest(),
            op_logout(),
        ]
    } else {
        Vec::default()
    }
}

#[op2(async)]
#[serde]
async fn op_check_for_update(state: Rc<RefCell<OpState>>) -> Result<(String, String), AnyError> {
    debug!("op_check_for_update");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::CheckForUpdate(sx.into()))
        .unwrap();

    Ok(rx
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .unwrap_or_default())
}

#[op2(async)]
#[string]
async fn op_motd(state: Rc<RefCell<OpState>>) -> Result<String, AnyError> {
    debug!("op_motd");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::MOTD(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2]
#[string]
fn op_get_current_login(state: &mut OpState) -> Option<String> {
    state
        .borrow::<Wallet>()
        .address()
        .map(|h160| format!("{h160:#x}"))
}

#[op2(async)]
#[string]
async fn op_get_previous_login(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    debug!("op_get_previous_login");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetPreviousLogin(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2(async)]
#[serde]
async fn op_login_previous(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
    debug!("op_login_previous");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginPrevious(sx.into()))
        .unwrap();

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

#[derive(Default)]
struct NewLogin {
    code: Option<tokio::sync::oneshot::Receiver<Result<Option<i32>, String>>>,
    result: Option<tokio::sync::oneshot::Receiver<Result<(), String>>>,
}

fn new_login(state: &mut OpState) -> &mut NewLogin {
    if !state.has::<NewLogin>() {
        state.put(NewLogin::default());
    }

    let mut login = state.take::<NewLogin>();

    if login.code.is_none() && login.result.is_none() {
        let (sc, code) = tokio::sync::oneshot::channel();
        let (sx, result) = tokio::sync::oneshot::channel();
        state
            .borrow_mut::<SuperUserScene>()
            .send(SystemApi::LoginNew(sc.into(), sx.into()))
            .unwrap();

        login.code = Some(code);
        login.result = Some(result);
    }

    state.put(login);
    state.borrow_mut()
}

#[op2(async)]
#[string]
async fn op_login_new_code(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    debug!("op_login_new_code");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut state);
        login.code.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
        .map(|code| code.map(|c| format!("{c}")))
}

#[op2(async)]
#[string]
async fn op_login_new_success(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
    debug!("op_login_new_success");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut state);
        login.result.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

#[op2(fast)]
fn op_login_guest(state: &mut OpState) {
    debug!("op_login_guest");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginGuest)
        .unwrap();
}

#[op2(fast)]
fn op_login_cancel(state: &mut OpState) {
    debug!("op_login_cancel");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginCancel)
        .unwrap();
}

#[op2(fast)]
fn op_logout(state: &mut OpState) {
    debug!("op_logout");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::Logout)
        .unwrap();
}
