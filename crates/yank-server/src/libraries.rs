//! Reuse SQLite connections for active accounts without pinning every tenant in memory.
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use yank_core::Store;

use crate::{ApiResult, internal};

/// Bounds both simultaneous blocking jobs and the number of retained SQLite page caches.
const MAX_CONNECTIONS: usize = 16;

pub(super) struct Libraries {
    directory: PathBuf,
    idle: Mutex<VecDeque<(String, Store)>>,
    opening: Mutex<()>,
    workers: Arc<Semaphore>,
}

impl Libraries {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            idle: Mutex::default(),
            opening: Mutex::default(),
            workers: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        }
    }

    fn take_idle(&self, user: &str) -> ApiResult<Option<Store>> {
        let mut idle = self.idle.lock().map_err(|_| internal())?;
        Ok(idle
            .iter()
            .rposition(|(id, _)| id == user)
            .and_then(|at| idle.remove(at))
            .map(|(_, store)| store))
    }

    fn checkout(&self, user: &str) -> ApiResult<Store> {
        if let Some(store) = self.take_idle(user)? {
            return Ok(store);
        }
        // First open may migrate the file. An already open connection can keep serving
        // reads or writes while another account initializes its schema.
        let _opening = self.opening.lock().map_err(|_| internal())?;
        match self.take_idle(user)? {
            Some(store) => Ok(store),
            None => Ok(Store::open(self.directory.join(format!("{user}.sqlite3")))?),
        }
    }

    pub(super) async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        user: String,
        run: impl FnOnce(&Store) -> yank_core::Result<T> + Send + 'static,
    ) -> ApiResult<T> {
        let permit = self
            .workers
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| internal())?;
        let libraries = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let store = libraries.checkout(&user)?;
            let value = run(&store)?;
            let evicted = {
                let mut idle = libraries.idle.lock().map_err(|_| internal())?;
                idle.push_back((user, store));
                if idle.len() > MAX_CONNECTIONS {
                    idle.pop_front()
                } else {
                    None
                }
            };
            // Closing SQLite can checkpoint its WAL. Do that outside the cache lock.
            drop(evicted);
            Ok(value)
        })
        .await
        .map_err(|_| internal())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use yank_core::Clip;

    #[tokio::test]
    async fn concurrent_requests_share_only_their_own_tenant_data() {
        let directory = tempfile::tempdir().unwrap();
        let libraries = Arc::new(Libraries::new(directory.path().into()));
        let (ready, running) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let first = libraries.clone();
        let task = tokio::spawn(async move {
            first
                .run("a".into(), move |db| {
                    db.capture(Clip::text("device", "private"), "device")?;
                    ready.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                    db.stats()
                })
                .await
        });
        running.await.unwrap();
        for (user, expected) in [("b", 0), ("a", 1)] {
            let stats = libraries
                .run(user.into(), Store::stats)
                .await
                .map_err(|e| e.1)
                .unwrap();
            assert_eq!(stats.clips, expected);
        }
        release.send(()).unwrap();
        assert_eq!(task.await.unwrap().map_err(|e| e.1).unwrap().clips, 1);
    }

    #[tokio::test]
    async fn evicted_library_reopens_with_saved_history() {
        let directory = tempfile::tempdir().unwrap();
        let libraries = Arc::new(Libraries::new(directory.path().into()));
        libraries
            .run("first".into(), |db| {
                db.capture(Clip::text("device", "keep me"), "device")
            })
            .await
            .map_err(|e| e.1)
            .unwrap();
        for id in 0..MAX_CONNECTIONS {
            libraries
                .run(id.to_string(), Store::stats)
                .await
                .map_err(|e| e.1)
                .unwrap();
        }
        {
            let idle = libraries.idle.lock().unwrap();
            assert_eq!(idle.len(), MAX_CONNECTIONS);
            assert!(idle.iter().all(|(id, _)| id != "first"));
        }
        let stats = libraries
            .run("first".into(), Store::stats)
            .await
            .map_err(|e| e.1)
            .unwrap();
        assert_eq!(stats.clips, 1);
    }
}
