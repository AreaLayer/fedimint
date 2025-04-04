pub mod recovery;

use std::collections::{BTreeMap, BTreeSet};

use fedimint_api_client::api::{DynGlobalApi, DynModuleApi};
use fedimint_core::config::FederationId;
use fedimint_core::core::ModuleKind;
use fedimint_core::db::{Database, DatabaseVersion};
use fedimint_core::module::{ApiAuth, ApiVersion, CommonModuleInit, ModuleInit, MultiApiVersion};
use fedimint_core::task::TaskGroup;
use fedimint_core::{NumPeers, apply, async_trait_maybe_send};
use fedimint_derive_secret::DerivableSecret;
use fedimint_logging::LOG_CLIENT;
use tracing::warn;

use super::ClientContext;
use super::recovery::RecoveryProgress;
use crate::db::ClientModuleMigrationFn;
use crate::module::ClientModule;
use crate::sm::ModuleNotifier;

pub struct ClientModuleInitArgs<C>
where
    C: ClientModuleInit,
{
    pub federation_id: FederationId,
    pub peer_num: usize,
    pub cfg: <<C as ModuleInit>::Common as CommonModuleInit>::ClientConfig,
    pub db: Database,
    pub core_api_version: ApiVersion,
    pub module_api_version: ApiVersion,
    pub module_root_secret: DerivableSecret,
    pub notifier: ModuleNotifier<<<C as ClientModuleInit>::Module as ClientModule>::States>,
    pub api: DynGlobalApi,
    pub admin_auth: Option<ApiAuth>,
    pub module_api: DynModuleApi,
    pub context: ClientContext<<C as ClientModuleInit>::Module>,
    pub task_group: TaskGroup,
}

impl<C> ClientModuleInitArgs<C>
where
    C: ClientModuleInit,
{
    pub fn federation_id(&self) -> &FederationId {
        &self.federation_id
    }

    pub fn peer_num(&self) -> usize {
        self.peer_num
    }

    pub fn cfg(&self) -> &<<C as ModuleInit>::Common as CommonModuleInit>::ClientConfig {
        &self.cfg
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn core_api_version(&self) -> &ApiVersion {
        &self.core_api_version
    }

    pub fn module_api_version(&self) -> &ApiVersion {
        &self.module_api_version
    }

    pub fn module_root_secret(&self) -> &DerivableSecret {
        &self.module_root_secret
    }

    pub fn notifier(
        &self,
    ) -> &ModuleNotifier<<<C as ClientModuleInit>::Module as ClientModule>::States> {
        &self.notifier
    }

    pub fn api(&self) -> &DynGlobalApi {
        &self.api
    }

    pub fn admin_auth(&self) -> Option<&ApiAuth> {
        self.admin_auth.as_ref()
    }

    pub fn module_api(&self) -> &DynModuleApi {
        &self.module_api
    }

    /// Get the [`ClientContext`] for later use
    ///
    /// Notably `ClientContext` can not be used during `ClientModuleInit::init`,
    /// as the outer context is not yet complete. But it can be stored to be
    /// used in the methods of [`ClientModule`], at which point it will be
    /// ready.
    pub fn context(&self) -> ClientContext<<C as ClientModuleInit>::Module> {
        self.context.clone()
    }

    pub fn task_group(&self) -> &TaskGroup {
        &self.task_group
    }
}

pub struct ClientModuleRecoverArgs<C>
where
    C: ClientModuleInit,
{
    pub federation_id: FederationId,
    pub num_peers: NumPeers,
    pub cfg: <<C as ModuleInit>::Common as CommonModuleInit>::ClientConfig,
    pub db: Database,
    pub core_api_version: ApiVersion,
    pub module_api_version: ApiVersion,
    pub module_root_secret: DerivableSecret,
    pub notifier: ModuleNotifier<<<C as ClientModuleInit>::Module as ClientModule>::States>,
    pub api: DynGlobalApi,
    pub admin_auth: Option<ApiAuth>,
    pub module_api: DynModuleApi,
    pub context: ClientContext<<C as ClientModuleInit>::Module>,
    pub progress_tx: tokio::sync::watch::Sender<RecoveryProgress>,
    pub task_group: TaskGroup,
}

impl<C> ClientModuleRecoverArgs<C>
where
    C: ClientModuleInit,
{
    pub fn federation_id(&self) -> &FederationId {
        &self.federation_id
    }

    pub fn num_peers(&self) -> NumPeers {
        self.num_peers
    }

    pub fn cfg(&self) -> &<<C as ModuleInit>::Common as CommonModuleInit>::ClientConfig {
        &self.cfg
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn task_group(&self) -> &TaskGroup {
        &self.task_group
    }

    pub fn core_api_version(&self) -> &ApiVersion {
        &self.core_api_version
    }

    pub fn module_api_version(&self) -> &ApiVersion {
        &self.module_api_version
    }

    pub fn module_root_secret(&self) -> &DerivableSecret {
        &self.module_root_secret
    }

    pub fn notifier(
        &self,
    ) -> &ModuleNotifier<<<C as ClientModuleInit>::Module as ClientModule>::States> {
        &self.notifier
    }

    pub fn api(&self) -> &DynGlobalApi {
        &self.api
    }

    pub fn admin_auth(&self) -> Option<&ApiAuth> {
        self.admin_auth.as_ref()
    }

    pub fn module_api(&self) -> &DynModuleApi {
        &self.module_api
    }

    /// Get the [`ClientContext`]
    ///
    /// Notably `ClientContext`, unlike [`ClientModuleInitArgs::context`],
    /// the client context is guaranteed to be usable immediately.
    pub fn context(&self) -> ClientContext<<C as ClientModuleInit>::Module> {
        self.context.clone()
    }

    pub fn update_recovery_progress(&self, progress: RecoveryProgress) {
        // we want a warning if the send channel was not connected to
        #[allow(clippy::disallowed_methods)]
        if progress.is_done() {
            // Recovery is complete when the recovery function finishes. To avoid
            // confusing any downstream code, we never send completed process.
            warn!(target: LOG_CLIENT, "Module trying to send a completed recovery progress. Ignoring");
        } else if progress.is_none() {
            // Recovery starts with "none" none progress. To avoid
            // confusing any downstream code, we never send none process afterwards.
            warn!(target: LOG_CLIENT, "Module trying to send a none recovery progress. Ignoring");
        } else if self.progress_tx.send(progress).is_err() {
            warn!(target: LOG_CLIENT, "Module trying to send a recovery progress but nothing is listening");
        }
    }
}

#[apply(async_trait_maybe_send!)]
pub trait ClientModuleInit: ModuleInit + Sized {
    type Module: ClientModule;

    /// Api versions of the corresponding server side module's API
    /// that this client module implementation can use.
    fn supported_api_versions(&self) -> MultiApiVersion;

    fn kind() -> ModuleKind {
        <Self::Module as ClientModule>::kind()
    }

    /// Recover the state of the client module, optionally from an existing
    /// snapshot.
    ///
    /// If `Err` is returned, the higher level client/application might try
    /// again at a different time (client restarted, code version changed, etc.)
    async fn recover(
        &self,
        _args: &ClientModuleRecoverArgs<Self>,
        _snapshot: Option<&<Self::Module as ClientModule>::Backup>,
    ) -> anyhow::Result<()> {
        warn!(
            target: LOG_CLIENT,
            kind = %<Self::Module as ClientModule>::kind(),
            "Module does not support recovery, completing without doing anything"
        );
        Ok(())
    }

    /// Initialize a [`ClientModule`] instance from its config
    async fn init(&self, args: &ClientModuleInitArgs<Self>) -> anyhow::Result<Self::Module>;

    /// Retrieves the database migrations from the module to be applied to the
    /// database before the module is initialized. The database migrations map
    /// is indexed on the "from" version.
    fn get_database_migrations(&self) -> BTreeMap<DatabaseVersion, ClientModuleMigrationFn> {
        BTreeMap::new()
    }

    /// Db prefixes used by the module
    ///
    /// If `Some` is returned, it should contain list of database
    /// prefixes actually used by the module for it's keys.
    ///
    /// In (some subset of) non-production tests,
    /// module database will be scanned for presence of keys
    /// that do not belong to this list to verify integrity
    /// of data and possibly catch any unforeseen bugs.
    fn used_db_prefixes(&self) -> Option<BTreeSet<u8>> {
        None
    }
}
