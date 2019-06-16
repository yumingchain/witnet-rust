//! # Application actor.
//!
//! See [`App`](App) actor for more information.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use actix::prelude::*;
use failure::Error;
use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;
use serde_json::json;

use witnet_net::client::tcp::{jsonrpc as rpc_client, JsonRpcClient};
use witnet_protected::ProtectedString;

use crate::actors::{crypto, storage, Crypto, RadExecutor, Storage};
use crate::wallet;

pub mod builder;
pub mod error;
pub mod handlers;

/// Expose message to stop application.
pub use handlers::Stop;

/// Application actor.
///
/// The application actor is in charge of managing the state of the application and coordinating the
/// service actors, e.g.: storage, node client, and so on.
pub struct App {
    db: Arc<rocksdb::DB>,
    storage: Addr<Storage>,
    rad_executor: Addr<RadExecutor>,
    crypto: Addr<Crypto>,
    node_client: Option<Addr<JsonRpcClient>>,
    subscriptions: [Option<pubsub::Sink>; 10],
    sessions: HashMap<wallet::SessionId, HashSet<wallet::WalletId>>,
    unlocked_wallets: HashMap<wallet::WalletId, HashSet<wallet::SessionId>>,
    wallet_keys: HashMap<wallet::WalletId, wallet::Key>,
}

// let result = if self.opened_wallets.borrow().iter().any(|id_| id_ == id) {
//     Err(storage::Error::WalletAlreadyOpenend(id.to_string()))
// } else {

// };

// result

impl App {
    pub fn build() -> builder::AppBuilder {
        builder::AppBuilder::default()
    }

    pub fn new(
        db: Arc<rocksdb::DB>,
        storage: Addr<Storage>,
        rad_executor: Addr<RadExecutor>,
        crypto: Addr<Crypto>,
        node_client: Option<Addr<JsonRpcClient>>,
    ) -> Self {
        Self {
            db,
            storage,
            rad_executor,
            node_client,
            crypto,
            subscriptions: Default::default(),
            sessions: Default::default(),
            unlocked_wallets: Default::default(),
            wallet_keys: Default::default(),
        }
    }

    /// Return an id for a new subscription. If there are no available subscription slots, then
    /// `None` is returned.
    pub fn subscribe(&mut self, subscriber: pubsub::Subscriber) -> Result<usize, Error> {
        let (id, slot) = self
            .subscriptions
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
            .ok_or_else(|| error::Error::SubscribeFailed("max limit of subscriptions reached"))?;

        *slot = subscriber
            .assign_id(pubsub::SubscriptionId::from(id as u64))
            .ok();

        Ok(id)
    }

    /// Remove a subscription and leave its corresponding slot free.
    pub fn unsubscribe(&mut self, id: pubsub::SubscriptionId) -> Result<(), Error> {
        let index = match id {
            pubsub::SubscriptionId::Number(n) => Ok(n as usize),
            _ => Err(error::Error::UnsubscribeFailed(
                "subscription id must be a number",
            )),
        }?;
        let slot = self
            .subscriptions
            .as_mut()
            .get_mut(index)
            .ok_or_else(|| error::Error::UnsubscribeFailed("subscription id not found"))?;

        *slot = None;

        Ok(())
    }

    /// Forward a Json-RPC call to the node.
    pub fn forward(
        &mut self,
        method: String,
        params: rpc::Params,
    ) -> ResponseFuture<serde_json::Value, Error> {
        match &self.node_client {
            Some(addr) => {
                let req = rpc_client::Request::method(method)
                    .params(params)
                    .expect("rpc::Params failed serialization");
                let fut = addr
                    .send(req)
                    .map_err(error::Error::RequestFailedToSend)
                    .and_then(|result| result.map_err(error::Error::RequestFailed))
                    .map_err(Error::from);

                Box::new(fut)
            }
            None => {
                let fut = future::err(Error::from(error::Error::NodeNotConnected));

                Box::new(fut)
            }
        }
    }

    /// Get id and caption of all the wallets stored in the database.
    fn get_wallet_infos(&self) -> ResponseFuture<Vec<wallet::WalletInfo>, Error> {
        let fut = self
            .storage
            .send(storage::GetWalletInfos(self.db.clone()))
            .map_err(map_storage_failed_err)
            .and_then(map_err);

        Box::new(fut)
    }

    /// Create an empty wallet.
    fn create_wallet(
        &self,
        caption: String,
        password: ProtectedString,
        seed_source: wallet::SeedSource,
    ) -> ResponseActFuture<Self, wallet::WalletId, Error> {
        let key_spec = wallet::Wip::Wip3;
        let fut = self
            .crypto
            .send(crypto::GenWalletKeys(seed_source))
            .map_err(map_crypto_failed_err)
            .and_then(map_err)
            .into_actor(self)
            .and_then(move |(id, master_key), slf, _ctx| {
                // Keypath: m/3'/4919'/0'
                let keypath = wallet::KeyPath::master()
                    .hardened(3)
                    .hardened(4919)
                    .hardened(0);
                let keychains = wallet::KeyChains::new(keypath);
                let account = wallet::Account::new(keychains);
                let content = wallet::WalletContent::new(master_key, key_spec, vec![account]);
                let info = wallet::WalletInfo {
                    id: id.clone(),
                    caption,
                };
                let wallet = wallet::Wallet::new(info, content);

                slf.storage
                    .send(storage::CreateWallet(slf.db.clone(), wallet, password))
                    .map_err(map_storage_failed_err)
                    .map(move |_| id)
                    .into_actor(slf)
            });

        Box::new(fut)
    }

    fn unlock_wallet(
        &mut self,
        id: wallet::WalletId,
        session_id: wallet::SessionId,
        password: ProtectedString,
    ) -> ResponseActFuture<Self, (), Error> {
        // check if the wallet has already being unlocked by another session
        match self.unlocked_wallets.get(&id).cloned() {
            Some(mut owner_sessions) => {
                log::debug!(
                    "Wallet {} already unlocked. Appending {} to its list of active sessions.",
                    &id,
                    &session_id
                );
                owner_sessions.insert(id);
                Box::new(fut::ok(()))
            }
            None => {
                let f = self
                    .storage
                    .send(storage::UnlockWallet(self.db.clone(), id, password))
                    .map_err(map_storage_failed_err)
                    .and_then(map_err)
                    .into_actor(self)
                    .and_then(move |unlocked_wallet, _slf, ctx| {
                        ctx.notify(handlers::WalletUnlocked {
                            session_id,
                            unlocked_wallet,
                        });

                        fut::ok(())
                    });

                Box::new(f)
            }
        }
    }

    /// Perform all the tasks needed to properly stop the application.
    fn stop(&self) -> ResponseFuture<(), Error> {
        let fut = self
            .storage
            .send(storage::Flush(self.db.clone()))
            .map_err(map_storage_failed_err)
            .and_then(map_err);

        Box::new(fut)
    }

    /// Save wallet in the list of unlocked wallets for the given session.
    fn assoc_wallet_to_session(
        &mut self,
        wallet: wallet::UnlockedWallet,
        session_id: wallet::SessionId,
    ) {
        let id = wallet.id;

        let session_wallets = self
            .sessions
            .entry(session_id.clone())
            .or_insert_with(HashSet::new);
        let wallet_sessions = self
            .unlocked_wallets
            .entry(id.clone())
            .or_insert_with(HashSet::new);

        session_wallets.insert(id.clone());
        wallet_sessions.insert(session_id.clone());
        self.wallet_keys.insert(id.clone(), wallet.key);

        log::debug!("Associated wallet: {} to session: {}", &id, session_id);
    }
}

impl Actor for App {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        if let Some(ref client) = self.node_client {
            let recipient = ctx.address().recipient();
            let request =
                rpc_client::Request::method("witnet_subscribe").value(json!(["newBlocks"]));
            client.do_send(rpc_client::SetSubscriber(recipient, request));
        }
    }
}

impl Supervised for App {}

fn map_crypto_failed_err(err: actix::MailboxError) -> Error {
    Error::from(error::Error::CryptoCommFailed(err))
}

fn map_storage_failed_err(err: actix::MailboxError) -> Error {
    Error::from(error::Error::StorageCommFailed(err))
}

fn map_err<T, E>(result: Result<T, E>) -> Result<T, Error>
where
    E: failure::Fail,
{
    result.map_err(Error::from)
}
