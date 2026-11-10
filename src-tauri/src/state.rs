use std::sync::Arc;

use crate::credentials::SecretStore;
use crate::database::Database;
use crate::monitoring::manager::MonitorManager;
use crate::sftp::remote::SftpPool;
use crate::sftp::transfer::TransferManager;
use crate::ssh::connections::ConnectionRegistry;
use crate::ssh::forwarding::ForwardManager;
use crate::ssh::session::SessionManager;

pub struct AppState {
    pub db: Arc<Database>,
    pub secrets: Arc<dyn SecretStore>,
    pub sessions: Arc<SessionManager>,
    pub connections: Arc<ConnectionRegistry>,
    pub monitors: Arc<MonitorManager>,
    pub sftp: Arc<SftpPool>,
    pub transfers: Arc<TransferManager>,
    pub forwards: Arc<ForwardManager>,
}

impl AppState {
    pub fn new(db: Database, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            db: Arc::new(db),
            secrets,
            sessions: Arc::new(SessionManager::default()),
            connections: Arc::new(ConnectionRegistry::default()),
            monitors: Arc::new(MonitorManager::default()),
            sftp: Arc::new(SftpPool::default()),
            transfers: Arc::new(TransferManager::default()),
            forwards: Arc::new(ForwardManager::default()),
        }
    }
}
