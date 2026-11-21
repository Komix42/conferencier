use std::future::Future;
use std::sync::Arc;

use conferencier::{ConfigScope, ConfigStore, ConfigTable, Result, SharedScope, toml::Table};
use tokio::sync::RwLock;

struct ManualScope;

impl ConfigScope for ManualScope {
    fn from_store(_store: &ConfigStore) -> Result<SharedScope<Self>> {
        Ok(Arc::new(RwLock::new(Self)))
    }

    fn load(
        _scope: &SharedScope<Self>,
        _store: &ConfigStore,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    fn save(
        _scope: &SharedScope<Self>,
        _store: &ConfigStore,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
}

struct ManualTable;

impl ConfigTable for ManualTable {
    fn from_table(_table_path: &str, _table: Table) -> Result<Self> {
        Ok(Self)
    }

    fn to_table(&self, _table_path: &str) -> Result<Table> {
        Ok(Table::new())
    }
}

fn main() {}
