use std::sync::Arc;

use actix::utils::TimerFunc;
use futures::future;

use witnet_data_structures::chain::InventoryItem;

use crate::actors::{
    worker::{HandleBlockRequest, HandleSuperBlockRequest, NodeStatusRequest, NotifyStatus},
    *,
};
use crate::model;

use super::*;

impl App {
    /// Start the actor App with the provided parameters
    pub fn start(params: Params) -> Addr<Self> {
        let actor = Self {
            server: None,
            params,
            state: Default::default(),
        };

        actor.start()
    }

    /// Stop the wallet application completely
    /// Note: if `rpc.on` subscriptions are closed before shutting down, stop() works correctly.
    pub fn stop(&mut self, ctx: &mut <Self as Actor>::Context) {
        log::debug!("Stopping application...");
        let s = self.server.take();
        // Potentially leak memory because we never join the thread, but that's fine because we are stopping the application
        std::thread::spawn(move || {
            drop(s);
        });
        self.stop_worker()
            .map_err(|_| log::error!("Couldn't stop application!"))
            .and_then(|_| {
                log::info!("Application stopped. Shutting down system!");
                System::current().stop();
                Ok(())
            })
            .into_actor(self)
            .spawn(ctx);
    }

    /// Return a new subscription id for a session.
    pub fn next_subscription_id(
        &mut self,
        session_id: &types::SessionId,
    ) -> Result<types::SubscriptionId> {
        if self.state.is_session_active(session_id) {
            // We are re-using the session id as the subscription id, this is because using a number
            // can let any client call the unsubscribe method for any other session.
            Ok(types::SubscriptionId::from(session_id))
        } else {
            Err(Error::SessionNotFound)
        }
    }

    /// Try to create a subscription and store it in the session. After subscribing, events related
    /// to wallets unlocked by this session will be sent to the client.
    pub fn subscribe(
        &mut self,
        session_id: types::SessionId,
        _subscription_id: types::SubscriptionId,
        sink: types::Sink,
    ) -> Result<()> {
        self.state.subscribe(&session_id, sink).map(|dyn_sink| {
            // If the subscription was successful, notify subscriber about initial status for all
            // wallets that belong to this session.
            let wallets = self.state.get_wallets_by_session(&session_id);
            if let Ok(wallets) = wallets {
                for (_, wallet) in wallets.iter() {
                    self.params
                        .worker
                        .do_send(NotifyStatus(wallet.clone(), dyn_sink.clone()));
                }
            }
        })
    }

    /// Remove a subscription.
    pub fn unsubscribe(&mut self, id: &types::SubscriptionId) -> Result<()> {
        // Session id and subscription id are currently the same thing. See comment in
        // next_subscription_id method.
        self.state.unsubscribe(id).map(|_| ())
    }

    /// Generate a receive address for the wallet's current account.
    pub fn generate_address(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
        external: bool,
        label: Option<String>,
    ) -> ResponseActFuture<model::Address> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::GenAddress(wallet, external, label))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Get a list of addresses generated by a wallet.
    pub fn get_addresses(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
        offset: u32,
        limit: u32,
        external: bool,
    ) -> ResponseActFuture<model::Addresses> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::GetAddresses(wallet, offset, limit, external))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Get a list of addresses generated by a wallet.
    pub fn get_balance(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
    ) -> ResponseActFuture<model::WalletBalance> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::GetBalance(wallet))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Get a list of transactions associated to a wallet account.
    pub fn get_transactions(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
        offset: u32,
        limit: u32,
    ) -> ResponseActFuture<model::Transactions> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::GetTransactions(wallet, offset, limit))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Run a RADRequest and return the computed result.
    pub fn run_rad_request(
        &self,
        req: types::RADRequest,
    ) -> ResponseFuture<types::RADRequestExecutionReport> {
        let f = self
            .params
            .worker
            .send(worker::RunRadRequest(req))
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Generate a random BIP39 mnemonics sentence
    pub fn generate_mnemonics(&self, length: types::MnemonicLength) -> ResponseFuture<String> {
        let f = self
            .params
            .worker
            .send(worker::GenMnemonic(length))
            .map_err(From::from);

        Box::new(f)
    }

    /// Forward a Json-RPC call to the node.
    pub fn forward(
        &mut self,
        method: String,
        params: types::RpcParams,
    ) -> ResponseFuture<types::Json> {
        let req = types::RpcRequest::method(method)
            .timeout(self.params.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Get public info of all the wallets stored in the database.
    pub fn wallet_infos(&self) -> ResponseFuture<Vec<model::Wallet>> {
        let f = self
            .params
            .worker
            .send(worker::WalletInfos)
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Create an empty HD Wallet.
    pub fn create_wallet(
        &self,
        password: types::Password,
        seed_source: types::SeedSource,
        name: Option<String>,
        description: Option<String>,
        overwrite: bool,
    ) -> ResponseFuture<String> {
        let f = self
            .params
            .worker
            .send(worker::CreateWallet(
                name,
                description,
                password,
                seed_source,
                overwrite,
            ))
            .flatten()
            .map_err(From::from);

        Box::new(f)
    }

    /// Update a wallet details
    pub fn update_wallet(
        &self,
        session_id: types::SessionId,
        wallet_id: String,
        name: Option<String>,
        description: Option<String>,
    ) -> ResponseActFuture<()> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            let wallet_update = slf
                .params
                .worker
                .send(worker::UpdateWallet(
                    wallet,
                    name.clone(),
                    description.clone(),
                ))
                .flatten()
                .map_err(From::from);

            let info_update = slf
                .params
                .worker
                .send(worker::UpdateWalletInfo(wallet_id, name))
                .flatten()
                .map_err(From::from);

            wallet_update.join(info_update).map(|_| ()).into_actor(slf)
        });

        Box::new(f)
    }

    /// Lock a wallet, that is, remove its encryption/decryption key from the list of known keys and
    /// close the session.
    ///
    /// This means the state of this wallet won't be updated with information received from the
    /// node.
    pub fn lock_wallet(&mut self, session_id: types::SessionId, wallet_id: String) -> Result<()> {
        self.state.remove_wallet(&session_id, &wallet_id)
    }

    /// Load a wallet's private information and keys in memory.
    pub fn unlock_wallet(
        &self,
        wallet_id: String,
        password: types::Password,
    ) -> ResponseActFuture<types::UnlockedWallet> {
        let f = self
            .params
            .worker
            .send(worker::UnlockWallet(wallet_id.clone(), password))
            .flatten()
            .map_err(From::from)
            .into_actor(self)
            .and_then(move |res, slf: &mut Self, _ctx| {
                let types::UnlockedSessionWallet {
                    wallet,
                    session_id,
                    data,
                } = res;

                slf.state
                    .create_session(session_id.clone(), wallet_id.clone(), wallet.clone());

                // Start synchronization for this wallet
                let sink = slf.state.get_sink(&session_id);
                slf.params.worker.do_send(worker::SyncRequest {
                    wallet_id,
                    wallet,
                    sink,
                });

                fut::ok(types::UnlockedWallet { data, session_id })
            });

        Box::new(f)
    }

    pub fn create_vtt(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
        vtt_params: types::VttParams,
    ) -> ResponseActFuture<types::Transaction> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::CreateVtt(wallet, vtt_params))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    pub fn create_data_req(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
        params: types::DataReqParams,
    ) -> ResponseActFuture<types::Transaction> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::CreateDataReq(wallet, params))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Perform all the tasks needed to properly stop the application.
    pub fn stop_worker(&self) -> ResponseFuture<()> {
        let fut = self
            .params
            .worker
            .send(worker::FlushDb)
            .map_err(internal_error)
            .and_then(|result| result.map_err(internal_error));

        Box::new(fut)
    }

    /// Return a timer function that can be scheduled to expire the session after the configured time.
    pub fn set_session_to_expire(&self, session_id: types::SessionId) -> TimerFunc<Self> {
        log::debug!(
            "Session {} will expire in {} seconds.",
            &session_id,
            self.params.session_expires_in.as_secs()
        );

        TimerFunc::new(
            self.params.session_expires_in,
            move |slf: &mut Self, _ctx| match slf.close_session(session_id.clone()) {
                Ok(_) => log::info!("Session {} closed", session_id),
                Err(err) => log::error!("Session {} couldn't be closed: {}", session_id, err),
            },
        )
    }

    /// Remove a session from the list of active sessions.
    pub fn close_session(&mut self, session_id: types::SessionId) -> Result<()> {
        self.state.remove_session(&session_id)
    }

    /// Get a client's previously stored value in the db (set method) with the given key.
    pub fn get(
        &self,
        session_id: types::SessionId,
        wallet_id: String,
        key: String,
    ) -> ResponseActFuture<Option<types::RpcValue>> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(|wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::Get(wallet, key))
                .flatten()
                .map_err(From::from)
                .and_then(|opt| match opt {
                    Some(value) => future::result(
                        serde_json::from_str(&value)
                            .map_err(internal_error)
                            .map(Some),
                    ),
                    None => future::result(Ok(None)),
                })
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Store a client's value in the db, associated to the given key.
    pub fn set(
        &self,
        session_id: types::SessionId,
        wallet_id: String,
        key: String,
        value: types::RpcParams,
    ) -> ResponseActFuture<()> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, _, _| {
            fut::result(serde_json::to_string(&value).map_err(internal_error)).and_then(
                move |value, slf: &mut Self, _| {
                    slf.params
                        .worker
                        .send(worker::Set(wallet, key, value))
                        .flatten()
                        .map_err(From::from)
                        .into_actor(slf)
                },
            )
        });

        Box::new(f)
    }

    /// Handle any kind of notifications received from a Witnet node.
    pub fn handle_notification(&mut self, topic: String, value: types::Json) -> Result<()> {
        match topic.as_str() {
            "blocks" => self.handle_block_notification(value),
            "superblocks" => self.handle_superblock_notification(value),
            "status" => self.handle_node_status_notification(value),
            _ => {
                log::debug!("Unhandled `{}` notification", topic);
                log::trace!("Payload is {:?}", value);

                Ok(())
            }
        }
    }

    /// Handle new block notifications received from a Witnet node.
    pub fn handle_block_notification(&mut self, value: types::Json) -> Result<()> {
        let block = serde_json::from_value::<types::ChainBlock>(value).map_err(node_error)?;

        // This iterator is collected early so as to free the immutable reference to `self`.
        let wallets: Vec<types::SessionWallet> = self
            .state
            .wallets
            .iter()
            .map(|(_, wallet)| wallet.clone())
            .collect();

        for wallet in &wallets {
            let sink = self.state.get_sink(&wallet.session_id);
            self.handle_block_in_worker(&block, &wallet, sink.clone());
        }

        Ok(())
    }

    /// Handle superblock notifications received from a Witnet node.
    pub fn handle_superblock_notification(&mut self, value: types::Json) -> Result<()> {
        let superblock_notification =
            serde_json::from_value::<types::SuperBlockNotification>(value).map_err(node_error)?;

        // This iterator is collected early so as to free the immutable reference to `self`.
        let wallets: Vec<types::SessionWallet> = self
            .state
            .wallets
            .iter()
            .map(|(_, wallet)| wallet.clone())
            .collect();

        for wallet in &wallets {
            let sink = self.state.get_sink(&wallet.session_id);
            self.handle_superblock_in_worker(
                superblock_notification.clone(),
                wallet.clone(),
                sink.clone(),
            );
        }

        Ok(())
    }

    /// Handle node status notifications received from a Witnet node.
    pub fn handle_node_status_notification(&mut self, value: types::Json) -> Result<()> {
        let status = serde_json::from_value::<types::StateMachine>(value).map_err(node_error)?;

        // This iterator is collected early so as to free the immutable reference to `self`.
        let wallets: Vec<types::SessionWallet> = self
            .state
            .wallets
            .iter()
            .map(|(_, wallet)| wallet.clone())
            .collect();

        for wallet in &wallets {
            let sink = self.state.get_sink(&wallet.session_id);
            self.handle_node_status_in_worker(&status, wallet, sink.clone());
        }

        Ok(())
    }

    /// Offload block processing into a worker that operates on a different Arbiter than the main
    /// server thread, so as not to lock the rest of the application.
    pub fn handle_block_in_worker(
        &self,
        block: &types::ChainBlock,
        wallet: &types::SessionWallet,
        sink: types::DynamicSink,
    ) {
        // NOTE: Possible enhancement.
        // Maybe is a good idea to use a shared reference Arc instead of cloning `block` altogether,
        // moreover when this method is called iteratively by `handle_block_notification`.
        self.params.worker.do_send(HandleBlockRequest {
            block: block.clone(),
            wallet: wallet.clone(),
            sink,
        });
    }

    /// Offload superblock processing into a worker that operates on a different Arbiter than the main
    /// server thread, so as not to lock the rest of the application.
    pub fn handle_superblock_in_worker(
        &self,
        superblock_notification: types::SuperBlockNotification,
        wallet: types::SessionWallet,
        sink: types::DynamicSink,
    ) {
        self.params.worker.do_send(HandleSuperBlockRequest {
            superblock_notification,
            wallet,
            sink,
        });
    }

    /// Offload node status into a worker that operates on a different Arbiter than the main
    /// server thread, so as not to lock the rest of the application.
    pub fn handle_node_status_in_worker(
        &self,
        status: &types::StateMachine,
        wallet: &types::SessionWallet,
        sink: types::DynamicSink,
    ) {
        self.params.worker.do_send(NodeStatusRequest {
            status: *status,
            wallet: wallet.clone(),
            sink,
        });
    }

    /// Send a transaction to witnet network using the Inventory method
    fn send_inventory_transaction(
        &self,
        txn: types::Transaction,
    ) -> ResponseActFuture<serde_json::Value> {
        let method = "inventory".to_string();
        let params = InventoryItem::Transaction(txn);

        let req = types::RpcRequest::method(method)
            .timeout(self.params.requests_timeout)
            .params(params)
            .expect("params failed serialization");
        let f = self
            .get_client()
            .actor
            .send(req)
            .flatten()
            .map_err(From::from)
            .inspect(|res| {
                log::debug!("Inventory request result: {:?}", res);
            })
            .map_err(|err| {
                log::warn!("Inventory request failed: {}", &err);
                err
            })
            .into_actor(self);

        Box::new(f)
    }

    /// Send a transaction to the node as inventory item broadcast
    /// and add a local pending balance movement to the wallet state.
    pub fn send_transaction(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
        transaction: types::Transaction,
    ) -> ResponseActFuture<SendTransactionResponse> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, act: &mut Self, _| {
            act.send_inventory_transaction(transaction.clone())
                .and_then(move |jsonrpc_result, _slf, _ctx| {
                    match wallet.add_local_movement(&model::ExtendedTransaction {
                        transaction,
                        metadata: None,
                    }) {
                        Ok(balance_movement) => actix::fut::ok(SendTransactionResponse {
                            jsonrpc_result,
                            balance_movement,
                        }),
                        Err(e) => {
                            log::error!("Error while adding local pending movement: {}", e);

                            actix::fut::err(Error::Internal(failure::Error::from(e)))
                        }
                    }
                })
        });

        Box::new(f)
    }

    /// Use wallet's master key to sign message data
    pub fn sign_data(
        &self,
        session_id: &types::SessionId,
        wallet_id: &str,
        data: String,
        extended_pk: bool,
    ) -> ResponseActFuture<model::ExtendedKeyedSignature> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::SignData(wallet, data, extended_pk))
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Shutdown system if session id is valid or there are no open sessions
    pub fn shutdown_request(
        &mut self,
        session_id: Option<types::SessionId>,
        ctx: &mut <Self as Actor>::Context,
    ) -> Result<()> {
        // Check if valid id or no open session(s)
        if let Some(session_id) = session_id {
            self.state.get_wallets_by_session(&session_id)?;
        } else if !self.state.sessions.is_empty() {
            return Err(app::Error::SessionsStillOpen);
        }
        self.stop(ctx);

        Ok(())
    }

    /// Get the URL and address of an existing JsonRpcClient actor.
    ///
    /// This method exists for convenience in case that at some point we decide to allow changing
    /// the `JsonRpcClient` address by putting `NodeClient` inside an `Arc<RwLock<_>>` or similar.
    #[inline(always)]
    pub fn get_client(&self) -> Arc<NodeClient> {
        self.params.client.clone()
    }

    /// Subscribe to receiving real time notifications of a specific type from a Witnet node.
    pub fn node_subscribe(&self, method: &str, ctx: &mut <Self as Actor>::Context) {
        let recipient = ctx.address().recipient();

        let request = types::RpcRequest::method("witnet_subscribe")
            .timeout(self.params.requests_timeout)
            .value(serde_json::to_value([method]).expect(
                "Any JSON-RPC method name should be serializable using `serde_json::to_value`",
            ));

        log::debug!("Subscribing to {} notifications: {:?}", method, request);

        self.get_client()
            .actor
            .do_send(jsonrpc::Subscribe(request, recipient));
    }

    /// Validate seed (mnemonics or xprv):
    ///  - check if seed data is valid
    ///  - check if there is already a wallet created with same seed
    ///  - return wallet id deterministically derived from seed data
    pub fn validate_seed(
        &self,
        seed_source: String,
        seed_data: types::Password,
    ) -> ResponseActFuture<ValidateMnemonicsResponse> {
        // Validate mnemonics source and data
        let f = fut::result(match seed_source.as_ref() {
            "xprv" => Ok(types::SeedSource::Xprv(seed_data)),
            "mnemonics" => types::Mnemonic::from_phrase(seed_data)
                .map_err(|err| Error::Validation(app::field_error("seed_data", format!("{}", err))))
                .map(types::SeedSource::Mnemonics),
            _ => Err(Error::Validation(app::field_error(
                "seed_source",
                "Seed source has to be mnemonics|xprv.",
            ))),
        })
        // Check if seed was already used in wallet
        .and_then(|seed, slf: &mut Self, _| {
            slf.params
                .worker
                .send(worker::CheckWalletSeedRequest(seed))
                .flatten()
                .map_err(From::from)
                .map(|(exist, wallet_id)| ValidateMnemonicsResponse { exist, wallet_id })
                .into_actor(slf)
        });

        Box::new(f)
    }

    /// Clear all chain data for a wallet state.
    ///
    /// Proceed with caution, as this wipes the following data entirely:
    /// - Synchronization status
    /// - Balances
    /// - Movements
    /// - Addresses and their metadata
    ///
    /// In order to prevent data race conditions, resyncing is not allowed while a sync or resync
    /// process is already in progress. Accordingly, this function returns whether chain data has
    /// been cleared or not.
    pub fn clear_chain_data_and_resync(
        &mut self,
        session_id: types::SessionId,
        wallet_id: String,
    ) -> ResponseActFuture<bool> {
        let f = fut::result(
            self.state
                .get_wallet_by_session_and_id(&session_id, &wallet_id),
        )
        .and_then(move |wallet, slf: &mut Self, _| {
            let sink = slf.state.get_sink(&session_id);

            // Send `Resync` message to worker
            slf.params
                .worker
                .send(worker::Resync {
                    wallet_id,
                    wallet,
                    sink,
                })
                .flatten()
                .map_err(From::from)
                .into_actor(slf)
        });

        Box::new(f)
    }
}
